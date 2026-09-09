<script setup lang="ts">
import { computed, h, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch, type Component } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { NButton, NForm, NFormItem, NIcon, NInput, NModal, NSelect, NSpace, NSwitch, useMessage, type SelectRenderLabel } from 'naive-ui'
import {
  AddOutline,
  AppsOutline,
  CreateOutline,
  DocumentOutline,
  DownloadOutline,
  FolderOpenOutline,
  FolderOutline,
  GlobeOutline,
  PlayOutline,
  RefreshOutline,
  SearchOutline,
  TrashOutline,
} from '@vicons/ionicons5'
import { isTauri, storage } from '../adapter'
import { findDuplicateAmong, isDark, loadItemsByKind, store } from '../store'
import { formatLastSync, TYPE_LABEL, type Item, type ItemType } from '../types'
import ItemIcon from '../components/ItemIcon.vue'
import FolderTree from './FolderTree.vue'

const message = useMessage()
type Tab = 'application' | 'website' | 'folder'
const tab = ref<Tab>('application')
const keyword = ref('')
const syncing = ref(false)
const syncResult = ref<{ added: number; updated: number; total: number } | null>(null)
const showModal = ref(false)
const editing = ref<Item | null>(null)
const form = ref<Partial<Item>>({})

const tabs: Array<{ key: Tab; label: string }> = [
  { key: 'application', label: '电脑应用' },
  { key: 'website', label: '网站链接' },
  { key: 'folder', label: '文件/文件夹' },
]

const sectionTitle = computed(() =>
  tab.value === 'application' ? '电脑应用' : tab.value === 'website' ? '网站链接' : '文件/文件夹',
)
const syncButtonText = computed(() => {
  if (syncing.value) return '正在同步...'
  if (tab.value === 'website') return '同步收藏夹'
  if (tab.value === 'folder') return '同步本地目录'
  return '同步应用'
})

// 启动数据页通过后端分页检索取数，内存只保留当前页条目，避免整表
// （尤其整盘同步后可达数万条）一次性回传前端造成卡顿。
const items = ref<Item[]>([])
const totalCount = ref(0)
const itemsLoading = ref(false)
// 路由切走（KeepAlive 缓存）后不再响应刷新，避免后台重复拉取
const pageActive = ref(true)
let itemsReloadTimer: ReturnType<typeof window.setTimeout> | undefined
let itemsOff: (() => void) | null = null
// 请求序号：只采纳最后一次发起的取数结果，避免快速输入 / 外部刷新时旧响应覆盖新列表
let loadSeq = 0

// 列表分页加载，每页最多渲染 PAGE_CHUNK 条
const PAGE_CHUNK = 120
const visibleCount = ref(PAGE_CHUNK)

function currentKind(): 'application' | 'website' | 'folder' {
  return tab.value === 'application' || tab.value === 'website' ? tab.value : 'folder'
}

async function loadRows() {
  const seq = ++loadSeq
  itemsLoading.value = true
  try {
    if (isTauri) {
      const page = await invoke<{ items: Item[]; total: number }>('db_page_items', {
        kind: currentKind(),
        keyword: keyword.value.trim(),
        offset: 0,
        limit: visibleCount.value,
      })
      if (seq !== loadSeq) return
      items.value = page.items
      totalCount.value = page.total
    } else {
      const all = await loadItemsByKind(currentKind())
      const q = keyword.value.trim().toLowerCase()
      const filtered = q
        ? all.filter((i) => [i.name, i.alias, i.url, i.description].join(' ').toLowerCase().includes(q))
        : all
      if (seq !== loadSeq) return
      totalCount.value = filtered.length
      items.value = filtered.slice(0, visibleCount.value)
    }
  } catch (error) {
    if (seq === loadSeq) message.error(`加载${sectionTitle.value}失败：${String(error)}`)
  } finally {
    if (seq === loadSeq) itemsLoading.value = false
  }
}

function scheduleRowsReload() {
  if (!pageActive.value) return
  window.clearTimeout(itemsReloadTimer)
  itemsReloadTimer = window.setTimeout(() => void loadRows(), 300)
}

// 其它窗口增删改、本页增删改或窗口重新聚焦后，当前标签页重新拉取自己的子集
itemsOff = storage.onItemsChanged(() => scheduleRowsReload())
window.addEventListener('settings:focus', scheduleRowsReload)

