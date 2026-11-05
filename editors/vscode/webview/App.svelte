<script lang="ts">
  import TermPage from './TermPage.svelte'
  import { call, onMessage, post } from './bridge'
  import type { EditableFile } from './types'

  let name = $state(document.body.dataset.name ?? '')
  let version = $state(0)
  let editableFiles = $state<EditableFile[]>([])

  onMessage((message) => {
    if (message.type === 'show') name = message.name
    else if (message.type === 'changed') version += 1
  })

  call<EditableFile[]>('kifedit/editableFiles').then((files) => (editableFiles = files))

  $effect(() => {
    post({ type: 'title', name })
    window.scrollTo(0, 0)
  })
</script>

<TermPage
  {name}
  {editableFiles}
  {version}
  onNavigate={(n) => (name = n)}
  onOpen={(location) => post({ type: 'open', ...location })}
/>
