<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { NAlert, NButton, NIcon, useMessage } from 'naive-ui'
import {
  AppsOutline,
  CloseOutline,
  ColorPaletteOutline,
  SettingsOutline,
  SquareOutline,
  CopyOutline,
  ExpandOutline,
  ContractOutline,
  ChevronBackOutline,
  ChevronForwardOutline,
  RemoveOutline,
} from '@vicons/ionicons5'
import { useRoute } from 'vue-router'
import { prefetchSettingsPages } from '../router'
import { isTauri } from '../adapter'
import { flushSettings, isDark, store } from '../store'
import BrandIcon from '../components/BrandIcon.vue'

type Page = 'data' | 'appearance' | 'application'
const route = useRoute()
const maximized = ref(false)
const fullscreen = ref(false)
const collapsed = ref(false)
const message = useMessage()

if (typeof window !== 'undefined') {
  collapsed.value = window.localStorage.getItem('nexious-settings-sidebar-collapsed') === 'true'
}

const navs: Array<{ key: Page; label: string; icon: unknown }> = [
  { key: 'data', label: '启动数据', icon: AppsOutline },
  { key: 'appearance', label: '外观设置', icon: ColorPaletteOutline },
  { key: 'application', label: '应用设置', icon: SettingsOutline },
]

function minimize() {
  if (isTauri) invoke('minimize_settings_window').catch(() => {})
}
async function closeWindow() {
  await flushSettings()
  if (store.settingsError) return
  if (isTauri) invoke('close_settings_window').catch(() => {})
}

async function toggleMaximize() {
  if (!isTauri) return
  try {
    await getCurrentWindow().toggleMaximize()
    maximized.value = await getCurrentWindow().isMaximized()
  } catch (error) { message.error(String(error)) }
}
async function toggleFullscreen() {
  try {
    if (isTauri) {
      const win = getCurrentWindow()
      await win.setFullscreen(!await win.isFullscreen())
      fullscreen.value = await win.isFullscreen()
    } else {
      if (document.fullscreenElement) await document.exitFullscreen()
      else await document.documentElement.requestFullscreen()
      fullscreen.value = !!document.fullscreenElement
    }
  } catch (error) { message.error(String(error)) }
}
function startDrag(event: MouseEvent) {
  if (event.button !== 0 || (event.target as HTMLElement).closest('button')) return
  if (isTauri) void getCurrentWindow().startDragging().catch((error) => message.error(String(error)))
}

function toggleSidebar() {
  collapsed.value = !collapsed.value
  window.localStorage.setItem('nexious-settings-sidebar-collapsed', String(collapsed.value))
}

let unFocus: (() => void) | null = null
let unResize: (() => void) | null = null
onUnmounted(() => {
  unFocus?.()
  unResize?.()
})
onMounted(() => {
  // 空闲时预取三个设置页，打包环境首次点开页面时无需等待动态加载
  prefetchSettingsPages()
  if (isTauri) {
    const win = getCurrentWindow()
    const refreshWindowState = async () => {
      maximized.value = await win.isMaximized()
      fullscreen.value = await win.isFullscreen()
    }
    void refreshWindowState()
    void win.onResized(() => void refreshWindowState()).then((off) => (unResize = off))
    getCurrentWindow()
      .onFocusChanged(({ payload }) => {
        // 重新聚焦时通知当前页面按需刷新各自的数据（每个页面各自加载自己的接口）
        if (payload) window.dispatchEvent(new CustomEvent('settings:focus'))
      })
      .then((off) => (unFocus = off))
      .catch(() => {})
  }
})
</script>

<template>
  <div class="settings-root" :class="{ dark: isDark }" :data-accent="store.settings.accent">
    <div class="titlebar" @mousedown="startDrag" @dblclick.self="toggleMaximize">
      <div class="tb-brand" @dblclick="toggleMaximize">
        <div class="brand-chip small"><BrandIcon :size="15" /></div>
        <b>快速启动</b>
      </div>
      <div class="tb-controls">
        <button class="tb-btn" aria-label="最小化" title="最小化" :disabled="!isTauri" @click="minimize">
          <NIcon :component="RemoveOutline" :size="16" />
        </button>
        <button class="tb-btn" :aria-label="maximized ? '还原' : '最大化'" :title="maximized ? '还原' : '最大化'" :disabled="!isTauri || fullscreen" @click="toggleMaximize">
          <NIcon :component="maximized ? CopyOutline : SquareOutline" :size="14" />
        </button>
        <button class="tb-btn" :aria-label="fullscreen ? '退出全屏' : '全屏'" :title="fullscreen ? '退出全屏' : '全屏'" @click="toggleFullscreen">
          <NIcon :component="fullscreen ? ContractOutline : ExpandOutline" :size="16" />
        </button>
        <button class="tb-btn close" aria-label="关闭" title="关闭" :disabled="!isTauri" @click="closeWindow">
          <NIcon :component="CloseOutline" :size="16" />
        </button>
      </div>
    </div>
    <div class="settings-body">
      <aside class="sider" :class="{ collapsed }">
        <nav class="nav">
          <router-link
            v-for="n in navs"
            :key="n.key"
            :to="{ name: n.key }"
            class="nav-item"
            :class="{ active: route.name === n.key }"
            :aria-current="route.name === n.key ? 'page' : undefined"
          >
            <NIcon :component="n.icon as never" :size="17" />
            <span>{{ n.label }}</span>
          </router-link>
        </nav>
        <div class="sider-foot">
          <!-- <div class="sider-footer">v1.0.0</div> -->
        </div>
        <button
          class="sider-trigger"
          type="button"
          :aria-label="collapsed ? '展开菜单' : '收起菜单'"
          :title="collapsed ? '展开菜单' : '收起菜单'"
          @click="toggleSidebar"
        >
          <NIcon :component="collapsed ? ChevronForwardOutline : ChevronBackOutline" :size="15" />
        </button>
      </aside>
      <main class="s-content">
        <NAlert v-if="store.settingsError" type="error" class="settings-error" :title="store.settingsError">
          <NButton size="small" :loading="store.savingSettings" @click="flushSettings">重试保存</NButton>
        </NAlert>
        <router-view v-slot="{ Component }">
          <Transition name="page" mode="out-in">
            <KeepAlive>
              <component :is="Component" :key="route.name ?? 'data'" />
            </KeepAlive>
          </Transition>
        </router-view>
      </main>
    </div>
  </div>
</template>
