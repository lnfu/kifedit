<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import type {
    EditableFile,
    FormInput,
    FormulaRef,
    Group,
    Location,
    Preview,
    TermPageData,
    ValueInput,
  } from './types'

  let {
    name,
    editableFiles,
    onNavigate,
    onOpen,
  }: {
    name: string
    editableFiles: EditableFile[]
    onNavigate: (name: string) => void
    onOpen: (location: Location) => void
  } = $props()

  let page = $state<TermPageData | null>(null)
  let error = $state<string | null>(null)
  let refresh = $state(0)

  // 表單：一次修改一條公式；送出前先預覽（§8.3）
  let editing = $state<string | null>(null)
  let draft = $state<string[]>([])
  let pending = $state<{ input: FormInput; preview: Preview } | null>(null)
  let formError = $state<string | null>(null)
  let lastFile = $state<number | null>(null)

  let addFile = $state<number | null>(null)
  let addRelation = $state('')
  let addArgs = $state<ValueInput[]>([])

  const key = (f: FormulaRef) => `${f.fileId}:${f.index}`

  function startEdit(f: FormulaRef) {
    editing = key(f)
    draft = f.args.map((a) => a.text)
    formError = null
  }

  async function propose(input: FormInput) {
    formError = null
    try {
      const preview = await invoke<Preview>('preview_form', { input })
      pending = { input, preview }
    } catch (e) {
      formError = String(e)
    }
  }

  function submitEdit(f: FormulaRef) {
    const i = f.args.findIndex((a, i) => a.kind !== 'other' && a.text !== draft[i])
    if (i < 0) {
      editing = null
      return
    }
    const formula = { file: f.fileId, index: f.index }
    const arg = f.args[i]
    propose(
      arg.kind === 'string'
        ? { type: 'changeString', formula, argument: i, text: draft[i] }
        : { type: 'changeArgument', formula, argument: i, value: { kind: arg.kind as ValueInput['kind'], text: draft[i] } },
    )
  }

  function submitAdd() {
    if (addFile === null) return
    propose({ type: 'addFact', file: addFile, relation: addRelation, args: addArgs })
  }

  async function confirm() {
    if (!pending) return
    try {
      const applied = await invoke<{ file: number; version: number }>('apply_form', { input: pending.input })
      lastFile = applied.file
      pending = null
      editing = null
      refresh += 1
    } catch (e) {
      formError = String(e)
      pending = null
    }
  }

  async function undoLast() {
    if (lastFile === null) return
    try {
      await invoke('undo', { file: lastFile })
      refresh += 1
    } catch (e) {
      formError = String(e)
    }
  }

  $effect(() => {
    addFile = editableFiles[0]?.id ?? null
    addRelation = ''
    addArgs = [
      { kind: 'symbol', text: name },
      { kind: 'symbol', text: '' },
    ]
  })

  $effect(() => {
    const requested = name
    void refresh
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

{#snippet actions(f: FormulaRef)}
  {#if f.editable}
    {#if f.args.some((a) => a.kind !== 'other')}
      <button class="action" title="Edit" onclick={() => startEdit(f)}>✎</button>
    {/if}
    <button
      class="action"
      title="Delete"
      onclick={() => propose({ type: 'deleteFact', formula: { file: f.fileId, index: f.index } })}>✕</button>
  {/if}
{/snippet}

{#snippet editor(f: FormulaRef)}
  <div class="edit">
    {#each f.args as arg, i (i)}
      {#if arg.kind === 'other'}
        <span class="none">(nested term)</span>
      {:else if arg.kind === 'string'}
        <textarea rows="2" bind:value={draft[i]}></textarea>
      {:else}
        <input bind:value={draft[i]} />
      {/if}
    {/each}
    <div>
      <button onclick={() => submitEdit(f)}>Preview</button>
      <button onclick={() => (editing = null)}>Cancel</button>
    </div>
  </div>
{/snippet}

{#snippet formulas(items: FormulaRef[])}
  {#each items as f, i (i)}
    <div class="formula">
      <button class="location" onclick={() => onOpen({ file: f.fileId, line: f.line })}>
        {fileName(f.file)}:{f.line}
      </button>
      {@render actions(f)}
      {#if editing === key(f)}
        {@render editor(f)}
      {:else}
        <pre>{f.text}</pre>
      {/if}
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

{#if pending}
  <div class="preview">
    <h3>Preview: line {pending.preview.line}</h3>
    {#each pending.preview.removed as l, i (i)}
      <pre class="removed">- {l}</pre>
    {/each}
    {#each pending.preview.added as l, i (i)}
      <pre class="added">+ {l}</pre>
    {/each}
    <button onclick={confirm}>Apply</button>
    <button onclick={() => (pending = null)}>Cancel</button>
  </div>
{/if}
{#if formError}
  <p class="error">{formError}</p>
{/if}
{#if lastFile !== null && !pending}
  <p class="none">The change is applied. It is not saved. <button onclick={undoLast}>Undo</button></p>
{/if}

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
            <div class="doc">
              {#if editing === key(entry.formula)}
                {@render editor(entry.formula)}
              {:else}
                <p class:format={entry.relation === 'termFormat'}>{entry.text}</p>
              {/if}
              {@render actions(entry.formula)}
            </div>
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

    {#if editableFiles.length > 0}
      <section>
        <h2>Add fact</h2>
        <div class="add">
          <select bind:value={addFile}>
            {#each editableFiles as f (f.id)}
              <option value={f.id}>{fileName(f.path)}</option>
            {/each}
          </select>
          <input placeholder="relation" bind:value={addRelation} />
          {#each addArgs as arg, i (i)}
            <select bind:value={arg.kind}>
              <option value="symbol">symbol</option>
              <option value="string">string</option>
              <option value="number">number</option>
            </select>
            <input bind:value={arg.text} />
          {/each}
          <button onclick={() => (addArgs = [...addArgs, { kind: 'symbol', text: '' }])}>+ Argument</button>
          {#if addArgs.length > 1}
            <button onclick={() => (addArgs = addArgs.slice(0, -1))}>− Argument</button>
          {/if}
          <button onclick={submitAdd} disabled={!addRelation}>Preview</button>
        </div>
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
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }
  .location:hover {
    text-decoration: underline;
  }
  .action {
    border: none;
    background: none;
    padding: 0 4px;
    color: var(--muted);
    cursor: pointer;
  }
  .action:hover {
    color: var(--text);
  }
  .doc {
    display: flex;
    gap: 4px;
    align-items: flex-start;
  }
  .doc p {
    margin: 4px 0;
  }
  .edit,
  .add {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    margin: 4px 0;
  }
  .edit textarea {
    width: 100%;
  }
  .preview {
    border: 1px solid var(--border);
    padding: 8px;
    margin-bottom: 12px;
  }
  .removed {
    color: var(--error);
  }
  .added {
    color: #1a7f37;
  }
  pre {
    margin: 2px 0 0;
    white-space: pre-wrap;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
</style>
