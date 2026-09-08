<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useMessage } from 'naive-ui'
import { CloseOutline, MoveOutline, SearchOutline, SettingsOutline } from '@vicons/ionicons5'
import { NIcon } from 'naive-ui'
import { isTauri } from './adapter'
import { isDark, reloadItems, searchItems, store } from './store'
import { SEARCH_ENGINES, TYPE_LABEL, type Item } from './types'
import BrandIcon from './components/BrandIcon.vue'
import ItemIcon from './components/ItemIcon.vue'

const message = useMessage()
const query = ref('')
const selectedIndex = ref(0)
const inputEl = ref<HTMLInputElement | null>(null)
const launcherEl = ref<HTMLElement | null>(null)
const launching = ref(false)

const results = computed(() => searchItems(query.value))
const searchRowUrl = computed(() =>
  (SEARCH_ENGINES[store.settings.searchEngine] ?? SEARCH_ENGINES.Google) + encodeURIComponent(query.value.trim()),
)
let unFocus: (() => void) | null = null
let resizeToken = 0
let resizeObserver: ResizeObserver | null = null

function startDrag(event: MouseEvent) {
  if (event.button !== 0 || !isTauri) return
  event.preventDefault()
  void getCurrentWindow().startDragging().catch((error) => message.error(`拖动窗口失败：${String(error)}`))
}

function scheduleResize() {
  if (!isTauri) return
  const token = ++resizeToken
  void nextTick(() => {
    window.requestAnimationFrame(() => {
      if (token !== resizeToken) return
      const panel = launcherEl.value
      if (!panel) return
      const h = Math.max(48, Math.ceil(panel.offsetHeight))
      invoke('resize_launcher', { height: h }).catch(() => {})
    })
  })
}

watch([query, results, () => store.settings.showIcons], () => {
  selectedIndex.value = 0
  scheduleResize()
})

function onKeydown(e: KeyboardEvent) {
  if (e.isComposing) return
  const max = results.value.length
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, max)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    void activate(selectedIndex.value)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    if (query.value) {
      query.value = ''
    } else if (isTauri) {
      invoke('hide_launcher').catch(() => {})
    }
  }
}

async function openItem(item: Item) {
  if (launching.value) return
  if (!isTauri) {
    if (item.type === 'website') window.open(item.url, '_blank', 'noopener,noreferrer')
    else message.info('请在桌面应用中打开本地启动项')
    return
  }
  launching.value = true
  try {
    await invoke('open_item', { item, keyword: query.value.trim() })
    query.value = ''
    if (store.settings.autoHide) await invoke('hide_launcher')
  } catch (error) {
    message.error(String(error))
  } finally {
    launching.value = false
  }
}

async function webSearch() {
  if (!query.value.trim() || launching.value) return
  if (!isTauri) {
    window.open(searchRowUrl.value, '_blank', 'noopener,noreferrer')
    return
  }
  launching.value = true
  try {
    await invoke('open_url', { url: searchRowUrl.value })
    query.value = ''
    if (store.settings.autoHide) await invoke('hide_launcher')
  } catch (error) {
    message.error(String(error))
  } finally {
    launching.value = false
  }
}

function activate(index: number) {
  if (index < results.value.length) openItem(results.value[index])
  else webSearch()
}

function openSettings() {
  if (!isTauri) {
    window.location.search = '?view=settings'
    return
  }
  invoke('show_settings_window').catch(() => {})
  invoke('hide_launcher').catch(() => {})
}

function clearQuery() {
  query.value = ''
  inputEl.value?.focus()
}

function onPanelEnter() {
  if (isTauri && store.settings.hoverShow) invoke('focus_launcher').catch(() => {})
}

onMounted(() => {
  inputEl.value?.focus()
  scheduleResize()
  if (isTauri && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(() => scheduleResize())
    if (launcherEl.value) resizeObserver.observe(launcherEl.value)
  }
  if (isTauri) {
    getCurrentWindow()
      .onFocusChanged(({ payload }) => {
        if (payload) {
          void reloadItems()
          inputEl.value?.focus()
        }
      })
      .then((off) => (unFocus = off))
      .catch(() => {})
  }
})
onUnmounted(() => {
  resizeObserver?.disconnect()
  unFocus?.()
})
</script>

<template>
  <div
    ref="launcherEl"
    class="launcher"
    :data-accent="store.settings.accent"
    :class="{ dark: isDark }"
    :style="{
      '--panel-alpha': String(store.settings.opacity / 100),
      '--search-radius': `${store.settings.searchRadius}px`,
    }"
    @mouseenter="onPanelEnter"
  >
    <div class="search-row" @mousedown="startDrag">
      <button class="launcher-drag" :class="store.settings.showIcons ? 'brand-chip' : 'icon-btn'" aria-label="拖动搜索窗口" title="拖动搜索窗口" @mousedown="startDrag">
        <BrandIcon v-if="store.settings.showIcons" :size="18" />
        <NIcon v-else :component="MoveOutline" :size="18" />
      </button>
      <input
        ref="inputEl"
        v-model="query"
        class="search-input"
        type="text"
        aria-label="搜索启动项"
        :placeholder="store.settings.placeholderText || '输入内容，快速启动...'"
        spellcheck="false"
        autocomplete="off"
        @mousedown.stop
        @keydown="onKeydown"
      />
      <button v-if="query" class="icon-btn" aria-label="清除" @mousedown.stop @click="clearQuery">
        <NIcon :component="CloseOutline" :size="16" />
      </button>
      <button class="icon-btn" aria-label="设置" title="设置" @mousedown.stop @click="openSettings">
        <NIcon :component="SettingsOutline" :size="17" />
      </button>
    </div>

    <div v-show="query.trim()" class="dropdown">
      <div
        v-for="(r, idx) in results"
        :key="r.id"
        class="result-row"
        :class="{ active: idx === selectedIndex }"
        :data-selected="idx === selectedIndex"
        @click="openItem(r)"
        @mouseenter="selectedIndex = idx"
      >
        <ItemIcon v-if="store.settings.showIcons" :icon="r.icon" :type="r.type" />
        <div class="result-text">
          <b>{{ r.name }}</b>
          <span>{{ r.url }}</span>
        </div>
        <span class="result-type">{{ TYPE_LABEL[r.type] }}</span>
        <span class="result-enter">↵</span>
      </div>

      <div
        class="result-row search-elsewhere"
        :class="{ active: selectedIndex === results.length }"
        @click="webSearch"
        @mouseenter="selectedIndex = results.length"
      >
        <div v-if="store.settings.showIcons" class="result-icon plain">
          <NIcon :component="SearchOutline" :size="17" />
        </div>
        <div class="result-text single">
          <b>未找到？使用{{ store.settings.searchEngine }}搜索引擎搜索 “{{ query.trim() }}”</b>
        </div>
        <span class="result-enter">↵</span>
      </div>
    </div>
  </div>
</template>
