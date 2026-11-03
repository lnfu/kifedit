<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import TreeNode from './TreeNode.svelte'
  import type { TreeNodeData as Node } from './types'

  let { view, onSelect }: { view: string; onSelect: (name: string) => void } = $props()

  let roots = $state<Node[]>([])
  let error = $state<string | null>(null)

  $effect(() => {
    invoke<Node[]>('tree_children', { view, node: null })
      .then((nodes) => (roots = nodes))
      .catch((e) => (error = String(e)))
  })
</script>

{#if error}
  <p class="error">{error}</p>
{:else}
  <ul>
    {#each roots as root (root.kind + root.name)}
      <TreeNode {view} node={root} {onSelect} />
    {/each}
  </ul>
{/if}

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
</style>
