import { listen } from '@tauri-apps/api/event'
import { SvelteMap } from 'svelte/reactivity'
import type { Diagnostic, DiagnosticsChanged } from './types'

// 所有檔案最新的診斷，由後端的 diagnostics_changed 事件更新（§14.3）
export const problems = new SvelteMap<number, Diagnostic[]>()

let started = false

export function startProblems() {
  if (started) return
  started = true
  listen<DiagnosticsChanged>('diagnostics_changed', (event) => {
    problems.set(event.payload.file, event.payload.diagnostics)
  })
}

export function clearProblems() {
  problems.clear()
}
