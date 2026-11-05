// webview 與插件之間的訊息。LSP request 由插件轉給 server
declare function acquireVsCodeApi(): { postMessage(message: unknown): void }

const vscode = acquireVsCodeApi()
let next = 0
const waiting = new Map<number, { resolve: (value: any) => void; reject: (reason: string) => void }>()
const listeners: ((message: any) => void)[] = []

window.addEventListener('message', (event) => {
  const message = event.data
  if (message.type === 'response') {
    const pending = waiting.get(message.id)
    waiting.delete(message.id)
    if (message.error !== undefined) pending?.reject(message.error)
    else pending?.resolve(message.result)
    return
  }
  listeners.forEach((listener) => listener(message))
})

export function call<T = unknown>(method: string, params?: unknown): Promise<T> {
  const id = next++
  return new Promise<T>((resolve, reject) => {
    waiting.set(id, { resolve, reject })
    vscode.postMessage({ type: 'request', id, method, params })
  })
}

export function post(message: unknown) {
  vscode.postMessage(message)
}

export function onMessage(listener: (message: any) => void) {
  listeners.push(listener)
}
