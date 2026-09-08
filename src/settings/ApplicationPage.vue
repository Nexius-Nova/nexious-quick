<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { NButton, NSwitch, useMessage } from 'naive-ui'
import { isTauri } from '../adapter'
import { receiveSettings, store } from '../store'
import ShortcutPage from './ShortcutPage.vue'
import SearchPage from './SearchPage.vue'
import AboutPage from './AboutPage.vue'
import packageInfo from '../../package.json'

const message = useMessage()
const checking = ref(true)
const saving = ref(false)
const togglingPin = ref(false)
const togglingTray = ref(false)
const startupError = ref('')
const currentVersion = ref(packageInfo.version)
const checkingUpdate = ref(false)
type UpdateState = 'idle' | 'latest' | 'outdated' | 'error'
const updateState = ref<UpdateState>('idle')
const updateError = ref('')
const latestVersion = ref('')
const releaseUrl = ref('')

const UPDATE_REPO = 'Nexius-Nova/nexious-quick'
const RELEASES_URL = `https://github.com/${UPDATE_REPO}/releases/latest`
// 浏览器调试降级用；桌面端（Tauri）改走后端命令，避免 GitHub API 匿名限流
const UPDATE_API = `https://api.github.com/repos/${UPDATE_REPO}/releases/latest`

async function loadAppVersion() {
  if (isTauri) {
    try {
      currentVersion.value = await getVersion()
    } catch {
      /* 保持 package.json 版本 */
    }
  }
}

function versionParts(value: string): number[] {
  return value
    .replace(/^v/i, '')
    .split(/[.+-]/)
    .map((part) => Number.parseInt(part, 10) || 0)
}

function isNewerVersion(latest: string, current: string): boolean {
  const latestParts = versionParts(latest)
  const currentParts = versionParts(current)
  const length = Math.max(latestParts.length, currentParts.length)
  for (let i = 0; i < length; i += 1) {
    const diff = (latestParts[i] ?? 0) - (currentParts[i] ?? 0)
    if (diff !== 0) return diff > 0
  }
  return false
}

interface ReleaseCheckResult {
  found: boolean
  tag: string
  url: string
}

// 桌面端：走后端命令（GitHub 网页跳转），避免 api.github.com 匿名限流
async function fetchNativeRelease(): Promise<ReleaseCheckResult> {
  const result = await invoke<{ found: boolean; url: string }>('check_latest_release')
  if (!result.found) return { found: false, tag: '', url: '' }
  const tag = extractTagFromUrl(result.url)
  if (!tag) throw new Error('未能识别 GitHub 最新版本号')
  return { found: true, tag, url: result.url }
}

// 浏览器调试环境无法调用后端命令，降级走 GitHub API
async function fetchApiRelease(): Promise<ReleaseCheckResult> {
  const response = await fetch(UPDATE_API, {
    headers: { Accept: 'application/vnd.github+json' },
  })
  if (!response.ok) {
    if (response.status === 404) return { found: false, tag: '', url: '' }
    throw new Error(`GitHub 接口返回 ${response.status}`)
  }
  const data = await response.json()
  const tag = String(data?.tag_name ?? '').replace(/^v/i, '')
  if (!tag) throw new Error('GitHub 返回数据中缺少版本号')
  return { found: true, tag, url: String(data?.html_url ?? RELEASES_URL) }
}

// 从 GitHub Releases 跳转地址（…/releases/tag/v0.1.1）中提取不带 v 前缀的版本号
function extractTagFromUrl(url: string): string {
  const match = url.match(/\/tag\/(?:v)?([^/?#]+)/i)
  return match ? match[1] : ''
}

async function checkForUpdates() {
  if (checkingUpdate.value) return
  checkingUpdate.value = true
  updateState.value = 'idle'
  updateError.value = ''
  latestVersion.value = ''
  releaseUrl.value = ''
  try {
    const release = isTauri ? await fetchNativeRelease() : await fetchApiRelease()
    if (!release.found) {
      updateState.value = 'latest'
      return
    }
    latestVersion.value = release.tag
    releaseUrl.value = release.url || RELEASES_URL
    updateState.value = isNewerVersion(release.tag, currentVersion.value) ? 'outdated' : 'latest'
  } catch (error) {
    updateState.value = 'error'
    updateError.value = String(error)
  } finally {
    checkingUpdate.value = false
  }
}

function openReleasePage() {
  const url = releaseUrl.value || RELEASES_URL
  if (isTauri) {
    invoke('open_url', { url }).catch(() => {
      message.error('打开下载页面失败，请手动访问 GitHub Releases')
    })
  } else {
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}

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
onMounted(() => {
  void loadAutostart()
  void loadAppVersion()
})

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
    <section class="settings-section">
      <h2>软件更新</h2>
      <div class="update-row">
        <div class="update-main">
          <span class="update-label">当前版本 v{{ currentVersion }}</span>
          <span v-if="checkingUpdate" class="update-meta">正在检查更新…</span>
          <span v-else-if="updateState === 'latest'" class="update-meta ok">已是最新版本</span>
          <span v-else-if="updateState === 'outdated'" class="update-meta new">发现新版本 v{{ latestVersion }}，可前往下载安装包</span>
          <span v-else-if="updateState === 'error'" class="update-meta fail">检查失败：{{ updateError }}</span>
          <span v-else class="update-meta">检查 GitHub Releases 中发布的新版本</span>
        </div>
        <div class="update-actions">
          <NButton v-if="updateState === 'outdated'" size="small" type="primary" @click="openReleasePage">前往下载</NButton>
          <NButton secondary size="small" :loading="checkingUpdate" @click="checkForUpdates">
            {{ updateState === 'outdated' ? '再次检查' : '检查更新' }}
          </NButton>
        </div>
      </div>
    </section>
    <ShortcutPage />
    <SearchPage />
    <AboutPage />
  </div>
</template>
