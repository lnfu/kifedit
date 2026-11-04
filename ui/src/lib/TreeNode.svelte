<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import TreeNode from './TreeNode.svelte'
  import type { Location, TreeNodeData as Node } from './types'

  let {
    view,
    node,
    depth,
    selected,
    onSelect,
    onOpen,
  }: {
    view: string
    node: Node
    depth: number
    selected: string | null
    onSelect: (name: string) => void
    onOpen: (location: Location) => void
  } = $props()

  let expanded = $state(false)
  let children = $state<Node[] | null>(null)
  let error = $state<string | null>(null)

  async function toggle() {
    expanded = !expanded
    if (expanded && children === null) {
      try {
        children = await invoke<Node[]>('tree_children', { view, node: node.id })
      } catch (e) {
        error = String(e)
      }
    }
  }

  function select() {
    if (node.term) onSelect(node.term)
    else if (node.location) onOpen(node.location)
    else if (node.hasChildren) toggle()
  }

  // 方向鍵：右展開、左收合（docs/ui-design.md 第 9 節）
  function onKey(event: KeyboardEvent) {
    if (event.key === 'ArrowRight' && node.hasChildren && !expanded) toggle()
    else if (event.key === 'ArrowLeft' && expanded) toggle()
    else return
    event.preventDefault()
  }

  // 箭頭只展開或收合，不開啟詞彙頁
  function onChevron(event: MouseEvent) {
    event.stopPropagation()
    toggle()
  }
</script>

<li role="treeitem" aria-expanded={node.hasChildren ? expanded : undefined} aria-selected={node.term !== null && node.term === selected}>
  <!-- 整列是一個按鈕；雙擊展開或收合 -->
  <button
    class="plain row {node.kind}"
    class:selected={node.term !== null && node.term === selected}
    style:padding-left="{8 + depth * 12}px"
    onclick={select}
    ondblclick={() => node.hasChildren && (node.term || node.location) && toggle()}
    onkeydown={onKey}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <span class="toggle" aria-hidden="true" onclick={node.hasChildren ? onChevron : undefined} ondblclick={(e) => e.stopPropagation()}>
      {#if node.hasChildren}
        {#if expanded}<ChevronDown class="ico" />{:else}<ChevronRight class="ico" />{/if}
      {/if}
    </span>
    <span class="name">{node.label}</span>
    {#if node.otherParents > 0}
      <span class="parents">+{node.otherParents} other parents</span>
    {/if}
  </button>
  {#if expanded}
    {#if error}
      <p class="error">{error}</p>
    {:else if children}
      <ul role="group">
        {#each children as child (child.kind + child.id)}
          <TreeNode {view} node={child} depth={depth + 1} {selected} {onSelect} {onOpen} />
        {/each}
      </ul>
    {/if}
  {/if}
</li>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    width: 100%;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 2px;
    padding-right: 8px;
    border-radius: 0;
    white-space: nowrap;
    text-align: left;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .row:hover:not(.selected) {
    background: var(--surface-2);
  }
  .row.selected,
  .row.selected:hover {
    background: var(--selected);
    color: var(--selected-text);
  }
  .row:focus-visible {
    outline-offset: -2px;
  }
  .toggle {
    width: 16px;
    height: 16px;
    flex: none;
    display: flex;
    color: var(--muted);
  }
  .member {
    font-style: italic;
  }
  .group {
    color: var(--muted);
  }
  .parents {
    margin-left: 6px;
    color: var(--muted);
    font-family: var(--font-sans);
    font-size: 11px;
  }
  .error {
    padding: 0 12px;
    margin: 0;
  }
</style>
