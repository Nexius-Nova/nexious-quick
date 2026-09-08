<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { NButton, NSwitch, useMessage } from 'naive-ui'
import { isTauri } from '../adapter'
import { receiveSettings, store } from '../store'
import ShortcutPage from './ShortcutPage.vue'
import SearchPage from './SearchPage.vue'
import AboutPage from './AboutPage.vue'

const message = useMessage()
const checking = ref(true)
const saving = ref(false)
const togglingPin = ref(false)
const togglingTray = ref(false)
const startupError = ref('')

async function loadAutostart() {
  checking.value = true
  startupError.value = ''
  try {
    if (isTauri) receiveSettings({ autoStart: String(await invoke<boolean>('get_autostart')) })
  } catch (error) {
    startupError.value = `无法读取开机自启动状态：${String(error)}`
  } finally { checking.value = false }
}
async function setAutostart(enabled: boolean) {
  if (!isTauri || saving.value || checking.value) return
  const previous = store.settings.autoStart
  store.settings.autoStart = enabled
  saving.value = true
  try {
    const actual = await invoke<boolean>('set_autostart', { enabled })
    receiveSettings({ autoStart: String(actual) })
    message.success(actual ? '已开启开机自启动' : '已关闭开机自启动')
  } catch (error) {
    store.settings.autoStart = previous
    message.error(String(error))
  }
  finally { saving.value = false }
}
onMounted(loadAutostart)

async function setAlwaysOnTop(enabled: boolean) {
  if (!isTauri || togglingPin.value) return
  const previous = store.settings.alwaysOnTop
  store.settings.alwaysOnTop = enabled
  togglingPin.value = true
  try {
    await invoke('set_always_on_top', { enabled })
    receiveSettings({ alwaysOnTop: String(enabled) })
    message.success(enabled ? '已开启启动器置顶' : '已关闭启动器置顶')
  } catch (error) {
    store.settings.alwaysOnTop = previous
    message.error(String(error))
  } finally {
    togglingPin.value = false
  }
}

async function setCloseToTray(enabled: boolean) {
  if (!isTauri || togglingTray.value) return
  const previous = store.settings.closeToTray
  store.settings.closeToTray = enabled
  togglingTray.value = true
  try {
    await invoke('set_close_to_tray', { enabled })
    receiveSettings({ closeToTray: String(enabled) })
    message.success(enabled ? '关闭窗口时将驻留系统托盘' : '关闭窗口时将直接退出应用')
  } catch (error) {
    store.settings.closeToTray = previous
    message.error(String(error))
  } finally {
    togglingTray.value = false
  }
}
</script>

<template>
  <div class="page application-page">
    <div class="page-header"><h1>应用设置</h1></div>
    <section class="settings-section">
      <h2>启动与窗口</h2>
      <div class="switch-row">
        <span>开机自启动</span>
        <NSwitch :value="store.settings.autoStart" aria-label="开机自启动" :loading="checking || saving" :disabled="!isTauri || checking || saving || !!startupError" @update:value="setAutostart" />
      </div>
      <div v-if="startupError" class="field-error" role="alert">{{ startupError }} <NButton size="tiny" @click="loadAutostart">重试</NButton></div>
      <div class="switch-row switch-text-row">
        <div class="switch-text">
          <span>启动器窗口置顶</span>
          <small>启动器搜索框始终显示在其他窗口上方</small>
        </div>
        <NSwitch
          :value="store.settings.alwaysOnTop"
          aria-label="启动器窗口置顶"
          :loading="togglingPin"
          :disabled="!isTauri || togglingPin"
          @update:value="setAlwaysOnTop"
        />
      </div>
      <div class="switch-row switch-text-row">
        <div class="switch-text">
          <span>关闭窗口时驻留托盘</span>
          <small>开启后关闭窗口仅最小化到托盘并继续后台运行；关闭则直接退出应用</small>
        </div>
        <NSwitch
          :value="store.settings.closeToTray"
          aria-label="关闭窗口时驻留托盘"
          :loading="togglingTray"
          :disabled="!isTauri || togglingTray"
          @update:value="setCloseToTray"
        />
      </div>
      <div class="switch-row"><span>启动项目后隐藏搜索窗口</span><NSwitch v-model:value="store.settings.autoHide" aria-label="启动项目后隐藏搜索窗口" /></div>
      <div class="switch-row"><span>失去焦点时隐藏搜索窗口</span><NSwitch v-model:value="store.settings.hideOnBlur" aria-label="失去焦点时隐藏搜索窗口" /></div>
      <div class="switch-row"><span>鼠标悬停原位置唤起窗口</span><NSwitch v-model:value="store.settings.hoverShow" aria-label="鼠标悬停原位置唤起窗口" /></div>
    </section>
    <ShortcutPage />
    <SearchPage />
    <AboutPage />
  </div>
</template>
