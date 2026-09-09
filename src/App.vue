<script setup lang="ts">
import { computed, onMounted, ref, watchEffect } from 'vue'
import { darkTheme, NButton, NConfigProvider, NMessageProvider, zhCN, dateZhCN, type GlobalThemeOverrides } from 'naive-ui'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauri } from './adapter'
import { initStore, isDark, store } from './store'
import { accentTheme } from './types'
import Launcher from './Launcher.vue'
import Settings from './settings/Settings.vue'

type Kind = 'main' | 'settings'
const kind = ref<Kind>('main')
const ready = ref(false)
const error = ref('')
const themeOverrides = computed<GlobalThemeOverrides>(() => ({
  common: {
    primaryColor: isDark.value ? accentTheme(store.settings.accent).darkPrimary : accentTheme(store.settings.accent).primary,
    primaryColorHover: isDark.value ? accentTheme(store.settings.accent).darkHover : accentTheme(store.settings.accent).lightHover,
    primaryColorPressed: isDark.value ? accentTheme(store.settings.accent).darkPressed : accentTheme(store.settings.accent).lightPressed,
    primaryColorSuppl: isDark.value ? accentTheme(store.settings.accent).darkPrimary : accentTheme(store.settings.accent).primary,
    borderRadius: '6px',
    fontFamily: "'Segoe UI', 'Microsoft YaHei UI', sans-serif",
  },
}))
watchEffect(() => {
  document.documentElement.dataset.theme = isDark.value ? 'dark' : 'light'
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
    // 设置窗口无需整表条目（启动数据页按需加载自己的子集），只有启动器全量加载供搜索
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
