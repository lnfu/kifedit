<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { open } from '@tauri-apps/plugin-dialog'
  import Tree from './lib/Tree.svelte'
  import TermPage from './lib/TermPage.svelte'

  type FileSummary = {
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
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  function navigate(name: string) {
    if (selected === name) return
    if (selected) history = [...history, selected]
    selected = name
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
    {/if}
  </header>

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
            <Tree {view} onSelect={navigate} />
          {/key}
        {/key}
      </aside>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resizer" onpointerdown={startResize}></div>
      <section class="content">
        {#if selected}
          <button onclick={back} disabled={history.length === 0}>← Back</button>
          <TermPage name={selected} onNavigate={navigate} />
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
