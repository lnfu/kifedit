// 主題：第一次跟隨系統設定，使用者切換後記住選擇（docs/ui-design.md 第 5 節）
type Theme = 'light' | 'dark'

const KEY = 'kifedit.theme'

function stored(): Theme | null {
  try {
    const value = localStorage.getItem(KEY)
    return value === 'light' || value === 'dark' ? value : null
  } catch {
    return null
  }
}

function system(): Theme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export const theme = $state({ current: stored() ?? system() })

export function applyTheme() {
  document.documentElement.dataset.theme = theme.current
}

export function toggleTheme() {
  theme.current = theme.current === 'dark' ? 'light' : 'dark'
  applyTheme()
  try {
    localStorage.setItem(KEY, theme.current)
  } catch {}
}

// 使用者還沒選過時，系統設定改變就跟著改
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  if (stored()) return
  theme.current = system()
  applyTheme()
})
