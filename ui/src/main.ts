import { mount } from 'svelte'
import '@fontsource/ibm-plex-sans/400.css'
import '@fontsource/ibm-plex-sans/500.css'
import '@fontsource/ibm-plex-sans/600.css'
import '@fontsource/ibm-plex-mono/400.css'
import '@fontsource/ibm-plex-mono/500.css'
import '@fontsource/ibm-plex-mono/600.css'
import './app.css'
import App from './App.svelte'
import { applyTheme } from './lib/theme.svelte'

// 掛載前先設定主題，避免先閃過另一種顏色
applyTheme()

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
