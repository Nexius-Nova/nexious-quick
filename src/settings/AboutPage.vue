<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { NButton, NIcon, useMessage } from 'naive-ui'
import { LogOutOutline } from '@vicons/ionicons5'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { isTauri } from '../adapter'
import { flushSettings, store } from '../store'
import BrandIcon from '../components/BrandIcon.vue'
import packageInfo from '../../package.json'

const version = ref(packageInfo.version)
const message = useMessage()
const quitting = ref(false)
onMounted(async () => {
  if (isTauri) version.value = await getVersion()
})
async function quit() {
  if (!isTauri || quitting.value) return
  quitting.value = true
  try {
    await flushSettings()
    if (store.settingsError) throw new Error(store.settingsError)
    await invoke('quit_app')
  } catch (error) { message.error(String(error)) }
  finally { quitting.value = false }
}
</script>

<template>
  <section class="settings-section">
    <h2>关于</h2>
    <div class="about-row">
      <div class="brand-chip"><BrandIcon :size="20" /></div>
      <div class="about-name"><strong>Nexious Quick</strong><span>版本 {{ version }}</span></div>
      <NButton secondary type="error" size="small" :disabled="!isTauri" :loading="quitting" @click="quit">
        <template #icon><NIcon :component="LogOutOutline" /></template>退出应用
      </NButton>
    </div>
  </section>
</template>
