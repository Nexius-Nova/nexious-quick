import { computed, reactive, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { isTauri, storage } from './adapter'
import {
  DEFAULT_SETTINGS,
  LAUNCHER_ANIMATION_OPTIONS,
  SEARCH_ENGINES,
  accentTheme,
  type AppSettings,
  type Item,
  type ItemType,
  type LauncherAnimation,
  type ThemeMode,
} from './types'

export const store = reactive({
  ready: false,
  settingsError: '',
  savingSettings: false,
  items: [] as Item[],
  settings: { ...DEFAULT_SETTINGS } as AppSettings,
})

export const prefersDark = ref(
  typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches,
)
if (typeof window !== 'undefined' && window.matchMedia) {
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
    prefersDark.value = e.matches
  })
}

export const isDark = computed(
  () => store.settings.theme === 'dark' || (store.settings.theme === 'system' && prefersDark.value),
)

function applySettings(map: Record<string, string>) {
  const s = map
  Object.assign(store.settings, {
    theme: (['system', 'light', 'dark'].includes(s.theme) ? s.theme : 'system') as ThemeMode,
    accent: accentTheme(s.accent ?? '').key,
    opacity: clamp(Number(s.opacity ?? DEFAULT_SETTINGS.opacity), 30, 100),
    searchRadius: clamp(Number(s.searchRadius ?? DEFAULT_SETTINGS.searchRadius), 0, 32),
    searchWidth: clamp(Number(s.searchWidth ?? DEFAULT_SETTINGS.searchWidth), 320, 720),
    searchHeight: clamp(Number(s.searchHeight ?? DEFAULT_SETTINGS.searchHeight), 36, 64),
    animationEffect: (
      LAUNCHER_ANIMATION_OPTIONS.some((option) => option.key === s.animationEffect)
        ? s.animationEffect
        : DEFAULT_SETTINGS.animationEffect
    ) as LauncherAnimation,
    placeholderText: (s.placeholderText || '').trim().slice(0, 24) || DEFAULT_SETTINGS.placeholderText,
    // 旧版本保存的是内置图标 key，统一迁移到新的应用品牌图标；自定义图片继续保留。
    launcherIcon: s.launcherIcon?.startsWith('data:image/') ? s.launcherIcon : DEFAULT_SETTINGS.launcherIcon,
    autoHide: String(s.autoHide) !== 'false',
    hideOnBlur: String(s.hideOnBlur) === 'true',
    autoStart: String(s.autoStart) === 'true',
    alwaysOnTop: String(s.alwaysOnTop) !== 'false',
    closeToTray: String(s.closeToTray) !== 'false',
    hoverShow: String(s.hoverShow) === 'true',
    showIcons: String(s.showIcons) !== 'false',
    autoSyncApps: String(s.autoSyncApps) !== 'false',
    enabledWebsites: String(s.enabledWebsites) !== 'false',
    enabledFolders: String(s.enabledFolders) !== 'false',
    lastWebsiteSync: s.lastWebsiteSync || '',
    lastFolderSync: s.lastFolderSync || '',
    shortcut: s.shortcut || DEFAULT_SETTINGS.shortcut,
    searchEngine: Object.prototype.hasOwnProperty.call(SEARCH_ENGINES, s.searchEngine) ? s.searchEngine : DEFAULT_SETTINGS.searchEngine,
    lastSync: s.lastSync || '',
  })
}

function clamp(n: number, min: number, max: number): number {
  if (!Number.isFinite(n)) return min
  return Math.min(max, Math.max(min, Math.round(n)))
}

function toSettingsMap(s: AppSettings): Record<string, string> {
  return {
    theme: s.theme,
    accent: s.accent,
    opacity: String(s.opacity),
    searchRadius: String(s.searchRadius),
    searchWidth: String(s.searchWidth),
    searchHeight: String(s.searchHeight),
    animationEffect: s.animationEffect,
    placeholderText: s.placeholderText,
    launcherIcon: s.launcherIcon,
    autoHide: String(s.autoHide),
    hideOnBlur: String(s.hideOnBlur),
    autoStart: String(s.autoStart),
    alwaysOnTop: String(s.alwaysOnTop),
    closeToTray: String(s.closeToTray),
    hoverShow: String(s.hoverShow),
    showIcons: String(s.showIcons),
    autoSyncApps: String(s.autoSyncApps),
    enabledWebsites: String(s.enabledWebsites),
    enabledFolders: String(s.enabledFolders),
    lastWebsiteSync: s.lastWebsiteSync,
    lastFolderSync: s.lastFolderSync,
    shortcut: s.shortcut,
    searchEngine: s.searchEngine,
    lastSync: s.lastSync,
  }
}

let settingsTimer: number | undefined
let pendingSettings: Record<string, string> = {}
let savingPatch: Record<string, string> = {}
let applyingRemote = false

export function receiveSettings(patch: Record<string, string>) {
  if (!patch || typeof patch !== 'object') return
  applyingRemote = true
  try {
    applySettings({ ...toSettingsMap(store.settings), ...patch, ...savingPatch, ...pendingSettings })
  } finally {
    applyingRemote = false
  }
}

