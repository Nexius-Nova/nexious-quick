export type ItemType = 'application' | 'website' | 'folder' | 'file'
export type ThemeMode = 'system' | 'light' | 'dark'
export type LauncherAnimation = 'reveal' | 'float' | 'spring' | 'crossfade' | 'spotlight'

export const LAUNCHER_ANIMATION_OPTIONS: Array<{
  key: LauncherAnimation
  label: string
  description: string
}> = [
  { key: 'reveal', label: '垂直展开', description: '结果从搜索框方向向下逐项展开' },
  { key: 'float', label: '卡片浮出', description: '卡片从下方淡入上浮' },
  { key: 'spring', label: '弹性浮现', description: '结果项依次带弹簧回弹浮现' },
  { key: 'crossfade', label: '平稳淡化', description: '平稳淡入，输入过程最平稳' },
  { key: 'spotlight', label: '聚焦滑动', description: '选中项横向滑出聚焦，像光标一样跟随移动' },
]

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
  animationEffect: LauncherAnimation
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
  animationEffect: 'crossfade',
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

export interface AccentTheme {
  key: string
  label: string
  color: string
  rgb: string
  primary: string
  primaryStrong: string
  lightHover: string
  lightPressed: string
  darkPrimary: string
  darkHover: string
  darkPressed: string
}

export const ACCENT_THEMES: AccentTheme[] = [
  {
    key: 'blue',
    label: '经典蓝',
    color: '#2f6bfe',
    rgb: '47, 107, 254',
    primary: '#2f6bfe',
    primaryStrong: '#245fd8',
    lightHover: '#477cff',
    lightPressed: '#245fd8',
    darkPrimary: '#729bff',
    darkHover: '#96b4ff',
    darkPressed: '#5b88f5',
  },
  {
    key: 'violet',
    label: '典雅紫',
    color: '#7c3aed',
    rgb: '124, 58, 237',
    primary: '#7c3aed',
    primaryStrong: '#6d28d9',
    lightHover: '#8b5cf6',
    lightPressed: '#6d28d9',
    darkPrimary: '#a78bfa',
    darkHover: '#c4b5fd',
    darkPressed: '#8b5cf6',
  },
  {
    key: 'cyan',
    label: '湖水青',
    color: '#0891b2',
    rgb: '8, 145, 178',
    primary: '#0891b2',
    primaryStrong: '#0e7490',
    lightHover: '#06b6d4',
    lightPressed: '#0e7490',
    darkPrimary: '#22d3ee',
    darkHover: '#67e8f9',
    darkPressed: '#06b6d4',
  },
  {
    key: 'emerald',
    label: '翡翠绿',
    color: '#059669',
    rgb: '5, 150, 105',
    primary: '#059669',
    primaryStrong: '#047857',
    lightHover: '#10b981',
    lightPressed: '#047857',
    darkPrimary: '#34d399',
    darkHover: '#6ee7b7',
    darkPressed: '#10b981',
  },
  {
    key: 'orange',
    label: '落日橙',
    color: '#ea580c',
    rgb: '234, 88, 12',
    primary: '#ea580c',
    primaryStrong: '#c2410c',
    lightHover: '#f97316',
    lightPressed: '#c2410c',
    darkPrimary: '#fb923c',
    darkHover: '#fdba74',
    darkPressed: '#f97316',
  },
  {
    key: 'rose',
    label: '樱粉',
    color: '#e11d48',
    rgb: '225, 29, 72',
    primary: '#e11d48',
    primaryStrong: '#be123c',
    lightHover: '#f43f5e',
    lightPressed: '#be123c',
    darkPrimary: '#fb7185',
    darkHover: '#fda4af',
    darkPressed: '#f43f5e',
  },
  {
    key: 'slate',
    label: '石墨灰',
    color: '#475569',
    rgb: '71, 85, 105',
    primary: '#475569',
    primaryStrong: '#334155',
    lightHover: '#64748b',
    lightPressed: '#334155',
    darkPrimary: '#94a3b8',
    darkHover: '#cbd5e1',
    darkPressed: '#64748b',
  },
]

export function accentTheme(key: string): AccentTheme {
  return ACCENT_THEMES.find((item) => item.key === key) ?? ACCENT_THEMES[0]
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
