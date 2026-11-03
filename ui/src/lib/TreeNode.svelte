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
        children = await invoke<Node[]>('tree_children', { view, node: node.name })
      } catch (e) {
        error = String(e)
      }
    }
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
    <button class="name {node.kind}" onclick={() => onSelect(node.name)}>{node.name}</button>
    {#if node.otherParents > 0}
      <span class="parents">+{node.otherParents} other parents</span>
    {/if}
  </div>
  {#if expanded}
    {#if error}
      <p class="error">{error}</p>
    {:else if children}
      <ul>
        {#each children as child (child.kind + child.name)}
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
  }
  .name:hover {
    text-decoration: underline;
  }
  .instance {
    font-style: italic;
  }
  .parents {
    color: var(--muted);
    font-size: 12px;
  }
</style>
