<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { basicSetup } from 'codemirror'
  import { Annotation, EditorState, Prec, Text, type Transaction } from '@codemirror/state'
  import { EditorView, keymap } from '@codemirror/view'
  import { setDiagnostics, type Diagnostic as LintDiagnostic } from '@codemirror/lint'
  import { forceParsing } from '@codemirror/language'
  import { kif } from './kif'
  import type { Diagnostic, EditOutcome, FileContent, FileState } from './types'

  let { content, line }: { content: FileContent; line: number } = $props()

  let container: HTMLDivElement
  let view: EditorView | null = null
  let queue: Promise<void> = Promise.resolve()

  let version = $state(0)
  let savedVersion = $state(0)
  let errorCount = $state(0)
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

  function showErrors(errors: Diagnostic[]) {
    errorCount = errors.length
    if (!view) return
    const text = view.state.doc.toString()
    const offsets = utf16Offsets(text, errors.flatMap((e) => [e.from, e.to]))
    const diagnostics: LintDiagnostic[] = errors.map((e) => ({
      from: offsets.get(e.from)!,
      to: offsets.get(e.to)!,
      severity: 'error',
      message: `${e.code} ${e.message}`,
    }))
    view.dispatch(setDiagnostics(view.state, diagnostics))
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
        showErrors(result.errors)
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
      showErrors(result.errors)
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
        EditorState.readOnly.of(content.readOnly),
        EditorView.editable.of(!content.readOnly),
        EditorView.updateListener.of((update) => {
          for (const tr of update.transactions) {
            if (!tr.docChanged || tr.annotation(fromBackend)) continue
            const edits = toEdits(tr)
            queue = queue.then(() => send(file, edits))
          }
        }),
      ],
    })
    view = new EditorView({ state, parent: container })
    // CodeMirror 預設只解析畫面附近的內容，捲動時才補上標色；開檔時先解析全文，最多花 500 ms
    forceParsing(view, view.state.doc.length, 500)
    showErrors(content.errors)
    return () => {
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

<div class="file">
  <span class="path">{content.path}</span>
  {#if content.readOnly}
    <span class="badge">read-only</span>
  {:else}
    <button onclick={save} disabled={version === savedVersion}>Save</button>
    <button onclick={formatFormula} title="Ctrl+K Ctrl+F">Format Formula</button>
    <button onclick={() => runBackend('format_file')} title="Shift+Alt+F">Format File</button>
    {#if version !== savedVersion}
      <span class="badge">not saved</span>
    {/if}
  {/if}
</div>
<div class="editor" bind:this={container}></div>
<div class="status">
  <span>Syntax errors: {errorCount}</span>
  {#if resyncs > 0}
    <span>Resynchronized with the backend: {resyncs} times.</span>
  {/if}
  {#if message}
    <span>{message}</span>
  {/if}
</div>

<style>
  .file {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 4px;
  }
  .badge {
    font-size: 12px;
    color: var(--muted);
    border: 1px solid var(--border);
    padding: 0 4px;
  }
  .editor {
    height: calc(100vh - 160px);
    border: 1px solid var(--border);
  }
  .editor :global(.cm-editor) {
    height: 100%;
  }
  .status {
    display: flex;
    gap: 16px;
    font-size: 12px;
    color: var(--muted);
    margin-top: 4px;
  }
</style>
