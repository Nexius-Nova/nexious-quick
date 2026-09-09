<script lang="ts">
export interface LauncherIconOption {
  key: string
  label: string
  image: string
}

export const APP_ICON_URL = '/nexious-icon.png'
export const DEFAULT_LAUNCHER_ICON = 'app'

/** 应用品牌图标（自定义图片仍可在外观设置中覆盖）。 */
export const LAUNCHER_ICON_OPTIONS: LauncherIconOption[] = [
  { key: DEFAULT_LAUNCHER_ICON, label: '应用图标', image: APP_ICON_URL },
]
</script>

<script setup lang="ts">
import { computed } from 'vue'
import { store } from '../store'

const props = withDefaults(defineProps<{ size?: number }>(), { size: 18 })

const customIcon = computed(() => store.settings.launcherIcon.startsWith('data:image/'))
</script>

<template>
  <img
    :src="customIcon ? store.settings.launcherIcon : APP_ICON_URL"
    alt="启动器图标"
    :width="props.size"
    :height="props.size"
  />
</template>