async function loadMoreRows() {
  if (items.value.length >= totalCount.value) return
  if (!isTauri) {
    visibleCount.value += PAGE_CHUNK
    void loadRows()
    return
  }
  const seq = ++loadSeq
  itemsLoading.value = true
  try {
    const page = await invoke<{ items: Item[]; total: number }>('db_page_items', {
      kind: currentKind(),
      keyword: keyword.value.trim(),
      offset: items.value.length,
      limit: PAGE_CHUNK,
    })
    if (seq !== loadSeq) return
    const seen = new Set(items.value.map((i) => i.id))
    items.value = items.value.concat(page.items.filter((i) => !seen.has(i.id)))
    totalCount.value = page.total
    visibleCount.value = items.value.length
  } catch (error) {
    if (seq === loadSeq) message.error(`加载更多失败：${String(error)}`)
  } finally {
    if (seq === loadSeq) itemsLoading.value = false
  }
}
watch(keyword, () => {
  visibleCount.value = PAGE_CHUNK
  scheduleRowsReload()
})

const lastSyncText = computed(() => {
  if (tab.value === 'website') return formatLastSync(store.settings.lastWebsiteSync)
  if (tab.value === 'folder') return formatLastSync(store.settings.lastFolderSync)
  return formatLastSync(store.settings.lastSync)
})
const autoSyncModel = computed({
  get: () => store.settings.autoSyncApps,
  set: (v: boolean) => (store.settings.autoSyncApps = v),
})
const enabledWebsitesModel = computed({
  get: () => store.settings.enabledWebsites,
  set: (v: boolean) => (store.settings.enabledWebsites = v),
})
const enabledFoldersModel = computed({
  get: () => store.settings.enabledFolders,
  set: (v: boolean) => (store.settings.enabledFolders = v),
})

interface FolderRoot {
  path: string
  name: string
  recursive: boolean
}
const folderRoots = ref<FolderRoot[]>([])
const quickRoots = ref<FolderRoot[]>([])
const folderManagerOpen = ref(false)
const rootsLoading = ref(false)

const addedPaths = computed(() => folderRoots.value.map((r) => r.path))

function displayRootName(root: FolderRoot): string {
  const quick = quickRoots.value.find((q) => q.path.toLowerCase() === root.path.toLowerCase())
  return quick ? quick.name : root.name
}

function isQuickAdded(path: string): boolean {
  return folderRoots.value.some((r) => r.path.toLowerCase() === path.toLowerCase())
}

async function refreshFolderRoots() {
  if (!isTauri) return
  rootsLoading.value = true
  try {
    folderRoots.value = await invoke<FolderRoot[]>('get_folder_roots')
  } catch (err) {
    message.error(`读取同步目录失败：${String(err)}`)
  } finally {
    rootsLoading.value = false
  }
}

async function loadQuickRoots() {
  if (!isTauri) return
  try {
    quickRoots.value = await invoke<FolderRoot[]>('default_folder_roots')
  } catch {
    quickRoots.value = []
  }
}

async function openFolderManager() {
  if (!isTauri) return
  folderManagerOpen.value = true
  await Promise.all([refreshFolderRoots(), loadQuickRoots()])
}

async function addFolderRoot(path: string) {
  if (!path) return
  const previousCount = folderRoots.value.length
  try {
    folderRoots.value = await invoke<FolderRoot[]>('add_folder_root', { path })
    if (folderRoots.value.length === previousCount) {
      message.info('该目录已在同步列表中')
      return
    }
    const added = folderRoots.value.find((r) => r.path.toLowerCase() === path.toLowerCase())
    const label = added ? displayRootName(added) : path
    message.success(`已加入并开始同步「${label}」…`)
    // 新目录加入后立即单独同步一次，便于马上在启动器中搜索到
    syncing.value = true
    try {
      const r = await invoke<{ added: number; updated: number; total: number }>('sync_folders', {
        onlyPath: path,
      })
      syncResult.value = r
      if (r.total === 0) message.warning(`「${label}」没有扫描到可索引的文件或文件夹`)
      else message.success(`「${label}」同步完成：新增 ${r.added} 项，更新 ${r.updated} 项`)
    } catch (syncErr) {
      message.error(`同步「${label}」失败：${String(syncErr)}`)
    } finally {
      syncing.value = false
    }
  } catch (err) {
    message.error(String(err))
  }
}

