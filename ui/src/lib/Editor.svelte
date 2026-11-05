<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { basicSetup } from 'codemirror'
  import { Annotation, EditorState, Prec, Text, type Transaction } from '@codemirror/state'
  import { EditorView, keymap } from '@codemirror/view'
  import { listen } from '@tauri-apps/api/event'
  import { forEachDiagnostic, setDiagnostics, type Diagnostic as LintDiagnostic } from '@codemirror/lint'
  import { forceParsing } from '@codemirror/language'
  import { kif, kifHighlight } from './kif'
  import type { Diagnostic, DiagnosticsChanged, EditOutcome, FileContent, FileState, Location } from './types'

  let {
    content,
    line,
    onEdit,
    onOpen,
    onLeave,
  }: {
    content: FileContent
    line: number
    onEdit?: () => void
    onOpen?: (location: Location) => void
    onLeave?: (file: number, line: number) => void // 關閉前回報游標所在的行
  } = $props()

  let container: HTMLDivElement
  let view: EditorView | null = null
  let queue: Promise<void> = Promise.resolve()

  let version = $state(0)
  let savedVersion = $state(0)
  let errorCount = $state(0)
  let warningCount = $state(0)
  let message = $state<string | null>(null)
  let resyncs = $state(0)

  // 後端傳回的全文：套用時不再送回後端
  const fromBackend = Annotation.define<boolean>()
  const encoder = new TextEncoder()

  // CodeMirror 的位置是 UTF-16；後端使用 UTF-8 位元組
  function byteOffset(doc: Text, pos: number) {
    return encoder.encode(doc.sliceString(0, pos)).length
  }

  function utf16Offsets(text: string, bytes: number[]) {
    const targets = [...new Set(bytes)].sort((a, b) => a - b)
    const result = new Map<number, number>()
    let byte = 0
    let i = 0
    for (const target of targets) {
      while (byte < target && i < text.length) {
        const cp = text.codePointAt(i)!
        byte += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4
        i += cp > 0xffff ? 2 : 1
      }
      result.set(target, i)
    }
    return result
  }

  function toLint(list: Diagnostic[]): LintDiagnostic[] {
    if (!view) return []
    const offsets = utf16Offsets(view.state.doc.toString(), list.flatMap((d) => [d.from, d.to]))
    return list.map((d) => ({
      from: offsets.get(d.from)!,
      to: offsets.get(d.to)!,
      severity: d.severity,
      message: `${d.code} ${d.message}`,
      // 例如 E004 的 domain 宣告：跳到造成問題的公式
      actions: d.related.slice(0, 1).map((location) => ({
        name: 'Open source',
        apply: () => onOpen?.(location),
      })),
    }))
  }

  function display(list: LintDiagnostic[]) {
    if (!view) return
    errorCount = list.filter((d) => d.severity === 'error').length
    warningCount = list.filter((d) => d.severity === 'warning').length
    view.dispatch(setDiagnostics(view.state, list))
  }

  // 檢查結果：只接受與目前版本相同者（§14.4）
  function showDiagnostics(list: Diagnostic[]) {
    display(toLint(list))
  }

  // 打字時立即顯示語法錯誤；其他診斷由 CodeMirror 隨修改移動位置，等檢查結果送來再更新
  function showSyntaxErrors(errors: Diagnostic[]) {
    if (!view) return
    const kept: LintDiagnostic[] = []
    forEachDiagnostic(view.state, (d, from, to) => {
      if (!d.message.startsWith('E001 ')) kept.push({ ...d, from, to })
    })
    display([...kept, ...toLint(errors)])
  }

  // 一筆交易中的所有修改都以修改前的文件為準，與 LSP 和後端 apply_edits 的規則相同
  function toEdits(tr: Transaction) {
    const doc = tr.startState.doc
    const edits: { from: number; to: number; insert: string }[] = []
    tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
      edits.push({ from: byteOffset(doc, fromA), to: byteOffset(doc, toA), insert: inserted.toString() })
    })
    return edits
  }

  async function send(file: number, edits: ReturnType<typeof toEdits>) {
    try {
      const result = await invoke<EditOutcome>('apply_edits', { file, version, edits })
      if (file !== content.id || !view) return
      version = result.version
      if (result.status === 'applied') {
        showSyntaxErrors(result.errors)
      } else {
        resyncs += 1
        replaceText(result.text)
      }
      message = null
    } catch (e) {
      message = String(e)
    }
  }

  // 以最少的變動把編輯器內容換成後端的全文，游標位置得以保留
  function replaceText(text: string) {
    if (!view) return
    const old = view.state.doc.toString()
    let start = 0
    while (start < old.length && start < text.length && old[start] === text[start]) start++
    let oldEnd = old.length
    let newEnd = text.length
    while (oldEnd > start && newEnd > start && old[oldEnd - 1] === text[newEnd - 1]) {
      oldEnd--
      newEnd--
    }
    view.dispatch({
      changes: { from: start, to: oldEnd, insert: text.slice(start, newEnd) },
      annotations: fromBackend.of(true),
    })
  }

  // 復原、重做與格式化由後端執行，再把結果套到編輯器；與表單共用同一份歷史（§8.5）
  async function runBackend(command: string, args: Record<string, unknown> = {}) {
    if (content.readOnly) return
    const file = content.id
    await queue
    try {
      const result = await invoke<FileState | null>(command, { file, ...args })
      if (!result || file !== content.id) return
      replaceText(result.text)
      version = result.version
      showSyntaxErrors(result.errors)
      message = null
    } catch (e) {
      message = String(e)
    }
  }

  function formatFormula() {
    if (!view) return
    const offset = byteOffset(view.state.doc, view.state.selection.main.head)
    runBackend('format_file', { offset })
  }

  async function save() {
    if (content.readOnly) return
    const file = content.id
    await queue
    try {
      savedVersion = await invoke<number>('save_file', { file })
      message = 'Saved.'
    } catch (e) {
      message = String(e)
    }
  }

  // 顏色取自 app.css 的設計代號（docs/ui-design.md 第 3 節）
  const theme = EditorView.theme({
    '&': { height: '100%', backgroundColor: 'var(--surface)', color: 'var(--text)' },
    '.cm-content': { fontFamily: 'var(--font-mono)', fontSize: '13px', caretColor: 'var(--text)' },
    '.cm-cursor': { borderLeftColor: 'var(--text)' },
    '.cm-gutters': { backgroundColor: 'var(--surface)', color: 'var(--muted)', border: 'none' },
    '.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--selected) 40%, transparent)' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text)' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
      backgroundColor: 'var(--selected) !important',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--surface)',
      border: '1px solid var(--border-control)',
      borderRadius: '6px',
      boxShadow: 'var(--shadow)',
    },
    '.cm-diagnostic-error': { borderLeftColor: 'var(--error)' },
    '.cm-lintRange-error': { textDecorationColor: 'var(--error)' },
  })

  // 換檔案時重建編輯器狀態
  $effect(() => {
    const file = content.id
    version = content.version
    savedVersion = content.savedVersion
    resyncs = 0
    message = null
    const state = EditorState.create({
      doc: content.text,
      extensions: [
        Prec.highest(
          keymap.of([
            { key: 'Mod-s', preventDefault: true, run: () => (save(), true) },
            { key: 'Mod-z', preventDefault: true, run: () => (runBackend('undo'), true) },
            { key: 'Mod-y', preventDefault: true, run: () => (runBackend('redo'), true) },
            { key: 'Mod-Shift-z', preventDefault: true, run: () => (runBackend('redo'), true) },
            { key: 'Shift-Alt-f', preventDefault: true, run: () => (runBackend('format_file'), true) },
            { key: 'Mod-k Mod-f', preventDefault: true, run: () => (formatFormula(), true) },
          ]),
        ),
        basicSetup,
        kif,
        kifHighlight,
        theme,
        EditorState.readOnly.of(content.readOnly),
        EditorView.editable.of(!content.readOnly),
        EditorView.updateListener.of((update) => {
          for (const tr of update.transactions) {
            if (!tr.docChanged || tr.annotation(fromBackend)) continue
            const edits = toEdits(tr)
            onEdit?.()
            queue = queue.then(() => send(file, edits))
          }
        }),
      ],
    })
    view = new EditorView({ state, parent: container })
    // CodeMirror 預設只解析畫面附近的內容，捲動時才補上標色；開檔時先解析全文，最多花 500 ms
    forceParsing(view, view.state.doc.length, 500)
    showDiagnostics(content.errors)
    const unlisten = listen<DiagnosticsChanged>('diagnostics_changed', (event) => {
      const update = event.payload
      if (update.file === file && update.version === version) showDiagnostics(update.diagnostics)
    })
    return () => {
      unlisten.then((stop) => stop())
      if (view) onLeave?.(file, view.state.doc.lineAt(view.state.selection.main.head).number)
      view?.destroy()
      view = null
    }
  })

  // 跳到指定行，置中顯示
  $effect(() => {
    const target = line
    void content
    if (!view) return
    const doc = view.state.doc
    const pos = doc.line(Math.min(Math.max(target, 1), doc.lines)).from
    view.dispatch({
      selection: { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    })
  })
</script>

<div class="editor-view">
  <div class="toolbar">
    <span class="path mono muted">{content.path}</span>
    {#if content.readOnly}
      <span class="badge">read-only</span>
    {:else}
      {#if version !== savedVersion}
        <span class="badge">not saved</span>
      {/if}
      <span class="spacer"></span>
      <button class="small" onclick={formatFormula} title="Ctrl+K Ctrl+F">Format Formula</button>
      <button class="small" onclick={() => runBackend('format_file')} title="Shift+Alt+F">Format File</button>
      <button class="small primary" onclick={save} disabled={version === savedVersion} title="Ctrl+S">Save</button>
    {/if}
  </div>
  <div class="editor" bind:this={container}></div>
  <div class="status">
    <span class:error={errorCount > 0}>Errors: {errorCount}</span>
    <span>Warnings: {warningCount}</span>
    {#if resyncs > 0}
      <span>Resynchronized with the backend: {resyncs} times.</span>
    {/if}
    {#if message}
      <span>{message}</span>
    {/if}
  </div>
</div>

<style>
  .editor-view {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .path {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .editor {
    flex: 1;
    min-height: 0;
  }
  .editor :global(.cm-editor) {
    height: 100%;
  }
  .status {
    display: flex;
    gap: 16px;
    padding: 2px 12px;
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--border-subtle);
  }
</style>
