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
      <aside>
        <h2>Classes</h2>
        {#key summary}
          <Tree view="classes" onSelect={navigate} />
        {/key}
      </aside>
      <section class="content">
        {#if selected}
          <button onclick={back} disabled={history.length === 0}>← Back</button>
          <TermPage name={selected} onNavigate={navigate} />
        {:else}
          <p>
            {summary.formulaCount.toLocaleString()} formulas in {summary.files.length} files.
            Load time: {summary.loadMs.toFixed(0)} ms.
          </p>
          <table>
            <thead>
              <tr><th>File</th><th>Role</th><th>Formulas</th><th>Syntax errors</th></tr>
            </thead>
            <tbody>
              {#each summary.files as file (file.path)}
                <tr>
                  <td class="path">{file.path}</td>
                  <td>{file.role}</td>
                  <td class="number">{file.formulas.toLocaleString()}</td>
                  <td class="number">{file.syntaxErrors}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>
    </div>
  {:else if !error}
    <p class="hint">Open a project.toml file to start.</p>
  {/if}
</main>
