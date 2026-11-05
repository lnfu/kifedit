<script lang="ts">
  import { call } from './bridge'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Plus from '@lucide/svelte/icons/plus'
  import Minus from '@lucide/svelte/icons/minus'
  import Trash from '@lucide/svelte/icons/trash-2'
  import Formula from './Formula.svelte'
  import type {
    EditableFile,
    FormInput,
    FormulaRef,
    Group,
    Preview,
    TermPageData,
    ValueInput,
  } from './types'

  let {
    name,
    editableFiles,
    version,
    onNavigate,
    onOpen,
  }: {
    name: string
    editableFiles: EditableFile[]
    version: number // 專案檔案有修改時加 1，重新載入頁面
    onNavigate: (name: string) => void
    onOpen: (location: { file: string; line: number }) => void
  } = $props()

  let page = $state<TermPageData | null>(null)
  let error = $state<string | null>(null)
  let refresh = $state(0)

  // 表單：一次修改一條公式；送出前先預覽，預覽顯示在修改的位置（§8.3）
  const ADD = 'add'
  let editing = $state<string | null>(null)
  let draft = $state<string[]>([])
  let pending = $state<{ input: FormInput; preview: Preview; anchor: string } | null>(null)
  let formError = $state<{ anchor: string; message: string } | null>(null)
  let applied = $state(false)

  let adding = $state(false)
  let addFile = $state<number | null>(null)
  let addRelation = $state('')
  let addArgs = $state<ValueInput[]>([])

  const key = (f: FormulaRef) => `${f.fileId}:${f.index}`

  function startEdit(f: FormulaRef) {
    editing = key(f)
    draft = f.args.map((a) => a.text)
    pending = null
    formError = null
  }

  function cancel() {
    editing = null
    pending = null
    formError = null
  }

  async function propose(input: FormInput, anchor: string) {
    formError = null
    try {
      const preview = await call<Preview>('kifedit/previewForm', { input })
      pending = { input, preview, anchor }
    } catch (e) {
      formError = { anchor, message: String(e) }
    }
  }

  function submitEdit(f: FormulaRef) {
    const i = f.args.findIndex((a, i) => a.kind !== 'other' && a.text !== draft[i])
    if (i < 0) {
      cancel()
      return
    }
    const formula = { file: f.fileId, index: f.index }
    const arg = f.args[i]
    propose(
      arg.kind === 'string'
        ? { type: 'changeString', formula, argument: i, text: draft[i] }
        : { type: 'changeArgument', formula, argument: i, value: { kind: arg.kind as ValueInput['kind'], text: draft[i] } },
      key(f),
    )
  }

  function submitAdd() {
    if (addFile === null) return
    propose({ type: 'addFact', file: addFile, relation: addRelation, args: addArgs }, ADD)
  }

  async function confirm() {
    if (!pending) return
    const anchor = pending.anchor
    try {
      await call('kifedit/applyForm', { input: pending.input })
      applied = true
      pending = null
      editing = null
      if (anchor === ADD) resetAdd()
      refresh += 1
    } catch (e) {
      formError = { anchor, message: String(e) }
      pending = null
    }
  }

  function resetAdd() {
    adding = false
    addFile = editableFiles[0]?.id ?? null
    addRelation = ''
    addArgs = [
      { kind: 'symbol', text: name },
      { kind: 'symbol', text: '' },
    ]
  }

  $effect(() => {
    void name
    resetAdd()
  })

  $effect(() => {
    const requested = name
    void refresh
    void version
    error = null
    call<TermPageData>('kifedit/termPage', { name: requested })
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

  function language(lang: string) {
    return lang.replace(/Language$/, '')
  }

  function fileName(path: string) {
    return path.split('/').pop()
  }
</script>

{#snippet links(names: string[])}
  {#if names.length === 0}
    <span class="muted">None</span>
  {:else}
    {#each names as n (n)}
      <button class="plain link mono" onclick={() => onNavigate(n)}>{n}</button>
    {/each}
  {/if}
{/snippet}

{#snippet actions(f: FormulaRef)}
  {#if f.editable}
    <span class="actions">
      {#if f.args.some((a) => a.kind !== 'other')}
        <button class="plain action" aria-label="Edit" title="Edit" onclick={() => startEdit(f)}>
          <Pencil class="ico" />
        </button>
      {/if}
      <button
        class="plain action"
        aria-label="Delete"
        title="Delete"
        onclick={() => propose({ type: 'deleteFact', formula: { file: f.fileId, index: f.index } }, key(f))}
      >
        <Trash class="ico" />
      </button>
    </span>
  {/if}
{/snippet}

{#snippet review(anchor: string)}
  {#if pending?.anchor === anchor}
    <div class="review">
      <div class="diff mono">
        {#each pending.preview.removed as l, i (i)}
          <div class="diff-del">- {l}</div>
        {/each}
        {#each pending.preview.added as l, i (i)}
          <div class="diff-add">+ {l}</div>
        {/each}
      </div>
      <div class="buttons">
        <button class="primary" onclick={confirm}>Apply</button>
        <button onclick={() => (pending = null)}>Cancel</button>
        <span class="muted small">
          Line {pending.preview.line} · {Math.max(pending.preview.removed.length, pending.preview.added.length)} lines change
        </span>
      </div>
    </div>
  {/if}
  {#if formError?.anchor === anchor}
    <p class="error small">{formError.message}</p>
  {/if}
{/snippet}

{#snippet editor(f: FormulaRef)}
  <div class="edit">
    <div class="fields">
      {#each f.args as arg, i (i)}
        {#if arg.kind === 'other'}
          <span class="muted small">(nested term)</span>
        {:else if arg.kind === 'string'}
          <textarea aria-label="Argument {i + 1}" rows="2" bind:value={draft[i]}></textarea>
        {:else}
          <input class="mono" aria-label="Argument {i + 1}" bind:value={draft[i]} />
        {/if}
      {/each}
    </div>
    {#if pending?.anchor !== key(f)}
      <div class="buttons">
        <button onclick={() => submitEdit(f)}>Preview</button>
        <button onclick={cancel}>Cancel</button>
      </div>
    {/if}
    {@render review(key(f))}
  </div>
{/snippet}

{#snippet formulas(items: FormulaRef[])}
  {#each items as f, i (i)}
    <div class="formula">
      <button class="plain location mono" onclick={() => onOpen({ file: f.file, line: f.line })}>
        {fileName(f.file)}:{f.line}
      </button>
      <div class="text">
        {#if editing === key(f)}
          {@render editor(f)}
        {:else}
          <Formula text={f.text} />
          {@render review(key(f))}
        {/if}
      </div>
      {@render actions(f)}
    </div>
  {/each}
{/snippet}

{#snippet group(title: string, g: Group)}
  <h3>{title} <span class="muted">{g.total}</span></h3>
  {#if g.total === 0}
    <p class="muted">None</p>
  {:else}
    {@render formulas(g.items)}
    {#if g.total > g.items.length}
      <p class="muted small">The page shows {g.items.length} of {g.total} formulas.</p>
    {/if}
  {/if}
{/snippet}

{#if error}
  <p class="error page-pad">{error}</p>
{:else if page}
  <article>
    <div class="title">
      <h1 class="mono">{page.name}</h1>
      {#each page.kinds as k (k)}<span class="badge">{k}</span>{/each}
    </div>

    {#if applied && !pending}
      <div class="applied">
        <span>The change is applied. It is not saved. To undo the change, use Undo in the file.</span>
      </div>
    {/if}

    {#if page.problems.length > 0}
      <section>
        <h2>Problems <span class="muted">{page.problems.length}</span></h2>
        {#each page.problems as p, i (i)}
          <button class="plain problem" onclick={() => onOpen({ file: p.file, line: p.line })}>
            <span class="severity {p.severity}">{p.severity === 'error' ? '●' : p.severity === 'warning' ? '▲' : 'ⓘ'}</span>
            <span class="mono muted">{p.code}</span>
            <span class="text">{p.message}</span>
            <span class="mono muted">{fileName(p.file)}:{p.line}</span>
          </button>
        {/each}
      </section>
    {/if}

    {#if !page.exists}
      <p class="error">No formula in the project uses this term.</p>
    {:else}
      <section>
        <h2>Documentation</h2>
        {#if page.documentation.length === 0}
          <p class="muted">None</p>
        {/if}
        <div class="docs">
          {#each byLanguage(page.documentation) as [lang, entries] (lang)}
            {#each entries as entry, i (i)}
              <span class="muted small">{i === 0 ? language(lang) : ''}</span>
              <div>
                {#if editing === key(entry.formula)}
                  {@render editor(entry.formula)}
                {:else}
                  <p class:format={entry.relation === 'termFormat'}>{entry.text}</p>
                  {@render review(key(entry.formula))}
                {/if}
              </div>
              {@render actions(entry.formula)}
            {/each}
          {/each}
        </div>
      </section>

      <section>
        <h2>Hierarchy</h2>
        <dl>
          {#if page.hierarchy}
            <dt>Parents</dt>
            <dd>{@render links(page.parents)}</dd>
            <dt>All ancestors</dt>
            <dd>{@render links(page.ancestors)}</dd>
            <dt>Children</dt>
            <dd>{@render links(page.children)}</dd>
          {/if}
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
            <p class="muted">Arity: {page.arity}</p>
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
        {#if !adding}
          <button class="dashed" onclick={() => (adding = true)}><Plus class="ico" /> Add Fact</button>
        {:else}
          <h2>Add Fact</h2>
          <div class="add">
            <label>
              <span class="muted small">File</span>
              <select bind:value={addFile}>
                {#each editableFiles as f (f.id)}
                  <option value={f.id}>{fileName(f.path)}</option>
                {/each}
              </select>
            </label>
            <label>
              <span class="muted small">Relation</span>
              <input class="mono" bind:value={addRelation} />
            </label>
            {#each addArgs as arg, i (i)}
              <label>
                <span class="muted small">Argument {i + 1}</span>
                <span class="arg">
                  <select aria-label="Kind of argument {i + 1}" bind:value={arg.kind}>
                    <option value="symbol">symbol</option>
                    <option value="string">string</option>
                    <option value="number">number</option>
                  </select>
                  <input class="mono" bind:value={arg.text} />
                </span>
              </label>
            {/each}
            <div class="buttons">
              <button
                class="small"
                aria-label="Add an argument"
                onclick={() => (addArgs = [...addArgs, { kind: 'symbol', text: '' }])}><Plus class="ico" /> Argument</button
              >
              {#if addArgs.length > 1}
                <button
                  class="small"
                  aria-label="Remove the last argument"
                  onclick={() => (addArgs = addArgs.slice(0, -1))}><Minus class="ico" /> Argument</button
                >
              {/if}
            </div>
          </div>
          {#if pending?.anchor !== ADD}
            <div class="buttons">
              <button onclick={submitAdd} disabled={!addRelation}>Preview</button>
              <button onclick={resetAdd}>Cancel</button>
            </div>
          {/if}
          {@render review(ADD)}
        {/if}
      </section>
    {/if}
  </article>
{/if}

<style>
  article {
    max-width: 920px;
    padding: 20px 28px 40px;
  }
  .page-pad {
    padding: 20px 28px;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0 4px 0 0;
    font-size: 22px;
    font-weight: 600;
  }
  h2 {
    margin: 24px 0 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }
  h3 {
    margin: 14px 0 4px;
    font-size: 12px;
    font-weight: 500;
  }
  p {
    margin: 4px 0;
  }
  .small {
    font-size: 12px;
  }
  .format {
    font-style: italic;
  }
  .applied {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--surface-2);
  }
  .docs {
    display: grid;
    grid-template-columns: 96px 1fr auto;
    gap: 6px 12px;
    align-items: start;
  }
  .docs > span {
    padding-top: 5px;
  }
  dl {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 6px 12px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  .link {
    padding-right: 10px;
    color: var(--accent);
    font-size: 12px;
  }
  .link:hover {
    text-decoration: underline;
  }
  .formula {
    display: grid;
    grid-template-columns: 170px 1fr auto;
    gap: 12px;
    align-items: start;
    padding: 2px 0;
  }
  .location {
    justify-content: flex-start;
    font-size: 12px;
    line-height: 20px;
    color: var(--muted);
  }
  .location:hover {
    color: var(--accent);
  }
  .text {
    min-width: 0;
  }
  .actions {
    display: flex;
    gap: 2px;
    visibility: hidden;
  }
  .formula:hover .actions,
  .docs:hover .actions,
  .actions:focus-within {
    visibility: visible;
  }
  .action {
    width: 24px;
    height: 24px;
    color: var(--muted);
  }
  .action:hover {
    color: var(--text);
  }
  .problem {
    width: 100%;
    display: flex;
    justify-content: flex-start;
    gap: 8px;
    padding: 2px 0;
    font-size: 12px;
    text-align: left;
  }
  .problem:hover .text {
    text-decoration: underline;
  }
  .problem .text {
    flex: 1;
    min-width: 0;
  }
  .severity.error {
    color: var(--error);
  }
  .severity.warning {
    color: var(--warning);
  }
  .severity.info {
    color: var(--muted);
  }
  .edit {
    padding: 8px;
    border: 1px solid var(--accent);
    border-radius: 6px;
  }
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .fields textarea {
    width: 100%;
  }
  .review {
    margin-top: 8px;
  }
  .diff {
    border-radius: 4px;
    overflow: hidden;
    font-size: 12px;
  }
  .diff > div {
    padding: 3px 8px;
    white-space: pre-wrap;
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .dashed {
    margin-top: 24px;
    border-style: dashed;
    color: var(--muted);
  }
  .add {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px 12px;
  }
  .add label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .arg {
    display: flex;
    gap: 4px;
  }
</style>
