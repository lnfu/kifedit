<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { open } from '@tauri-apps/plugin-dialog'
  import Tree from './lib/Tree.svelte'
  import TermPage from './lib/TermPage.svelte'
  import Editor from './lib/Editor.svelte'
  import Search from './lib/Search.svelte'
  import type { FileContent, FormatPlan, Location } from './lib/types'

  type FileSummary = {
    id: number
    path: string
    role: 'library' | 'editable'
    formulas: number
    syntaxErrors: number
  }

  type ProjectSummary = {
    name: string
    root: string
    files: FileSummary[]
    formulaCount: number
    loadMs: number
  }

  let summary = $state<ProjectSummary | null>(null)
  let error = $state<string | null>(null)
  let loading = $state(false)
  let selected = $state<string | null>(null)
  let history = $state<string[]>([])

  const views = [
    { id: 'classes', label: 'Classes' },
    { id: 'relations', label: 'Relations' },
    { id: 'attributes', label: 'Attributes' },
    { id: 'files', label: 'Files' },
  ]
  let view = $state('classes')

  // 側欄寬度存在 localStorage；無法使用時以預設值顯示
  const WIDTH_KEY = 'kifedit.sidebarWidth'
  let sidebarWidth = $state(readWidth())

  function readWidth() {
    try {
      return Number(localStorage.getItem(WIDTH_KEY)) || 360
    } catch {
      return 360
    }
  }

  function startResize(event: PointerEvent) {
    const startX = event.clientX
    const startWidth = sidebarWidth
    const move = (e: PointerEvent) => {
      sidebarWidth = Math.min(800, Math.max(200, startWidth + e.clientX - startX))
    }
    const stop = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', stop)
      try {
        localStorage.setItem(WIDTH_KEY, String(sidebarWidth))
      } catch {}
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
    try {
      summary = await invoke<ProjectSummary>('open_project', { path })
      selected = null
      history = []
      file = null
      panel = 'term'
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  let panel = $state<'term' | 'editor'>('term')
  let file = $state<FileContent | null>(null)
  let line = $state(1)

  function navigate(name: string) {
    panel = 'term'
    if (selected === name) return
    if (selected) history = [...history, selected]
    selected = name
  }

  async function openLocation(location: Location) {
    error = null
    try {
      // 每次都向後端取最新內容與版本號，文字以後端為準
      file = await invoke<FileContent>('file_text', { file: location.file })
      line = location.line
      panel = 'editor'
    } catch (e) {
      error = String(e)
    }
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
      if (file && changed.some((c) => c.file === file!.id)) {
        file = await invoke<FileContent>('file_text', { file: file.id })
      }
    } catch (e) {
      error = String(e)
    }
  }

  function back() {
    selected = history.at(-1) ?? null
    history = history.slice(0, -1)
  }
</script>

<main>
  <header>
    <button onclick={openProject} disabled={loading}>
      {loading ? 'Opening…' : 'Open Project'}
    </button>
    {#if summary}
      <span class="project">{summary.name}</span>
      <Search onNavigate={navigate} onOpen={openLocation} />
      <button onclick={previewFormat}>Format Project</button>
    {/if}
  </header>

  {#if plan}
    <div class="plan">
      {#if plan.changes.length === 0}
        <p>All editable files are formatted.</p>
      {:else}
        <p>Files that will change: {plan.changes.length}</p>
        <ul>
          {#each plan.changes as c (c.file)}
            <li>
              {c.path}: from line {c.preview.line}, {c.preview.removed.length} lines change to {c.preview.added.length} lines
            </li>
          {/each}
        </ul>
      {/if}
      {#each plan.skipped as path (path)}
        <p class="error">The file has syntax errors. It is not formatted: {path}</p>
      {/each}
      {#if plan.changes.length > 0}
        <button onclick={applyFormat}>Apply</button>
      {/if}
      <button onclick={() => (plan = null)}>Close</button>
    </div>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if summary}
    <div class="layout">
      <aside style:width="{sidebarWidth}px">
        <nav class="tabs">
          {#each views as v (v.id)}
            <button class:active={view === v.id} onclick={() => (view = v.id)}>{v.label}</button>
          {/each}
        </nav>
        {#key summary}
          {#key view}
            <Tree {view} onSelect={navigate} onOpen={openLocation} />
          {/key}
        {/key}
      </aside>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resizer" onpointerdown={startResize}></div>
      <section class="content">
        {#if panel === 'editor' && file}
          {#if selected}
            <button onclick={() => (panel = 'term')}>← {selected}</button>
          {/if}
          <Editor content={file} {line} />
        {:else if selected}
          <button onclick={back} disabled={history.length === 0}>← Back</button>
          <TermPage
            name={selected}
            editableFiles={summary.files.filter((f) => f.role === 'editable')}
            onNavigate={navigate}
            onOpen={openLocation}
          />
        {:else}
          <p class="hint">
            {summary.formulaCount.toLocaleString()} formulas in {summary.files.length} files.
            Load time: {summary.loadMs.toFixed(0)} ms.
          </p>
          <p class="hint">Select a term in the tree.</p>
        {/if}
      </section>
    </div>
  {:else if !error}
    <p class="hint">Open a project.toml file to start.</p>
  {/if}
</main>
