<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { open } from '@tauri-apps/plugin-dialog'
  import Tree from './lib/Tree.svelte'

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
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
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
          <Tree view="classes" />
        {/key}
      </aside>
      <section>
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
      </section>
    </div>
  {:else if !error}
    <p class="hint">Open a project.toml file to start.</p>
  {/if}
</main>