async function removeFolderRoot(path: string) {
  try {
    folderRoots.value = await invoke<FolderRoot[]>('remove_folder_root', { path })
    message.success('已从同步列表移除')
  } catch (err) {
    message.error(String(err))
  }
}

async function browseNativeFolderRoot() {
  if (!isTauri) return
  try {
    const picked = await invoke<string | null>('plugin:dialog|open', {
      options: { title: '选择要同步的目录', multiple: false, directory: true },
    })
    if (picked) await addFolderRoot(picked)
  } catch (err) {
    message.error(`打开目录选择器失败：${String(err)}`)
  }
}

watch(tab, (current) => {
  visibleCount.value = PAGE_CHUNK
  if (current === 'folder' && isTauri) void refreshFolderRoots()
  void loadRows()
})

onMounted(() => {
  if (tab.value === 'folder' && isTauri) void refreshFolderRoots()
  void loadRows()
})
onActivated(() => {
  pageActive.value = true
  void loadRows()
})
onDeactivated(() => {
  pageActive.value = false
})
onUnmounted(() => {
  window.removeEventListener('settings:focus', scheduleRowsReload)
  window.clearTimeout(itemsReloadTimer)
  itemsOff?.()
})

const colName = computed(() =>
  tab.value === 'website' ? '网址' : tab.value === 'folder' ? '路径' : '名称',
)

const isImageIcon = (icon: string) => icon.startsWith('data:')
const iconLetter = (item: Item) => item.name.charAt(0).toUpperCase()
function iconStyle(item: Item) {
  let hash = 0
  for (const ch of item.name) hash = (hash * 31 + ch.codePointAt(0)!) % 360
  return { background: `hsl(${hash}, 62%, 52%)` }
}

async function sync() {
  syncing.value = true
  syncResult.value = null
  try {
    if (isTauri) {
      if (tab.value === 'folder') {
        await refreshFolderRoots()
        if (!folderRoots.value.length) {
          message.warning('尚未添加同步目录，请先点击「浏览此电脑目录树」选择要同步的目录')
          return
        }
      }
      const command = tab.value === 'website' ? 'sync_bookmarks' : tab.value === 'folder' ? 'sync_folders' : 'sync_apps'
      const r = await invoke<{ added: number; updated: number; total: number }>(command)
      syncResult.value = r
      if (r.total === 0) message.warning(tab.value === 'website' ? '未找到可同步的浏览器收藏夹' : '未找到可同步的数据')
      else message.success(`同步完成：新增 ${r.added} 项，更新 ${r.updated} 项`)
      // 同步写入较多，稍后重新拉取当前标签页数据
      scheduleRowsReload()
    } else {
      message.success('浏览器预览模式不支持本地同步')
    }
  } catch (err) {
    message.error(String(err))
  } finally {
    syncing.value = false
  }
}

async function persistLocal(item: Item): Promise<Item> {
  const saved = await storage.saveItem(item)
  const idx = items.value.findIndex((i) => i.id === saved.id)
  if (idx >= 0) items.value.splice(idx, 1, saved)
  else items.value.unshift(saved)
  await storage.emitItemsChanged()
  return saved
}

function startAdd() {
  editing.value = null
  form.value = {
    name: '',
    alias: '',
    type: tab.value === 'folder' ? 'folder' : tab.value,
    url: '',
    args: '',
    description: '',
    icon: '',
    enabled: true,
  }
  showModal.value = true
}

function editItem(item: Item) {
  editing.value = item
  form.value = { ...item }
  showModal.value = true
}

async function saveItem() {
  const f = form.value
  if (!f.name?.trim() || !f.url?.trim()) {
    message.warning('请填写名称和路径 / 网址')
    return
  }
  if (f.type === 'website' && !/^https?:\/\//i.test(f.url.trim())) {
    message.warning('网站链接需以 http:// 或 https:// 开头')
    return
  }
  const item: Item = {
    id: editing.value?.id ?? 0,
    icon: f.icon || '',
    name: f.name.trim(),
    alias: (f.alias || '').trim(),
    type: (f.type || 'application') as ItemType,
    url: f.url.trim(),
    args: f.args || '',
    workdir: editing.value?.workdir || '',
    description: f.description || '',
    enabled: f.enabled !== false,
    updatedAt: '刚刚',
  }
  const duplicate = findDuplicateAmong(items.value, item)
  if (duplicate) {
    const reason =
      item.type === 'website'
        ? '相同网址的链接已存在'
        : item.type === 'application'
          ? '相同路径和参数的应用已存在'
          : '相同路径和名称的文件/文件夹已存在'
    message.warning(`无法添加：「${duplicate.name}」已存在（${reason}），请直接编辑该条目`)
    return
  }
  try {
    const saved = await persistLocal(item)
    // 网站项未带图标时，保存后仍在后台自动补全图标，不阻塞保存流程
    if (saved.type === 'website' && !saved.icon && isTauri && validWebsiteUrl(saved.url)) {
      void requestIcon(saved.url).then((icon) => {
        if (!icon) return
        const fresh = items.value.find((i) => i.id === saved.id)
        if (fresh && !fresh.icon) {
          void persistLocal({ ...fresh, icon, updatedAt: '刚刚' }).catch(() => {})
        }
      })
    }
    showModal.value = false
    message.success('已保存，立即可搜索')
  } catch (err) {
    message.error(String(err))
  }
}

