<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { open } from '@tauri-apps/plugin-dialog'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import Moon from '@lucide/svelte/icons/moon'
  import Sun from '@lucide/svelte/icons/sun'
  import X from '@lucide/svelte/icons/x'
  import Tree from './lib/Tree.svelte'
  import TermPage from './lib/TermPage.svelte'
  import Editor from './lib/Editor.svelte'
  import Search from './lib/Search.svelte'
  import ProblemsPanel from './lib/ProblemsPanel.svelte'
  import { clearProblems, problems, startProblems } from './lib/problems.svelte'
  import { theme, toggleTheme } from './lib/theme.svelte'
  import type { FileContent, FileSummary, FormatPlan, Location } from './lib/types'

  startProblems()

  type ProjectSummary = {
    name: string
    root: string
    files: FileSummary[]
    formulaCount: number
    loadMs: number
  }

  // 分頁與 VS Code 相同：預覽分頁（斜體）會被下一次開啟取代；雙擊或編輯後固定
  type Tab =
    | { kind: 'term'; name: string; preview: boolean }
    | { kind: 'file'; file: FileContent; line: number; preview: boolean }

  let summary = $state<ProjectSummary | null>(null)
  let error = $state<string | null>(null)
  let loading = $state(false)
  let tabs = $state<Tab[]>([])
  let active = $state(0)
  let current = $derived<Tab | undefined>(tabs[active])
  let selectedTerm = $derived(current?.kind === 'term' ? current.name : null)

  const views = [
    { id: 'classes', label: 'Classes' },
    { id: 'relations', label: 'Relations' },
    { id: 'attributes', label: 'Attributes' },
    { id: 'files', label: 'Files' },
  ]
  let view = $state('classes')

  // 版面配置記在 localStorage；無法使用時以預設值顯示
  function readNumber(key: string, fallback: number) {
    try {
      return Number(localStorage.getItem(key)) || fallback
    } catch {
      return fallback
    }
  }

  function store(key: string, value: string) {
    try {
      localStorage.setItem(key, value)
    } catch {}
  }

  const WIDTH_KEY = 'kifedit.sidebarWidth'
  const PANEL_KEY = 'kifedit.problemsHeight'
  const OPEN_KEY = 'kifedit.problemsOpen'
  let sidebarWidth = $state(readNumber(WIDTH_KEY, 300))
  let panelHeight = $state(readNumber(PANEL_KEY, 200))
  let panelOpen = $state(readNumber(OPEN_KEY, 1) === 1)

  function togglePanel() {
    panelOpen = !panelOpen
    store(OPEN_KEY, panelOpen ? '1' : '2')
  }

  function startPanelResize(event: PointerEvent) {
    const startY = event.clientY
    const startHeight = panelHeight
    const move = (e: PointerEvent) => {
      panelHeight = Math.min(window.innerHeight - 200, Math.max(80, startHeight - (e.clientY - startY)))
    }
    const stop = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', stop)
      store(PANEL_KEY, String(panelHeight))
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', stop)
  }

  function onKey(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 'm') {
      event.preventDefault()
      togglePanel()
    }
  }

  // 狀態列：editable 檔的錯誤與警告數
  let counts = $derived.by(() => {
    let errors = 0
    let warnings = 0
    for (const file of summary?.files ?? []) {
      if (file.role !== 'editable') continue
      for (const d of problems.get(file.id) ?? []) {
        if (d.severity === 'error') errors += 1
        else if (d.severity === 'warning') warnings += 1
      }
    }
    return { errors, warnings }
  })

  function startResize(event: PointerEvent) {
    const startX = event.clientX
    const startWidth = sidebarWidth
    const move = (e: PointerEvent) => {
      sidebarWidth = Math.min(800, Math.max(200, startWidth + e.clientX - startX))
    }
    const stop = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', stop)
      store(WIDTH_KEY, String(sidebarWidth))
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', stop)
  }

  async function openProject() {
    const path = await open({
      title: 'Open project.toml',
      filters: [{ name: 'Project file', extensions: ['toml'] }],
    })
    if (typeof path !== 'string') return

    loading = true
    error = null
    clearProblems()
    try {
      summary = await invoke<ProjectSummary>('open_project', { path })
      tabs = []
      active = 0
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  function key(tab: Tab) {
    return tab.kind === 'term' ? `term:${tab.name}` : `file:${tab.file.id}`
  }

  function show(tab: Tab) {
    const existing = tabs.findIndex((t) => key(t) === key(tab))
    if (existing >= 0) {
      tabs[existing] = { ...tab, preview: tabs[existing].preview && tab.preview }
      active = existing
      return
    }
    const preview = tabs.findIndex((t) => t.preview)
    if (tab.preview && preview >= 0) {
      tabs[preview] = tab
      active = preview
    } else {
      tabs.push(tab)
      active = tabs.length - 1
    }
  }

  function navigate(name: string) {
    show({ kind: 'term', name, preview: true })
  }

  async function openLocation(location: Location) {
    error = null
    try {
      // 每次都向後端取最新內容與版本號，文字以後端為準
      const file = await invoke<FileContent>('file_text', { file: location.file })
      show({ kind: 'file', file, line: location.line, preview: true })
    } catch (e) {
      error = String(e)
    }
  }

  function pin(index: number) {
    if (tabs[index]) tabs[index].preview = false
  }

  function close(index: number) {
    tabs.splice(index, 1)
    if (active >= tabs.length) active = Math.max(0, tabs.length - 1)
    else if (index < active) active -= 1
  }

  function title(tab: Tab) {
    return tab.kind === 'term' ? tab.name : tab.file.path.split('/').pop()
  }

  // Format Project：先預覽會變動的檔案，確認後才套用（§8.6）
  let plan = $state<FormatPlan | null>(null)

  async function previewFormat() {
    error = null
    try {
      plan = await invoke<FormatPlan>('format_project_preview')
    } catch (e) {
      error = String(e)
    }
  }

  async function applyFormat() {
    try {
      const changed = await invoke<{ file: number; version: number }[]>('format_project')
      plan = null
      for (const [i, tab] of tabs.entries()) {
        if (tab.kind === 'file' && changed.some((c) => c.file === tab.file.id)) {
          tabs[i] = { ...tab, file: await invoke<FileContent>('file_text', { file: tab.file.id }) }
        }
      }
    } catch (e) {
      error = String(e)
    }
  }

  function fileName(path: string) {
    return path.split('/').pop()
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <header>
    <span class="brand">kifedit</span>
    {#if summary}
      <span class="muted">{summary.name}</span>
    {/if}
    <button class="small" onclick={openProject} disabled={loading}>
      <FolderOpen class="ico" />
      {loading ? 'Opening…' : 'Open Project'}
    </button>
    <div class="center">
      {#if summary}
        <Search onNavigate={navigate} onOpen={openLocation} />
      {/if}
    </div>
    {#if summary}
      <button onclick={previewFormat}>Format Project</button>
    {/if}
    <button
      class="icon"
      onclick={toggleTheme}
      aria-label={theme.current === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
      title={theme.current === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
    >
      {#if theme.current === 'dark'}<Sun class="ico" />{:else}<Moon class="ico" />{/if}
    </button>
  </header>

  {#if error}
    <p class="banner error">{error}</p>
  {/if}

  {#if plan}
    <div class="plan">
      {#if plan.changes.length === 0}
        <p>All editable files are formatted.</p>
      {:else}
        <p>Files that will change: {plan.changes.length}</p>
        <ul>
          {#each plan.changes as c (c.file)}
            <li class="mono">
              {fileName(c.path)}
              <span class="muted">
                from line {c.preview.line}: {c.preview.removed.length} lines change to {c.preview.added.length} lines
              </span>
            </li>
          {/each}
        </ul>
      {/if}
      {#each plan.skipped as path (path)}
        <p class="error">The file has syntax errors. It is not formatted: {path}</p>
      {/each}
      <div class="actions">
        {#if plan.changes.length > 0}
          <button class="primary" onclick={applyFormat}>Apply</button>
        {/if}
        <button onclick={() => (plan = null)}>Close</button>
      </div>
    </div>
  {/if}

  {#if summary}
    <div class="body">
      <aside style:width="{sidebarWidth}px">
        <div class="views" role="tablist">
          {#each views as v (v.id)}
            <button
              class="small"
              role="tab"
              aria-selected={view === v.id}
              class:current={view === v.id}
              onclick={() => (view = v.id)}>{v.label}</button
            >
          {/each}
        </div>
        <div class="tree">
          {#key summary}
            {#key view}
              <Tree {view} selected={selectedTerm} onSelect={navigate} onOpen={openLocation} />
            {/key}
          {/key}
        </div>
      </aside>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resizer" onpointerdown={startResize}></div>
      <main>
        <div class="tabs" role="tablist">
          {#each tabs as tab, i (key(tab))}
            <div class="tab" class:current={i === active} class:preview={tab.preview}>
              <button
                class="plain"
                role="tab"
                aria-selected={i === active}
                onclick={() => (active = i)}
                ondblclick={() => pin(i)}
                onauxclick={(e) => e.button === 1 && close(i)}>{title(tab)}</button
              >
              <button class="plain close" aria-label="Close" title="Close" onclick={() => close(i)}>
                <X class="ico" />
              </button>
            </div>
          {/each}
        </div>
        <div class="page">
          {#if current?.kind === 'term'}
            {#key key(current)}
              <TermPage
                name={current.name}
                editableFiles={summary.files.filter((f) => f.role === 'editable')}
                onNavigate={navigate}
                onOpen={openLocation}
              />
            {/key}
          {:else if current?.kind === 'file'}
            {#key current.file}
              <Editor content={current.file} line={current.line} onEdit={() => pin(active)} onOpen={openLocation} />
            {/key}
          {:else}
            <div class="empty muted">
              <p>Select a term in the tree, or push Ctrl+P to search.</p>
            </div>
          {/if}
        </div>
        {#if panelOpen}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="panel-resizer" onpointerdown={startPanelResize}></div>
          <div class="panel" style:height="{panelHeight}px">
            <ProblemsPanel files={summary.files} onOpen={openLocation} onClose={togglePanel} />
          </div>
        {/if}
      </main>
    </div>
    <footer>
      <button class="plain counts" onclick={togglePanel} title="Show or hide the problems (Ctrl+Shift+M)">
        <span class="error-count">● {counts.errors}</span>
        <span class="warning-count">▲ {counts.warnings}</span>
      </button>
      <span>{summary.files.length} files</span>
      <span>{summary.formulaCount.toLocaleString()} formulas</span>
      <span class="right">Load {summary.loadMs.toFixed(0)} ms</span>
    </footer>
  {:else if !error}
    <div class="empty muted">
      <p>Open a project.toml file to start.</p>
    </div>
  {/if}
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    height: 44px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    font-weight: 600;
  }
  .center {
    flex: 1;
    display: flex;
    justify-content: center;
    min-width: 0;
  }
  .banner {
    margin: 0;
    padding: 6px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .plan {
    position: absolute;
    top: 52px;
    right: 12px;
    z-index: 20;
    width: 480px;
    padding: 12px;
    background: var(--surface);
    border: 1px solid var(--border-control);
    border-radius: 6px;
    box-shadow: var(--shadow);
  }
  .plan p {
    margin: 0 0 8px;
  }
  .plan ul {
    margin: 0 0 8px;
    padding-left: 16px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  aside {
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .views {
    display: flex;
    gap: 2px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .views button {
    border-color: transparent;
    background: none;
    color: var(--muted);
  }
  .views button.current {
    background: var(--surface-2);
    color: var(--text);
    font-weight: 500;
  }
  .tree {
    flex: 1;
    overflow: auto;
    padding: 6px 0;
  }
  .resizer {
    flex: none;
    width: 4px;
    cursor: col-resize;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }
  .resizer:hover {
    background: var(--accent);
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .tabs {
    height: 34px;
    flex: none;
    display: flex;
    align-items: flex-end;
    padding-left: 4px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 12px;
    color: var(--muted);
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    white-space: nowrap;
  }
  .tab.current {
    background: var(--surface);
    border-color: var(--border);
    color: var(--text);
    font-weight: 500;
    margin-bottom: -1px;
    padding-bottom: 7px;
  }
  .tab.preview > button:first-child {
    font-style: italic;
  }
  .close {
    color: var(--muted);
    visibility: hidden;
  }
  .tab:hover .close,
  .tab.current .close {
    visibility: visible;
  }
  .page {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .empty {
    flex: 1;
    display: grid;
    place-items: center;
    height: 100%;
  }
  footer {
    height: 24px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 12px;
    font-size: 12px;
    color: var(--muted);
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }
  .right {
    margin-left: auto;
  }
  .counts {
    gap: 10px;
    font-size: 12px;
  }
  .error-count {
    color: var(--error);
  }
  .warning-count {
    color: var(--warning);
  }
  .panel {
    flex: none;
    min-height: 0;
  }
  .panel-resizer {
    flex: none;
    height: 4px;
    cursor: row-resize;
    border-top: 1px solid var(--border);
  }
  .panel-resizer:hover {
    background: var(--accent);
  }
</style>
