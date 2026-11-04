<script lang="ts">
  import X from '@lucide/svelte/icons/x'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import { SvelteSet } from 'svelte/reactivity'
  import { problems } from './problems.svelte'
  import type { Diagnostic, FileSummary, Location, Severity } from './types'

  let {
    files,
    onOpen,
    onClose,
  }: {
    files: FileSummary[]
    onOpen: (location: Location) => void
    onClose: () => void
  } = $props()

  type Item = { file: FileSummary; d: Diagnostic }
  type Row = { kind: 'group'; key: string; label: string; count: number } | { kind: 'item'; item: Item }

  const ROW = 22
  const ORDER: Severity[] = ['error', 'warning', 'info']
  const ICON: Record<Severity, string> = { error: '●', warning: '▲', info: 'ⓘ' }

  let groupBy = $state<'file' | 'rule' | 'severity'>('file')
  let query = $state('')
  let show = $state<Record<Severity, boolean>>({ error: true, warning: true, info: true })
  let editableOnly = $state(true)
  const collapsed = new SvelteSet<string>()

  let byId = $derived(new Map(files.map((f) => [f.id, f])))

  let items = $derived.by(() => {
    const q = query.trim().toLowerCase()
    const out: Item[] = []
    for (const [id, list] of problems) {
      const file = byId.get(id)
      if (!file || (editableOnly && file.role !== 'editable')) continue
      for (const d of list) {
        if (!show[d.severity]) continue
        if (q && !`${d.code} ${d.message} ${fileName(file.path)}`.toLowerCase().includes(q)) continue
        out.push({ file, d })
      }
    }
    return out
  })

  let counts = $derived.by(() => {
    const c: Record<Severity, number> = { error: 0, warning: 0, info: 0 }
    for (const { d } of items) c[d.severity] += 1
    return c
  })

  // 分組後攤平成一列一列，給虛擬捲動使用
  let rows = $derived.by(() => {
    const groups = new Map<string, { label: string; items: Item[] }>()
    for (const item of items) {
      const [key, label] =
        groupBy === 'file'
          ? [`f${item.file.id}`, fileName(item.file.path)]
          : groupBy === 'rule'
            ? [`r${item.d.code}`, item.d.code]
            : [`s${ORDER.indexOf(item.d.severity)}`, severityLabel(item.d.severity)]
      let group = groups.get(key)
      if (!group) groups.set(key, (group = { label, items: [] }))
      group.items.push(item)
    }
    const out: Row[] = []
    const sorted = [...groups].sort((a, b) =>
      groupBy === 'severity' ? a[0].localeCompare(b[0]) : a[1].label.localeCompare(b[1].label),
    )
    for (const [key, group] of sorted) {
      out.push({ kind: 'group', key, label: group.label, count: group.items.length })
      if (collapsed.has(key)) continue
      group.items.sort((a, b) => a.file.path.localeCompare(b.file.path) || a.d.line - b.d.line || a.d.column - b.d.column)
      for (const item of group.items) out.push({ kind: 'item', item })
    }
    return out
  })

  // 虛擬捲動：只畫看得到的列
  let scrollTop = $state(0)
  let height = $state(0)
  let first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 5))
  let visible = $derived(rows.slice(first, first + Math.ceil(height / ROW) + 10))

  function toggle(key: string) {
    if (collapsed.has(key)) collapsed.delete(key)
    else collapsed.add(key)
  }

  function severityLabel(s: Severity) {
    return s === 'error' ? 'Errors' : s === 'warning' ? 'Warnings' : 'Information'
  }

  function fileName(path: string) {
    return path.split('/').pop() ?? path
  }
</script>

<section class="panel" aria-label="Problems">
  <div class="header">
    <span class="title">Problems</span>
    {#each ORDER as s (s)}
      <label class="toggle {s}">
        <input type="checkbox" bind:checked={show[s]} />
        {ICON[s]} {counts[s]}
      </label>
    {/each}
    <input class="filter" aria-label="Filter problems" placeholder="Filter" bind:value={query} />
    <select aria-label="Group by" bind:value={groupBy}>
      <option value="file">Group by file</option>
      <option value="rule">Group by rule</option>
      <option value="severity">Group by severity</option>
    </select>
    <label class="muted"><input type="checkbox" bind:checked={editableOnly} /> Editable files only</label>
    <button class="plain close" aria-label="Close the problems panel" title="Close (Ctrl+Shift+M)" onclick={onClose}>
      <X class="ico" />
    </button>
  </div>
  <div class="list" bind:clientHeight={height} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
    {#if rows.length === 0}
      <p class="muted empty">No problems.</p>
    {/if}
    <div class="spacer" style:height="{rows.length * ROW}px">
      {#each visible as row, i (first + i)}
        <div class="row-slot" style:top="{(first + i) * ROW}px">
          {#if row.kind === 'group'}
            <button class="plain group" onclick={() => toggle(row.key)}>
              {#if collapsed.has(row.key)}<ChevronRight class="ico" />{:else}<ChevronDown class="ico" />{/if}
              <span>{row.label}</span>
              <span class="muted">{row.count}</span>
            </button>
          {:else}
            {@const { file, d } = row.item}
            <button class="plain item" onclick={() => onOpen({ file: file.id, line: d.line })}>
              <span class="icon {d.severity}">{ICON[d.severity]}</span>
              <span class="code mono">{d.code}</span>
              <span class="message">{d.message}</span>
              <span class="where mono">{fileName(file.path)}:{d.line}:{d.column}</span>
            </button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</section>

<style>
  .panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 12px;
    flex-wrap: wrap;
  }
  .title {
    font-weight: 600;
    font-size: 13px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .filter {
    height: 24px;
    width: 180px;
  }
  .header select {
    height: 24px;
    font-size: 12px;
  }
  .close {
    margin-left: auto;
    color: var(--muted);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .spacer {
    position: relative;
  }
  .row-slot {
    position: absolute;
    left: 0;
    right: 0;
    height: 22px;
  }
  .group,
  .item {
    width: 100%;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 8px;
    padding: 0 12px;
    border-radius: 0;
    font-size: 12px;
    text-align: left;
    white-space: nowrap;
  }
  .group {
    font-weight: 600;
  }
  .item {
    padding-left: 32px;
  }
  .group:hover,
  .item:hover {
    background: var(--surface-2);
  }
  .item .message {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .code {
    color: var(--muted);
  }
  .where {
    color: var(--muted);
  }
  .icon.error,
  .toggle.error {
    color: var(--error);
  }
  .icon.warning,
  .toggle.warning {
    color: var(--warning);
  }
  .icon.info,
  .toggle.info {
    color: var(--muted);
  }
  .empty {
    padding: 8px 12px;
    margin: 0;
  }
</style>