async function onDelete(item: Item) {
  try {
    await storage.deleteItem(item.id)
    items.value = items.value.filter((i) => i.id !== item.id)
    await storage.emitItemsChanged()
    message.success(`已删除「${item.name}」`)
  } catch (err) {
    message.error(String(err))
  }
}

const launching = ref(new Set<number>())

function setLaunching(id: number, on: boolean) {
  const next = new Set(launching.value)
  if (on) next.add(id)
  else next.delete(id)
  launching.value = next
}

function isLaunching(id: number): boolean {
  return launching.value.has(id)
}

async function launchItem(item: Item) {
  if (isLaunching(item.id)) return
  if (!isTauri) {
    if (item.type === 'website') {
      window.open(item.url, '_blank', 'noopener,noreferrer')
      return
    }
    message.info('请在桌面应用中打开本地启动项')
    return
  }
  setLaunching(item.id, true)
  try {
    await invoke('open_item', { item, keyword: '' })
    message.success(`已启动「${item.name}」`)
  } catch (err) {
    message.error(String(err))
  } finally {
    setLaunching(item.id, false)
  }
}

function onIconUpload(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  if (!file.type.startsWith('image/')) {
    message.warning('请选择图片文件')
    return
  }
  if (file.size > 2 * 1024 * 1024) {
    message.warning('图标文件不能超过 2 MB')
    return
  }
  const reader = new FileReader()
  reader.onload = () => (form.value.icon = String(reader.result))
  reader.readAsDataURL(file)
}

function validWebsiteUrl(value: string): boolean {
  return /^https?:\/\/\S+$/i.test(value.trim())
}

const fetchingIcon = ref(false)
let autoIconTimer: ReturnType<typeof window.setTimeout> | undefined
let iconInflightUrl = ''
let iconInflight: Promise<string> | null = null

function canAutoFetchIcon(): boolean {
  return (
    isTauri &&
    showModal.value &&
    form.value.type === 'website' &&
    !form.value.icon &&
    !fetchingIcon.value &&
    validWebsiteUrl(form.value.url || '')
  )
}

function requestIcon(url: string): Promise<string> {
  const key = url.trim()
  // 同一网址的并发请求复用一次，避免重复抓取
  if (iconInflight && iconInflightUrl === key) return iconInflight
  iconInflightUrl = key
  iconInflight = (async () => {
    try {
      return await invoke<string>('resolve_website_icon', { url: key })
    } catch {
      return ''
    } finally {
      iconInflight = null
      iconInflightUrl = ''
    }
  })()
  return iconInflight
}

async function fetchWebsiteIcon(manual = false) {
  const url = (form.value.url || '').trim()
  if (!validWebsiteUrl(url)) {
    if (manual) message.warning('请输入以 http:// 或 https:// 开头的网址')
    return
  }
  if (fetchingIcon.value) return
  window.clearTimeout(autoIconTimer)
  fetchingIcon.value = true
  try {
    const icon = await requestIcon(url)
    if (icon && form.value.type === 'website') {
      form.value.icon = icon
      if (manual) message.success('已自动获取网站图标')
    } else if (manual) {
      message.info('未能获取到该网站的图标，可稍后重试或手动上传')
    }
  } finally {
    fetchingIcon.value = false
  }
}

