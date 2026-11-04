<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import SearchIcon from '@lucide/svelte/icons/search'
  import type { FormulaRef, Location, NameHit, QueryOutcome, TextHit } from './types'

  let { onNavigate, onOpen }: { onNavigate: (name: string) => void; onOpen: (location: Location) => void } =
    $props()

  type Mode = 'names' | 'text' | 'query'
  let input: HTMLInputElement
  let container: HTMLDivElement
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

  function onPointer(event: PointerEvent) {
    if (open && !container.contains(event.target as Node)) open = false
  }

  function fileName(path: string) {
    return path.split('/').pop()
  }
</script>

<svelte:window onkeydown={onKey} onpointerdown={onPointer} />

<div class="search" bind:this={container}>
  <label class="box">
    <SearchIcon class="ico muted" />
    <input
      bind:this={input}
      bind:value
      aria-label="Search"
      placeholder="Search names, text, or (query ?X)"
      onfocus={() => (open = true)}
      onkeydown={(e) => e.key === 'Enter' && submit()}
    />
    <select aria-label="Search mode" bind:value={chosen} disabled={mode === 'query'}>
      <option value="names">Names</option>
      <option value="text">Text</option>
      {#if mode === 'query'}
        <option value="query">Query</option>
      {/if}
    </select>
    <kbd class="muted">Ctrl P</kbd>
  </label>

  {#if open && value.trim()}
    <div class="results">
      {#if error}
        <p class="error">{error}</p>
      {:else if mode === 'names'}
        {#each names as hit (hit.name)}
          <button class="plain row" onclick={() => pickName(hit.name)}>
            <span class="mono">{hit.name}</span>
            <span class="kinds">{hit.kinds.join(', ')}</span>
          </button>
        {:else}
          <p class="muted">No names match.</p>
        {/each}
      {:else if mode === 'text'}
        {#if texts === null}
          <p class="muted">Push Enter to search the text in strings.</p>
        {:else}
          {#each texts as hit, i (i)}
            <button class="plain row" onclick={() => openFormula(hit.formula)}>
              <span class="mono">{hit.term ?? ''}</span>
              <span class="kinds mono">{fileName(hit.formula.file)}:{hit.formula.line}</span>
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
        <table class="mono">
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
                      <button class="plain link" onclick={() => pickName(v)}>{v}</button>
                    {:else}
                      {v}
                    {/if}
                  </td>
                {/each}
                <td>
                  {#each row.support as f, j (j)}
                    <button class="plain link muted" onclick={() => openFormula(f)}>
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
    width: 100%;
    max-width: 560px;
  }
  .box {
    height: 28px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 4px 0 10px;
    border: 1px solid var(--border-control);
    border-radius: 6px;
    background: var(--bg);
  }
  .box:focus-within {
    border-color: var(--accent);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0;
    border: none;
    background: transparent;
    outline: none;
  }
  select {
    height: 22px;
    padding: 0 4px;
    border: none;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 11px;
    padding-right: 6px;
  }
  .results {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 30;
    max-height: 60vh;
    overflow: auto;
    padding: 4px 0;
    background: var(--surface);
    border: 1px solid var(--border-control);
    border-radius: 6px;
    box-shadow: var(--shadow);
  }
  .row {
    width: 100%;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 12px;
    padding: 4px 10px;
    text-align: left;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .kinds {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
  .snippet {
    flex-basis: 100%;
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  p {
    margin: 4px 10px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th,
  td {
    text-align: left;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  th {
    font-family: var(--font-sans);
    font-weight: 500;
    color: var(--muted);
  }
  .link {
    padding-right: 8px;
    color: var(--accent);
  }
  .link.muted {
    color: var(--muted);
  }
</style>
