<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useMessage } from 'naive-ui'
import { CloseOutline, SearchOutline, SettingsOutline } from '@vicons/ionicons5'
import { NIcon } from 'naive-ui'
import { motion } from 'motion-v'
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

const resultEase = [0.22, 1, 0.36, 1] as const

const animationProfile = computed(() => {
  switch (store.settings.animationEffect) {
    case 'reveal':
      // 从搜索框方向向下“展开”
      return {
        initial: { opacity: 0, y: -14, scaleY: 0.7 },
        animate: { opacity: 1, y: 0, scaleY: 1 },
        hover: { y: -1 },
      }
    case 'float':
      // 卡片从下方淡入、上浮到原位
      return {
        initial: { opacity: 0, y: 18, scale: 0.92 },
        animate: { opacity: 1, y: 0, scale: 1 },
        hover: { y: -2, x: 2 },
      }
    case 'spring':
      // 入场带弹簧回弹浮现
      return {
        initial: { opacity: 0, y: 14, scale: 0.95 },
        animate: { opacity: 1, y: 0, scale: 1 },
        hover: { y: -2 },
      }
    case 'spotlight':
      // 横向滑入，配合下方 rowAnimate 让选中项横移，形成“光标跟随”效果
      return {
        initial: { opacity: 0, x: -18 },
        animate: { opacity: 1, x: 0 },
        hover: {},
      }
    case 'crossfade':
    default:
      return {
        initial: { opacity: 0 },
        animate: { opacity: 1 },
        hover: {},
      }
  }
})

// spotlight：当前选中行额外向右滑出，聚焦反馈跟随光标/方向键移动
function rowAnimate(index: number) {
  const target = { ...animationProfile.value.animate }
  if (store.settings.animationEffect === 'spotlight' && index === selectedIndex.value) {
    target.x = 8
  }
  return target
}

function resultTransition(index = 0) {
  if (store.settings.animationEffect === 'spring') {
    return {
      type: 'spring' as const,
      stiffness: 380,
      damping: 30,
      mass: 0.6,
      delay: Math.min(index, 5) * 0.035,
    }
  }
  // 逐行错峰：延迟随行号递增，形成逐行出现/淡入的效果
  const stagger = store.settings.animationEffect === 'reveal' ? 0.04 : 0.05
  return {
    duration: store.settings.animationEffect === 'crossfade' ? 0.2 : 0.3,
    delay: Math.min(index, 5) * stagger,
    ease: resultEase,
  }
}

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
      invoke('resize_launcher', { width: store.settings.searchWidth, height: h }).catch(() => {})
    })
  })
}

watch([query, results, () => store.settings.showIcons, () => store.settings.searchWidth, () => store.settings.searchHeight], () => {
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
    :data-effect="store.settings.animationEffect"
    :class="{ dark: isDark }"
    :style="{
      '--panel-alpha': String(store.settings.opacity / 100),
      '--search-radius': `${store.settings.searchRadius}px`,
      '--search-height': `${store.settings.searchHeight}px`,
    }"
    @mouseenter="onPanelEnter"
    @mousedown="startDrag"
  >
    <div class="search-row">
      <button v-if="store.settings.showIcons" class="launcher-drag brand-chip" aria-label="拖动搜索窗口" title="拖动搜索窗口">
        <BrandIcon :size="18" />
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
      <motion.button
        v-if="query"
        class="icon-btn"
        aria-label="清除"
        :whileHover="{ scale: 1.08 }"
        :whilePress="{ scale: 0.9 }"
        @mousedown.stop
        @click="clearQuery"
      >
        <NIcon :component="CloseOutline" :size="16" />
      </motion.button>
      <motion.button
        class="icon-btn"
        aria-label="设置"
        title="设置"
        :whileHover="{ scale: 1.08, rotate: 8 }"
        :whilePress="{ scale: 0.9, rotate: -4 }"
        @mousedown.stop
        @click="openSettings"
      >
        <NIcon :component="SettingsOutline" :size="17" />
      </motion.button>
    </div>

    <div v-if="query.trim()" class="dropdown">
        <motion.div
          v-for="(r, idx) in results"
          :key="r.id"
          class="result-row"
          :class="{ active: idx === selectedIndex }"
          :data-selected="idx === selectedIndex"
          :initial="animationProfile.initial"
          :animate="rowAnimate(idx)"
          :transition="resultTransition(idx)"
          :whileHover="animationProfile.hover"
          :whilePress="{ scale: 0.985 }"
          @click="openItem(r)"
          @mousedown.stop
          @mouseenter="selectedIndex = idx"
        >
          <ItemIcon :icon="r.icon" :type="r.type" />
          <div class="result-text">
            <b>{{ r.name }}</b>
            <span>{{ r.url }}</span>
          </div>
          <span class="result-type">{{ TYPE_LABEL[r.type] }}</span>
          <span class="result-enter">↵</span>
        </motion.div>

        <motion.div
          key="search-elsewhere"
          class="result-row search-elsewhere"
          :class="{ active: selectedIndex === results.length }"
          :initial="animationProfile.initial"
          :animate="rowAnimate(results.length)"
          :transition="resultTransition(results.length)"
          :whileHover="animationProfile.hover"
          :whilePress="{ scale: 0.985 }"
          @click="webSearch"
          @mousedown.stop
          @mouseenter="selectedIndex = results.length"
        >
          <div class="result-icon plain">
            <NIcon :component="SearchOutline" :size="17" />
          </div>
          <div class="result-text single">
            <b>未找到？使用{{ store.settings.searchEngine }}搜索引擎搜索 “{{ query.trim() }}”</b>
          </div>
          <span class="result-enter">↵</span>
        </motion.div>
    </div>
  </div>
</template>