// 输入网址后自动尝试获取图标（后台进行，不阻塞填写与保存）
watch(
  () => [form.value.type, form.value.url, form.value.icon],
  () => {
    window.clearTimeout(autoIconTimer)
    if (canAutoFetchIcon()) {
      autoIconTimer = window.setTimeout(() => {
        if (canAutoFetchIcon()) void fetchWebsiteIcon(false)
      }, 800)
    }
  },
  { immediate: true },
)

function onTypeChange(value: string) {
  const next = value as ItemType
  form.value.type = next
  tab.value = next === 'file' ? 'folder' : next as Tab
}

const typeOptions = (Object.keys(TYPE_LABEL) as ItemType[]).map((value) => ({
  label: TYPE_LABEL[value],
  value,
}))

const typeIcons: Record<ItemType, Component> = {
  application: AppsOutline,
  website: GlobeOutline,
  folder: FolderOutline,
  file: DocumentOutline,
}

const renderTypeLabel: SelectRenderLabel = (option, selected) =>
  h('span', { class: ['type-option', { selected }] }, [
    h(NIcon, { component: typeIcons[option.value as ItemType], size: 15 }),
    String(option.label ?? ''),
  ])

const pathLabel = computed(() => {
  if (form.value.type === 'website') return '网址'
  if (form.value.type === 'application') return '应用路径'
  return '路径'
})

const urlPlaceholder = computed(() => {
  switch (form.value.type) {
    case 'website':
      return 'https://www.example.com'
    case 'folder':
    case 'file':
      return 'D:\\Projects 或 C:\\Users\\...\\file.txt'
    default:
      return 'C:\\Program Files\\...\\app.exe'
  }
})

async function browsePath() {
  if (!isTauri) return
  const isApp = form.value.type === 'application'
  try {
    const picked = await invoke<string | null>('plugin:dialog|open', {
      options: isApp
        ? {
            title: '选择应用程序',
            multiple: false,
            filters: [{ name: '应用程序', extensions: ['exe', 'lnk', 'bat', 'cmd'] }],
          }
        : { title: '选择文件 / 文件夹', multiple: false, directory: true },
    })
    if (picked) form.value.url = picked
  } catch (err) {
    message.error(`打开文件选择器失败：${String(err)}`)
  }
}
</script>