export async function flushSettings() {
  window.clearTimeout(settingsTimer)
  if (store.savingSettings || !Object.keys(pendingSettings).length) return
  store.savingSettings = true
  store.settingsError = ''
  try {
    // Save only changed keys so another window cannot overwrite unrelated settings.
    while (Object.keys(pendingSettings).length) {
      savingPatch = pendingSettings
      pendingSettings = {}
      await storage.saveSettings(savingPatch)
      savingPatch = {}
    }
  } catch (error) {
    pendingSettings = { ...savingPatch, ...pendingSettings }
    savingPatch = {}
    store.settingsError = `设置保存失败：${String(error)}`
  } finally {
    store.savingSettings = false
  }
}

export async function reloadItems() {
  store.items = sanitizeItems(await storage.loadItems())
}

/** 按类型加载启动项子集（application / website / folder），供设置页按需取数，避免整表拉取。 */
export async function loadItemsByKind(kind: ItemType | 'all' = 'all'): Promise<Item[]> {
  if (!isTauri) {
    const all = sanitizeItems(await storage.loadItems())
    if (kind === 'all') return all
    return all.filter((i) =>
      kind === 'folder' ? i.type === 'folder' || i.type === 'file' : i.type === kind,
    )
  }
  return sanitizeItems(await invoke<unknown[]>('db_load_items_by_kind', { kind }))
}

/** 在给定列表内查找重复项（文件/文件夹按路径+名称，网站按 URL，应用按路径+参数）。 */
export function findDuplicateAmong(list: Item[], candidate: Item): Item | undefined {
  const key = itemDedupeKey(candidate)
  return list.find((item) => item.id !== candidate.id && itemDedupeKey(item) === key)
}

function sanitizeItems(list: unknown[]): Item[] {
  if (!Array.isArray(list)) return []
  const types: ItemType[] = ['application', 'website', 'folder', 'file']
  const items = list
    .filter((raw): raw is Record<string, unknown> => !!raw && typeof raw === 'object')
    .map((raw) => ({
      id: Number(raw.id) || 0,
      icon: String(raw.icon ?? ''),
      name: String(raw.name ?? ''),
      alias: String(raw.alias ?? ''),
      type: (types.includes(raw.type as ItemType) ? raw.type : 'application') as ItemType,
      url: String(raw.url ?? ''),
      args: String(raw.args ?? ''),
      workdir: String(raw.workdir ?? ''),
      description: String(raw.description ?? ''),
      enabled: raw.enabled !== false,
      updatedAt: String(raw.updatedAt ?? ''),
    }))
    .filter((i) => i.name || i.url)
  return dedupeItems(items)
}

/** 条目信息完整度：别名/描述/图标中非空字段数量越多越优先保留。 */
function itemRichness(item: Item): number {
  return [item.alias, item.description, item.icon].filter((v) => (v || '').trim()).length
}

/** 计算去重键：文件/文件夹按 类型+路径+名称，网站按规范化 URL，应用按 路径+参数。 */
function itemDedupeKey(item: Item): string {
  const name = (item.name || '').trim().toLowerCase()
  const url = (item.url || '').trim().toLowerCase()
  const args = (item.args || '').trim().toLowerCase()
  if (item.type === 'folder' || item.type === 'file') return `${item.type}|${name}|${url}`
  if (item.type === 'website') return `website|${url.replace(/\/+$/, '')}`
  if (item.type === 'application') return `application|${url}|${args}`
  return `${item.type}|${url}`
}

/** 去重：同一条目只保留信息最完整的一条（与后端 dedupe_items 规则一致）。 */
function dedupeItems(list: Item[]): Item[] {
  const best = new Map<string, { item: Item; richness: number }>()
  for (const item of list) {
    const key = itemDedupeKey(item)
    const current = best.get(key)
    if (!current) {
      best.set(key, { item, richness: itemRichness(item) })
      continue
    }
    const richness = itemRichness(item)
    if (richness > current.richness) best.set(key, { item, richness })
  }
  const kept = new Set([...best.values()].map((v) => v.item.id))
  return list.filter((item) => kept.has(item.id))
}

/** 判断当前数据中是否已存在相同的数据项（文件/文件夹按路径+名称，网站按 URL，应用按路径+参数）。 */
export function findDuplicateItem(candidate: Item): Item | undefined {
  const key = itemDedupeKey(candidate)
  return store.items.find((item) => item.id !== candidate.id && itemDedupeKey(item) === key)
}

export async function upsertItem(item: Item): Promise<Item> {
  const saved = await storage.saveItem(item)
  const idx = store.items.findIndex((i) => i.id === saved.id)
  if (idx >= 0) store.items.splice(idx, 1, saved)
  else store.items.unshift(saved)
  await storage.emitItemsChanged()
  return saved
}

export async function removeItem(id: number) {
  await storage.deleteItem(id)
  store.items = store.items.filter((i) => i.id !== id)
  await storage.emitItemsChanged()
}

