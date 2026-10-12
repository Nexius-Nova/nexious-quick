export type ItemType = 'application' | 'website' | 'folder' | 'file'
export type ThemeMode = 'system' | 'light' | 'dark'
export interface Item {
  id: number
  icon: string
  name: string
  alias: string
  type: ItemType
  url: string
  args?: string
  workdir?: string
  description: string
  enabled: boolean
  updatedAt: string
}

export interface AppSettings {
  theme: ThemeMode
  accent: string
  opacity: number
  searchRadius: number
  searchWidth: number
  searchHeight: number
  placeholderText: string
  launcherIcon: string
  autoHide: boolean
  hideOnBlur: boolean
  autoStart: boolean
  alwaysOnTop: boolean
  closeToTray: boolean
  hoverShow: boolean
  showIcons: boolean
  autoSyncApps: boolean
  enabledWebsites: boolean
  enabledFolders: boolean
  lastWebsiteSync: string
  lastFolderSync: string
  shortcut: string
  searchEngine: string
  lastSync: string
}

export const DEFAULT_SETTINGS: AppSettings = {
  theme: 'system',
  accent: 'blue',
  opacity: 85,
  searchRadius: 16,
  searchWidth: 480,
  searchHeight: 40,
  placeholderText: '输入内容，快速启动...',
  launcherIcon: 'app',
  autoHide: true,
  hideOnBlur: false,
  autoStart: false,
  alwaysOnTop: true,
  closeToTray: true,
  hoverShow: false,
  showIcons: true,
  autoSyncApps: true,
  enabledWebsites: true,
  enabledFolders: true,
  lastWebsiteSync: '',
  lastFolderSync: '',
  shortcut: 'Alt+Space',
  searchEngine: 'Google',
  lastSync: '',
}

export const SEARCH_ENGINES: Record<string, string> = {
  Google: 'https://www.google.com/search?q=',
  Bing: 'https://www.bing.com/search?q=',
  百度: 'https://www.baidu.com/s?wd=',
}

export const TYPE_LABEL: Record<ItemType, string> = {
  application: '应用',
  website: '网站',
  folder: '文件夹',
  file: '文件',
}

/** "Alt+Space" -> "Alt + Space"，仅用于展示 */
export function prettyShortcut(combo: string): string {
  return combo
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => {
      const low = p.toLowerCase()
      if (low === 'alt') return 'Alt'
      if (low === 'ctrl' || low === 'control') return 'Ctrl'
      if (low === 'shift') return 'Shift'
      if (low === 'super' || low === 'meta') return 'Win'
      if (/^key[a-z]$/i.test(p)) return p.slice(3).toUpperCase()
      if (/^digit\d$/i.test(p)) return p.slice(5)
      return p.length === 1 ? p.toUpperCase() : p.charAt(0).toUpperCase() + p.slice(1)
    })
    .join(' + ')
}

/** 把 keydown 事件转换为组合键字符串，例如 "Ctrl+Alt+T"；纯修饰键返回 null */
export function comboFromEvent(e: KeyboardEvent): string | null {
  const key = e.key
  if (['Control', 'Alt', 'Shift', 'Meta', 'AltGraph'].includes(key)) return null
  let code: string
  if (/^[a-zA-Z]$/.test(key)) code = key.toUpperCase()
  else if (/^\d$/.test(key)) code = key
  else if (key === ' ') code = 'Space'
  else if (key === 'Escape') return 'Escape'
  else if (e.code.startsWith('Key')) code = e.code.slice(3)
  else if (e.code.startsWith('Digit')) code = e.code.slice(5)
  else if (e.code.startsWith('Arrow')) code = e.code.slice(5)
  else if (['F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12'].includes(key)) code = key
  else code = key
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Super')
  parts.push(code)
  return parts.join('+')
}

export const RESERVED_SHORTCUTS = ['Alt+Tab', 'Alt+F4', 'Ctrl+Alt+Delete', 'Ctrl+Shift+Escape']

export function formatLastSync(raw: string): string {
  if (!raw) return '尚未同步'
  const ms = Number(raw)
  if (!Number.isFinite(ms) || ms <= 0) return '尚未同步'
  const d = new Date(ms)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}
