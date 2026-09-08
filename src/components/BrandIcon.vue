<script lang="ts">
import { CompassOutline, CubeOutline, FlashOutline, RocketOutline, SparklesOutline } from '@vicons/ionicons5'

export interface LauncherIconOption {
  key: string
  label: string
  component: import('vue').Component
}

/** 可选的启动器内置图标（用于外观设置与统一渲染）。 */
export const LAUNCHER_ICON_OPTIONS: LauncherIconOption[] = [
  { key: 'bolt', label: '闪电', component: FlashOutline },
  { key: 'rocket', label: '火箭', component: RocketOutline },
  { key: 'sparkles', label: '星光', component: SparklesOutline },
  { key: 'compass', label: '罗盘', component: CompassOutline },
  { key: 'cube', label: '魔方', component: CubeOutline },
]
</script>

<script setup lang="ts">
import { computed } from 'vue'
import { NIcon } from 'naive-ui'
import { store } from '../store'

const props = withDefaults(defineProps<{ size?: number }>(), { size: 18 })

const current = computed(
  () => LAUNCHER_ICON_OPTIONS.find((o) => o.key === store.settings.launcherIcon) ?? LAUNCHER_ICON_OPTIONS[0],
)
const isImage = computed(() => store.settings.launcherIcon.startsWith('data:'))
</script>

<template>
  <img v-if="isImage" :src="store.settings.launcherIcon" alt="启动器图标" />
  <NIcon v-else :component="current.component" :size="props.size" />
</template>
