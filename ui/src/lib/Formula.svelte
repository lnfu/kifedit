<script lang="ts">
  import { highlight } from './kif'

  let { text }: { text: string } = $props()
  let spans = $derived(highlight(text))
</script>

<pre>{#each spans as s, i (i)}{#if s.cls}<span class={s.cls}>{s.text}</span>{:else}{s.text}{/if}{/each}</pre>

<style>
  pre {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 20px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
