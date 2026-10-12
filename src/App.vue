<script setup lang="ts">
import { computed, onMounted, ref, watchEffect } from 'vue'
import { darkTheme, NButton, NConfigProvider, NMessageProvider, zhCN, dateZhCN, type GlobalThemeOverrides } from 'naive-ui'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauri } from './adapter'
import { initStore, isDark, store } from './store'
import { themeVariables, uiTheme } from './themes'
import Launcher from './Launcher.vue'
import Settings from './settings/Settings.vue'

type Kind = 'main' | 'settings'
const kind = ref<Kind>('main')
const ready = ref(false)
const error = ref('')
const activeTheme = computed(() => uiTheme(store.settings.accent))
const palette = computed(() => isDark.value ? activeTheme.value.dark : activeTheme.value.light)
const themeOverrides = computed<GlobalThemeOverrides>(() => ({
  common: {
    primaryColor: palette.value.accent,
    primaryColorHover: palette.value.accent,
    primaryColorPressed: palette.value.accent,
    primaryColorSuppl: palette.value.accent,
    bodyColor: palette.value.content,
    cardColor: palette.value.surface,
    modalColor: palette.value.surface,
    popoverColor: palette.value.surface,
    tableColor: palette.value.surface,
    inputColor: palette.value.input,
    actionColor: palette.value.input,
    borderColor: palette.value.border,
    dividerColor: palette.value.border,
    textColor1: palette.value.text,
    textColor2: palette.value.secondary,
    textColor3: palette.value.muted,
    placeholderColor: palette.value.muted,
    borderRadius: `${activeTheme.value.radius}px`,
    fontFamily: activeTheme.value.font,
  },
  Slider: { handleSize: '15px', railHeight: '3px' },
}))
watchEffect(() => {
  const root = document.documentElement
  root.dataset.theme = isDark.value ? 'dark' : 'light'
  root.dataset.uiStyle = activeTheme.value.key
  for (const [key, value] of Object.entries(themeVariables(activeTheme.value, isDark.value))) {
    root.style.setProperty(key, value)
  }
})

function detectWindow(): Kind {
  if (isTauri) {
    try {
      return getCurrentWindow().label === 'settings' ? 'settings' : 'main'
    } catch {
      return 'main'
    }
  }
  return new URLSearchParams(window.location.search).get('view') === 'settings' ? 'settings' : 'main'
}

async function initialize() {
  error.value = ''
  kind.value = detectWindow()
  try {
    // 设置页按需取数；启动器订阅数据变更并通过后端搜索。
    await initStore({ loadItems: kind.value !== 'settings' })
    ready.value = true
  } catch (cause) {
    error.value = `加载失败：${String(cause)}`
  }
}
onMounted(initialize)
</script>

<template>
  <NConfigProvider :theme="isDark ? darkTheme : null" :theme-overrides="themeOverrides" :locale="zhCN" :date-locale="dateZhCN">
    <NMessageProvider placement="bottom-right">
      <Launcher v-if="ready && kind === 'main'" />
      <Settings v-else-if="ready && kind === 'settings'" />
      <div v-else class="app-loading" role="status">
        {{ error || '正在加载...' }}
        <NButton v-if="error" size="small" @click="initialize">重试</NButton>
      </div>
    </NMessageProvider>
  </NConfigProvider>
</template>
