<script setup lang="ts">
import { computed, onUnmounted, reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { NButton, NIcon, useMessage } from 'naive-ui'
import { KeyOutline, PencilOutline } from '@vicons/ionicons5'
import { isTauri } from '../adapter'
import { receiveSettings, store } from '../store'
import { comboFromEvent, prettyShortcut, RESERVED_SHORTCUTS } from '../types'

const message = useMessage()
const recording = ref(false)
const draft = ref('')
const saving = ref(false)

const display = computed(() => prettyShortcut(draft.value || store.settings.shortcut))

const state = reactive({ handler: null as ((e: KeyboardEvent) => void) | null })

function startRecord() {
  recording.value = true
  draft.value = ''
  state.handler = (e: KeyboardEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (e.key === 'Escape') {
      stopRecord()
      return
    }
    const combo = comboFromEvent(e)
    if (combo) {
      draft.value = combo
      stopRecord()
    }
  }
  window.addEventListener('keydown', state.handler, true)
}

function stopRecord() {
  if (state.handler) window.removeEventListener('keydown', state.handler, true)
  state.handler = null
  recording.value = false
}

async function save() {
  if (saving.value || recording.value) return
  const combo = (draft.value || store.settings.shortcut).trim()
  if (RESERVED_SHORTCUTS.includes(combo)) {
    message.warning(`「${prettyShortcut(combo)}」为系统保留快捷键，可能无法生效`)
    return
  }
  if (!/(Ctrl|Alt|Super)\+/.test(combo)) {
    message.warning('请使用包含 Ctrl、Alt 或 Win 的组合键')
    return
  }
  if (!isTauri) {
    store.settings.shortcut = combo
    draft.value = ''
    message.success('快捷键已保存，全局唤起需在桌面应用中使用')
    return
  }
  saving.value = true
  try {
    await invoke('set_shortcut', { combo })
    receiveSettings({ shortcut: combo })
    draft.value = ''
    message.success(`快捷键 ${prettyShortcut(combo)} 已保存，立即生效`)
  } catch (err) {
    message.error(String(err))
  } finally { saving.value = false }
}

onUnmounted(stopRecord)
</script>

<template>
  <section class="settings-section">
    <h2>快捷键</h2>

    <div class="field-group narrow">
      <div class="field-label">全局快捷键</div>
      <div class="shortcut-box">
        <div class="shortcut-display" :class="{ recording }">
          <NIcon :component="KeyOutline" :size="16" />
          <span class="combo">{{ recording ? '请按下组合键...' : display }}</span>
        </div>
        <button class="ghost-btn" :disabled="saving" @click="recording ? stopRecord() : startRecord()">
          <NIcon :component="PencilOutline" :size="14" />
          {{ recording ? '取消' : '修改' }}
        </button>
        <NButton type="primary" size="small" :loading="saving" :disabled="recording || !draft || draft === store.settings.shortcut" @click="save">保存</NButton>
        <NButton v-if="draft && !recording" size="small" :disabled="saving" @click="draft = ''">取消</NButton>
      </div>
    </div>
  </section>
</template>