<template>
  <div class="page" :class="{ dark: isDark }">
    <div class="page-header">
      <div>
        <h1>启动数据</h1>
        <p>管理你的应用启动项，支持电脑应用、网站链接和文件/文件夹。</p>
      </div>
      <button class="ghost-btn" :disabled="syncing" @click="sync">
        <NIcon :component="RefreshOutline" :size="15" :class="{ spinning: syncing }" />
        {{ syncButtonText }}
      </button>
    </div>

    <div class="tab-bar">
      <button
        v-for="t in tabs"
        :key="t.key"
        class="tab-item"
        :class="{ active: tab === t.key }"
        @click="tab = t.key"
      >
        {{ t.label }}
      </button>
    </div>

    <div class="section">
      <div class="section-head">
        <div>
          <div class="section-title">{{ sectionTitle }}</div>
          <label v-if="tab === 'application'" class="toggle-line">
            <NSwitch v-model:value="autoSyncModel" size="small" />
            <span>自动同步电脑已安装的应用</span>
          </label>
          <label v-else-if="tab === 'website'" class="toggle-line">
            <NSwitch v-model:value="enabledWebsitesModel" size="small" />
            <span>允许在快速启动结果中显示网站链接</span>
          </label>
          <label v-else class="toggle-line">
            <NSwitch v-model:value="enabledFoldersModel" size="small" />
            <span>允许在快速启动结果中显示文件/文件夹</span>
          </label>
          <div class="section-sub">
            上次同步 · {{ lastSyncText }}
            <span v-if="tab === 'website'"> · 支持 Chrome、Edge、Brave 收藏夹</span>
            <span v-else-if="tab === 'folder'"> · 可在下方浏览整台电脑，自由添加要同步的目录</span>
          </div>
        </div>
        <div class="section-tools">
          <NInput v-model:value="keyword" size="small" clearable :placeholder="`搜索${sectionTitle}名称...`" class="table-search">
            <template #prefix><NIcon :component="SearchOutline" :size="14" /></template>
          </NInput>
          <button class="primary-btn" type="button" @click="startAdd">
            <NIcon :component="AddOutline" :size="15" />
            添加
          </button>
        </div>
      </div>

      <div v-if="tab === 'folder'" class="sync-dirs">
        <div class="sync-dirs-head">
          <div class="sync-dirs-info">
            <span class="sync-dirs-title">同步目录</span>
            <span class="sync-dirs-sub">点「同步本地目录」会把所选目录里的文件/文件夹索引为可搜索的启动项（自动跳过隐藏与系统目录，按广度覆盖各子目录，单个目录上限约 2 万项）。</span>
          </div>
          <div class="sync-dirs-actions">
            <button v-if="isTauri" class="ghost-btn small" type="button" @click="openFolderManager">
              <NIcon :component="FolderOpenOutline" :size="15" />
              浏览此电脑目录树
            </button>
            <button v-if="isTauri" class="ghost-btn small" type="button" @click="browseNativeFolderRoot">
              <NIcon :component="FolderOutline" :size="15" />
              系统浏览…
            </button>
          </div>
        </div>

        <div v-if="rootsLoading" class="sync-dirs-empty">正在读取同步目录…</div>
        <div v-else-if="folderRoots.length" class="sync-dir-chips">
          <span v-for="root in folderRoots" :key="root.path" class="sync-dir-chip" :title="root.path">
            <NIcon :component="FolderOutline" :size="14" />
            <span class="sync-dir-chip-name">{{ displayRootName(root) }}</span>
            <span class="sync-dir-chip-path">{{ root.path }}</span>
            <button type="button" class="sync-dir-chip-remove" title="移出同步" @click="removeFolderRoot(root.path)">
              <NIcon :component="TrashOutline" :size="13" />
            </button>
          </span>
        </div>
        <div v-else class="sync-dirs-empty">
          <template v-if="isTauri">
            尚未添加同步目录，点击「浏览此电脑目录树」选择任意目录（也可用「系统浏览…」通过资源管理器选择）。
          </template>
          <template v-else>浏览器预览模式不支持浏览本机目录。</template>
        </div>
      </div>

      <div v-if="syncResult" class="sync-result" role="status">
        本次同步：新增 {{ syncResult.added }} 项，更新 {{ syncResult.updated }} 项，共 {{ syncResult.total }} 项
      </div>

      <table class="data-table">
        <thead>
          <tr>
            <th class="col-icon">图标</th>
            <th>名称</th>
            <th>别名</th>
            <th>类型</th>
            <th v-if="tab !== 'application'">{{ colName }}</th>
            <th>描述</th>
            <th class="col-actions">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in items" :key="r.id">
            <td class="col-icon">
              <ItemIcon :icon="r.icon" :type="r.type" />
            </td>
            <td class="td-name" :title="r.name">{{ r.name }}</td>
            <td class="td-muted" :title="r.alias || ''">{{ r.alias || '—' }}</td>
            <td class="td-muted" :title="TYPE_LABEL[r.type]">{{ TYPE_LABEL[r.type] }}</td>
            <td v-if="tab !== 'application'" class="td-muted td-url" :title="r.url">{{ r.url }}</td>
            <td class="td-muted" :title="r.description || ''">{{ r.description || '—' }}</td>
            <td class="col-actions">
              <button
                class="row-btn launch"
                :class="{ spinning: isLaunching(r.id) }"
                :disabled="isLaunching(r.id)"
                title="启动"
                @click="launchItem(r)"
              >
                <NIcon :component="PlayOutline" :size="14" />
              </button>
              <button class="row-btn" title="编辑" @click="editItem(r)">
                <NIcon :component="CreateOutline" :size="15" />
              </button>
              <button class="row-btn danger" title="删除" @click="onDelete(r)">
                <NIcon :component="TrashOutline" :size="15" />
              </button>
            </td>
          </tr>
          <tr v-if="itemsLoading">
            <td colspan="7" class="empty table-loading">
              <NIcon :component="RefreshOutline" :size="14" class="spinning" />
              正在加载数据…
            </td>
          </tr>
          <tr v-else-if="!items.length">
            <td colspan="7" class="empty">暂无数据，点击右上角「添加」创建，或在「电脑应用」中手动同步。</td>
          </tr>
        </tbody>
      </table>
      <div v-if="totalCount > items.length" class="data-table-more">
        <button class="ghost-btn small" type="button" @click="loadMoreRows">
          已显示前 {{ items.length }} 条，共 {{ totalCount }} 条 · 加载更多
        </button>
      </div>
    </div>

    <NModal
      v-model:show="showModal"
      preset="card"
      :title="editing ? '编辑启动项' : '添加启动项'"
      class="item-modal"
      :style="{ width: '640px', maxWidth: 'calc(100vw - 32px)' }"
      :bordered="false"
      size="small"
    >
      <div class="item-form-layout">
        <label class="item-form-icon" title="上传图标">
          <span class="icon-upload-card">
            <img v-if="isImageIcon(form.icon || '')" :src="form.icon" alt="图标预览" />
            <span v-else-if="form.icon" class="emoji">{{ form.icon }}</span>
            <ItemIcon v-else :type="(form.type || 'application') as ItemType" />
          </span>
          <span class="icon-upload-link">上传图标</span>
          <input type="file" accept="image/png,image/jpeg,image/webp,image/svg+xml" hidden @change="onIconUpload" />
        </label>
        <NForm class="item-form-fields" label-placement="top" :show-feedback="false" require-mark-placement="right">
          <NFormItem label="名称" required>
            <NInput v-model:value="form.name" placeholder="例如：Visual Studio Code" />
          </NFormItem>
          <NFormItem label="别名">
            <NInput v-model:value="form.alias" placeholder="用逗号分隔，如 vscode,code" />
          </NFormItem>
          <NFormItem label="类型" required>
            <NSelect :value="form.type as string" :options="typeOptions" :render-label="renderTypeLabel" @update:value="onTypeChange" />
          </NFormItem>
          <NFormItem :label="pathLabel" required>
            <NInput v-model:value="form.url" :placeholder="urlPlaceholder">
              <template #suffix>
                <template v-if="form.type === 'website' && isTauri">
                  <button
                    type="button"
                    class="url-fetch-btn"
                    :disabled="fetchingIcon || !validWebsiteUrl(form.url || '')"
                    title="自动获取该网站的图标"
                    @click="fetchWebsiteIcon(true)"
                  >
                    <NIcon :component="fetchingIcon ? RefreshOutline : DownloadOutline" :size="13" :class="{ spinning: fetchingIcon }" />
                    <span>{{ fetchingIcon ? '获取中…' : '自动获取' }}</span>
                  </button>
                </template>
                <button v-else-if="form.type !== 'website' && isTauri" type="button" class="path-browse" title="浏览..." @click="browsePath">
                  <NIcon :component="FolderOpenOutline" :size="16" />
                </button>
              </template>
            </NInput>
          </NFormItem>
          <NFormItem label="描述">
            <NInput v-model:value="form.description" type="textarea" :autosize="{ minRows: 2, maxRows: 4 }" placeholder="可选描述" />
          </NFormItem>
        </NForm>
      </div>
      <template #footer>
        <NSpace justify="end" :size="12">
          <NButton @click="showModal = false">取消</NButton>
          <NButton type="primary" @click="saveItem">确定</NButton>
        </NSpace>
      </template>
    </NModal>

    <NModal
      v-model:show="folderManagerOpen"
      preset="card"
      title="添加同步目录 · 浏览整个电脑"
      class="folder-manager-modal"
      :style="{ width: '780px', maxWidth: 'calc(100vw - 32px)' }"
      :bordered="false"
      size="small"
    >
      <div class="folder-manager">
        <div class="fm-quick">
          <span class="fm-label">快捷添加：</span>
          <template v-if="quickRoots.length">
            <button
              v-for="q in quickRoots"
              :key="q.path"
              class="ghost-btn small"
              :disabled="isQuickAdded(q.path)"
              type="button"
              @click="addFolderRoot(q.path)"
            >
              <NIcon :component="FolderOutline" :size="14" />
              {{ q.name }}
              <span v-if="isQuickAdded(q.path)">✓</span>
            </button>
          </template>
          <button v-else class="ghost-btn small" type="button" @click="openFolderManager">
            <NIcon :component="RefreshOutline" :size="14" />
            重试
          </button>
          <button class="ghost-btn small" type="button" @click="browseNativeFolderRoot">
            <NIcon :component="FolderOpenOutline" :size="14" />
            系统浏览…
          </button>
        </div>
        <div class="fm-tree-label">目录树（点击箭头 / 文件夹展开）</div>
        <div class="fm-tree-panel">
          <FolderTree :added-paths="addedPaths" @add="addFolderRoot" />
        </div>
        <p class="fm-hint">
          已加入的目录会显示在上方「同步目录」中；回到页面点击「同步本地目录」即可把其中的文件与文件夹索引为可搜索启动项。
        </p>
      </div>
    </NModal>
  </div>
</template>