export async function initStore(options: { loadItems?: boolean } = {}) {
  // 设置窗口只加载轻量设置（用于外观主题等）；只有启动器需要整表条目供搜索
  const withItems = options.loadItems !== false
  await storage.onSettingsChanged(receiveSettings)
  applySettings(await storage.loadSettings())
  if (withItems) {
    await reloadItems()
    await seedIfNeeded()
  }
  store.ready = true
  watch(() => toSettingsMap(store.settings), (current, previous) => {
    if (applyingRemote) return
    for (const key of Object.keys(current)) {
      if (current[key] !== previous[key]) pendingSettings[key] = current[key]
    }
    window.clearTimeout(settingsTimer)
    settingsTimer = window.setTimeout(() => void flushSettings(), 50)
  }, { flush: 'sync' })
  if (withItems) {
    storage.onItemsChanged(() => void reloadItems())
  }
  window.addEventListener('pagehide', () => void flushSettings())
}

async function seedIfNeeded() {
  if (isTauri) {
    const seeded = store.settings.lastSync !== '' || store.items.length > 0
    if (seeded) return
    await seedWebAndFolders()
    if (store.settings.autoSyncApps) {
      invoke('sync_apps').then(() => reloadItems()).catch(() => {})
    }
    return
  }
  // 浏览器预览模式：注入演示数据
  if (store.items.length === 0) {
    await seedWebAndFolders()
    for (const item of demoApps()) await storage.saveItem(item)
    await reloadItems()
  }
}

async function seedWebAndFolders() {
  const websites: Array<[string, string, string, string]> = [
    ['百度', 'baidu', 'https://www.baidu.com', '搜索引擎'],
    ['B站', 'bilibili,bili', 'https://www.bilibili.com', '视频平台'],
    ['GitHub', 'github,gh', 'https://github.com', '代码托管平台'],
    ['知乎', 'zhihu', 'https://www.zhihu.com', '知识社区'],
    ['淘宝', 'taobao', 'https://www.taobao.com', '购物平台'],
  ]
  for (const [name, alias, url, description] of websites) {
    await storage.saveItem({
      id: 0, icon: '', name, alias, type: 'website', url,
      description, enabled: true, updatedAt: '刚刚',
    })
  }
  let dirs: Record<string, string> = {}
  if (isTauri) {
    try {
      dirs = await invoke<Record<string, string>>('get_user_dirs')
    } catch {
      dirs = {}
    }
  }
  const folders: Array<[string, string, string]> = [
    ['桌面', 'desktop', dirs.desktop ?? 'C:\\Users\\user\\Desktop'],
    ['文档', 'docs,documents', dirs.documents ?? 'C:\\Users\\user\\Documents'],
    ['下载', 'downloads,download', dirs.downloads ?? 'C:\\Users\\user\\Downloads'],
    ['图片', 'pictures,pics', dirs.pictures ?? 'C:\\Users\\user\\Pictures'],
  ]
  for (const [name, alias, url] of folders) {
    await storage.saveItem({
      id: 0, icon: '', name, alias, type: 'folder', url,
      description: '', enabled: true, updatedAt: '刚刚',
    })
  }
  await reloadItems()
}

function demoApps(): Item[] {
  const now = '刚刚'
  return [
    { id: 900001, icon: '', name: 'Visual Studio Code', alias: 'vscode,code,vs', type: 'application', url: 'C:\\Program Files\\Microsoft VS Code\\Code.exe', description: '代码编辑器', enabled: true, updatedAt: now },
    { id: 900002, icon: '', name: 'Google Chrome', alias: 'chrome', type: 'application', url: 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe', description: '浏览器', enabled: true, updatedAt: now },
    { id: 900003, icon: '', name: 'Microsoft Edge', alias: 'edge', type: 'application', url: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', description: '浏览器', enabled: true, updatedAt: now },
  ]
}

// ---------- 搜索（需求优先级：精确名称 > 别名 > 路径 > 描述） ----------

export function scoreItem(item: Item, q: string): number {
  const name = item.name.toLowerCase()
  const aliases = item.alias
    .split(/[,，]/)
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean)
  const url = item.url.toLowerCase()
  const desc = (item.description || '').toLowerCase()
  if (name === q) return 100
  if (aliases.includes(q)) return 90
  if (name.startsWith(q)) return 80
  if (aliases.some((a) => a.startsWith(q))) return 75
  if (name.includes(q)) return 70
  if (aliases.some((a) => a.includes(q))) return 60
  if (url.includes(q)) return 50
  if (desc.includes(q)) return 40
  return 0
}

function categoryEnabled(t: ItemType): boolean {
  if (t === 'website') return store.settings.enabledWebsites
  if (t === 'folder' || t === 'file') return store.settings.enabledFolders
  return true
}

export function searchItems(q: string, limit = 6): Item[] {
  const query = q.trim().toLowerCase()
  if (!query) return []
  return store.items
    .filter((i) => i.enabled && categoryEnabled(i.type))
    .map((i) => ({ item: i, score: scoreItem(i, query) }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score || a.item.name.length - b.item.name.length)
    .slice(0, limit)
    .map((x) => x.item)
}
