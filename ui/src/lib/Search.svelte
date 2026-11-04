<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import type { FormulaRef, Location, NameHit, QueryOutcome, TextHit } from './types'

  let { onNavigate, onOpen }: { onNavigate: (name: string) => void; onOpen: (location: Location) => void } =
    $props()

  type Mode = 'names' | 'text' | 'query'
  let input: HTMLInputElement
  let value = $state('')
  let chosen = $state<Mode>('names')
  let open = $state(false)
  let error = $state<string | null>(null)
  let names = $state<NameHit[]>([])
  let texts = $state<TextHit[] | null>(null)
  let outcome = $state<QueryOutcome | null>(null)

  // 以 ( 開頭的輸入是結構化查詢
  let mode = $derived<Mode>(value.trimStart().startsWith('(') ? 'query' : chosen)

  $effect(() => {
    void value
    texts = null
    outcome = null
  })

  // 名稱搜尋：停止輸入 150 ms 後送出
  let timer: ReturnType<typeof setTimeout> | undefined
  $effect(() => {
    const query = value.trim()
    if (mode !== 'names') return
    clearTimeout(timer)
    if (!query) {
      names = []
      return
    }
    timer = setTimeout(async () => {
      try {
        const result = await invoke<NameHit[]>('search_names', { query })
        if (query === value.trim()) names = result
        error = null
      } catch (e) {
        error = String(e)
      }
    }, 150)
  })

  async function submit() {
    const query = value.trim()
    if (!query) return
    open = true
    error = null
    try {
      if (mode === 'text') texts = await invoke<TextHit[]>('search_text', { query })
      if (mode === 'query') outcome = await invoke<QueryOutcome>('run_query', { query })
      if (mode === 'names' && names.length > 0) pickName(names[0].name)
    } catch (e) {
      error = String(e)
    }
  }

  function pickName(name: string) {
    open = false
    onNavigate(name)
  }

  function openFormula(f: FormulaRef) {
    open = false
    onOpen({ file: f.fileId, line: f.line })
  }

  function onKey(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'p') {
      event.preventDefault()
      input.focus()
      input.select()
      open = true
    } else if (event.key === 'Escape' && open) {
      open = false
    }
  }

  function fileName(path: string) {
    return path.split('/').pop()
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="search">
  <input
    bind:this={input}
    bind:value
    placeholder="Search (Ctrl+P). Start with ( for a query."
    onfocus={() => (open = true)}
    onkeydown={(e) => e.key === 'Enter' && submit()}
  />
  <select bind:value={chosen} disabled={mode === 'query'}>
    <option value="names">Names</option>
    <option value="text">Text</option>
    {#if mode === 'query'}
      <option value="query">Query</option>
    {/if}
  </select>

  {#if open && value.trim()}
    <div class="results">
      {#if error}
        <p class="error">{error}</p>
      {:else if mode === 'names'}
        {#each names as hit (hit.name)}
          <button class="row" onclick={() => pickName(hit.name)}>
            {hit.name} <span class="muted">{hit.kinds.join(', ')}</span>
          </button>
        {:else}
          <p class="muted">No names match.</p>
        {/each}
      {:else if mode === 'text'}
        {#if texts === null}
          <p class="muted">Push Enter to search the text in strings.</p>
        {:else}
          {#each texts as hit, i (i)}
            <button class="row" onclick={() => openFormula(hit.formula)}>
              <span class="muted">{fileName(hit.formula.file)}:{hit.formula.line}</span>
              {hit.term ?? ''}
              <span class="snippet">{hit.text}</span>
            </button>
          {:else}
            <p class="muted">No strings contain this text.</p>
          {/each}
        {/if}
      {:else if outcome === null}
        <p class="muted">Push Enter to run the query.</p>
      {:else if outcome.status === 'needsProver'}
        <p class="muted">{outcome.message}</p>
      {:else if outcome.rows.length === 0}
        <p class="muted">The query has no answers.</p>
      {:else}
        <p class="muted">Answers: {outcome.rows.length}</p>
        <table>
          <thead>
            <tr>
              {#each outcome.columns as c (c)}<th>{c}</th>{/each}
              <th>Source</th>
            </tr>
          </thead>
          <tbody>
            {#each outcome.rows as row, i (i)}
              <tr>
                {#each row.values as v, j (j)}
                  <td>
                    {#if /^[A-Za-z]/.test(v)}
                      <button class="link" onclick={() => pickName(v)}>{v}</button>
                    {:else}
                      {v}
                    {/if}
                  </td>
                {/each}
                <td>
                  {#each row.support as f, j (j)}
                    <button class="link muted" onclick={() => openFormula(f)}>
                      {fileName(f.file)}:{f.line}
                    </button>
                  {/each}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  {/if}
</div>

<style>
  .search {
    position: relative;
    display: flex;
    gap: 4px;
    flex: 1;
    max-width: 640px;
  }
  input {
    flex: 1;
  }
  .results {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 10;
    max-height: 60vh;
    overflow: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    padding: 4px;
  }
  .row {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: none;
    padding: 4px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .row:hover {
    background: var(--border);
  }
  .snippet {
    display: block;
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
  }
  .error {
    color: var(--error);
  }
  p {
    margin: 4px;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    font-size: 12px;
  }
  th,
  td {
    text-align: left;
    padding: 2px 6px;
    border-bottom: 1px solid var(--border);
  }
  .link {
    border: none;
    background: none;
    padding: 0 6px 0 0;
    font: inherit;
    color: #0969da;
    cursor: pointer;
  }
</style>
