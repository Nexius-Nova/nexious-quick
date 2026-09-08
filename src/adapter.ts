import { invoke } from '@tauri-apps/api/core'
import { emit, listen } from '@tauri-apps/api/event'
import type { Item } from './types'

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

const LS_ITEMS = 'nexious-items'
const LS_SETTINGS = 'nexious-settings'
export const ITEMS_CHANGED_EVENT = 'nexious:items-changed'
export const SETTINGS_CHANGED_EVENT = 'nexious:settings-changed'

function readLS<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    if (!raw) return fallback
    return JSON.parse(raw) as T
  } catch {
    return fallback
  }
}

function writeLS(key: string, value: unknown) {
  localStorage.setItem(key, JSON.stringify(value))
}

export const storage = {
  async loadItems(): Promise<Item[]> {
    if (!isTauri) return readLS<Item[]>(LS_ITEMS, [])
    return await invoke<Item[]>('db_load_items')
  },

  async saveItem(item: Item): Promise<Item> {
    if (!isTauri) {
      let list = readLS<Item[]>(LS_ITEMS, [])
      if (item.id > 0) {
        const index = list.findIndex((i) => i.id === item.id)
        if (index >= 0) list[index] = item
        else list.unshift(item)
      } else {
        item = { ...item, id: Math.max(Date.now(), ...list.map((i) => i.id + 1)) }
        list = [item, ...list]
      }
      writeLS(LS_ITEMS, list)
      return item
    }
    return await invoke<Item>('db_save_item', { item })
  },

  async deleteItem(id: number): Promise<void> {
    if (!isTauri) {
      writeLS(LS_ITEMS, readLS<Item[]>(LS_ITEMS, []).filter((i) => i.id !== id))
      return
    }
    await invoke('db_delete_item', { id })
  },

  async loadSettings(): Promise<Record<string, string>> {
    if (!isTauri) return readLS<Record<string, string>>(LS_SETTINGS, {})
    return await invoke<Record<string, string>>('db_load_settings')
  },

  async saveSettings(map: Record<string, string>): Promise<void> {
    if (!isTauri) {
      writeLS(LS_SETTINGS, { ...readLS<Record<string, string>>(LS_SETTINGS, {}), ...map })
      return
    }
    await invoke('db_save_settings', { settings: map })
  },

  async emitItemsChanged() {
    if (!isTauri) return
    try {
      await emit(ITEMS_CHANGED_EVENT, Date.now())
    } catch {
      /* ignore */
    }
  },

  onItemsChanged(cb: () => void): () => void {
    if (!isTauri) {
      const handler = (event: StorageEvent) => { if (event.key === LS_ITEMS) cb() }
      window.addEventListener('storage', handler)
      return () => window.removeEventListener('storage', handler)
    }
    let un: (() => void) | null = null
    listen(ITEMS_CHANGED_EVENT, cb).then((off) => (un = off)).catch(() => {})
    return () => un?.()
  },

  async onSettingsChanged(cb: (patch: Record<string, string>) => void): Promise<() => void> {
    if (isTauri) {
      return listen<Record<string, string>>(SETTINGS_CHANGED_EVENT, ({ payload }) => cb(payload))
    }
    const handler = (event: StorageEvent) => {
      if (event.key === LS_SETTINGS && event.newValue) {
        try { cb(JSON.parse(event.newValue)) } catch { /* Ignore invalid external data. */ }
      }
    }
    window.addEventListener('storage', handler)
    return () => window.removeEventListener('storage', handler)
  },
}
