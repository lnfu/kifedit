<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import type { FormulaRef, Group, TermPageData } from './types'

  let { name, onNavigate }: { name: string; onNavigate: (name: string) => void } = $props()

  let page = $state<TermPageData | null>(null)
  let error = $state<string | null>(null)

  $effect(() => {
    const requested = name
    error = null
    invoke<TermPageData>('term_page', { name: requested })
      .then((result) => {
        if (requested === name) page = result
      })
      .catch((e) => (error = String(e)))
  })

  function byLanguage(entries: TermPageData['documentation']) {
    const groups = new Map<string, TermPageData['documentation']>()
    for (const entry of entries) {
      groups.set(entry.language, [...(groups.get(entry.language) ?? []), entry])
    }
    return [...groups]
  }

  function fileName(path: string) {
    return path.split('/').pop()
  }
</script>

{#snippet links(names: string[])}
  {#if names.length === 0}
    <span class="none">None</span>
  {:else}
    {#each names as n (n)}
      <button class="link" onclick={() => onNavigate(n)}>{n}</button>
    {/each}
  {/if}
{/snippet}

{#snippet formulas(items: FormulaRef[])}
  {#each items as f, i (i)}
    <div class="formula">
      <span class="location">{fileName(f.file)}:{f.line}</span>
      <pre>{f.text}</pre>
    </div>
  {/each}
{/snippet}

{#snippet group(title: string, g: Group)}
  <h3>{title} ({g.total})</h3>
  {#if g.total === 0}
    <p class="none">None</p>
  {:else}
    {@render formulas(g.items)}
    {#if g.total > g.items.length}
      <p class="none">The page shows {g.items.length} of {g.total} formulas.</p>
    {/if}
  {/if}
{/snippet}

{#if error}
  <p class="error">{error}</p>
{:else if page}
  <article>
    <h1>{page.name}</h1>
    {#if !page.exists}
      <p class="error">No formula in the project uses this term.</p>
    {:else}
      <p class="kinds">{page.kinds.join(', ') || 'unknown kind'}</p>

      <section>
        <h2>Documentation</h2>
        {#if page.documentation.length === 0}
          <p class="none">None</p>
        {/if}
        {#each byLanguage(page.documentation) as [language, entries] (language)}
          <h3>{language}</h3>
          {#each entries as entry, i (i)}
            <p class:format={entry.relation === 'termFormat'}>{entry.text}</p>
          {/each}
        {/each}
      </section>

      <section>
        <h2>Hierarchy</h2>
        {#if page.hierarchy}
          <dl>
            <dt>Parents ({page.hierarchy})</dt>
            <dd>{@render links(page.parents)}</dd>
            <dt>All ancestors</dt>
            <dd>{@render links(page.ancestors)}</dd>
            <dt>Children</dt>
            <dd>{@render links(page.children)}</dd>
          </dl>
        {/if}
        <dl>
          {#if page.types.length > 0}
            <dt>Instance of</dt>
            <dd>{@render links(page.types)}</dd>
          {/if}
          {#if page.instances.length > 0}
            <dt>Instances</dt>
            <dd>{@render links(page.instances)}</dd>
          {/if}
        </dl>
      </section>

      {#if page.signature.length > 0 || page.examples.length > 0}
        <section>
          <h2>Signature</h2>
          {#if page.arity !== null}
            <p>Arity: {page.arity}</p>
          {/if}
          {@render formulas(page.signature)}
          {#if page.examples.length > 0}
            <h3>Examples</h3>
            {@render formulas(page.examples)}
          {/if}
        </section>
      {/if}

      <section>
        <h2>Axioms</h2>
        {@render group('Declarations', page.declarations)}
        {@render group('As an argument', page.asArgument)}
        {@render group('In rules', page.inRules)}
      </section>
    {/if}
  </article>
{/if}

<style>
  h1 {
    margin: 0;
    font-size: 20px;
  }
  h2 {
    font-size: 15px;
    margin: 20px 0 8px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    font-size: 13px;
    margin: 12px 0 4px;
    color: var(--muted);
  }
  .kinds,
  .none {
    color: var(--muted);
  }
  .format {
    font-style: italic;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 0 0 8px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  .link {
    border: none;
    background: none;
    padding: 0 8px 0 0;
    color: #0969da;
    cursor: pointer;
    font: inherit;
  }
  .formula {
    margin-bottom: 6px;
  }
  .location {
    font-size: 12px;
    color: var(--muted);
  }
  pre {
    margin: 2px 0 0;
    white-space: pre-wrap;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
</style>
