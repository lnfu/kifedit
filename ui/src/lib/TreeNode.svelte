<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import TreeNode from './TreeNode.svelte'
  import type { TreeNodeData as Node } from './types'

  let {
    view,
    node,
    onSelect,
  }: { view: string; node: Node; onSelect: (name: string) => void } = $props()

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
    else if (node.hasChildren) toggle()
  }
</script>

<li>
  <div class="row">
    {#if node.hasChildren}
      <button class="toggle" onclick={toggle} aria-label={expanded ? 'Collapse' : 'Expand'}>
        {expanded ? '▾' : '▸'}
      </button>
    {:else}
      <span class="toggle"></span>
    {/if}
    <button class="name {node.kind}" onclick={select}>{node.label}</button>
    {#if node.otherParents > 0}
      <span class="parents">+{node.otherParents} other parents</span>
    {/if}
  </div>
  {#if expanded}
    {#if error}
      <p class="error">{error}</p>
    {:else if children}
      <ul>
        {#each children as child (child.kind + child.id)}
          <TreeNode {view} node={child} {onSelect} />
        {/each}
      </ul>
    {/if}
  {/if}
</li>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding-left: 16px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .toggle {
    width: 16px;
    flex: none;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    cursor: pointer;
  }
  .name {
    border: none;
    background: none;
    padding: 0;
    color: inherit;
    font: inherit;
    cursor: pointer;
    text-align: left;
  }
  .name:hover {
    text-decoration: underline;
  }
  .member {
    font-style: italic;
  }
  .formula {
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .parents {
    color: var(--muted);
    font-size: 12px;
  }
</style>
