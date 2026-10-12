<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { NIcon, NInput, NSlider, NSwitch, useMessage } from 'naive-ui'
import {
  DesktopOutline,
  ImageOutline,
  MoonOutline,
  SettingsOutline,
  SunnyOutline,
} from '@vicons/ionicons5'
import { isDark, store } from '../store'
import {
  DEFAULT_SETTINGS,
  type ThemeMode,
} from '../types'
import { UI_THEMES, themeVariables } from '../themes'
import BrandIcon, { LAUNCHER_ICON_OPTIONS } from '../components/BrandIcon.vue'

const message = useMessage()
const themes: Array<{ key: ThemeMode; label: string; icon: unknown }> = [
  { key: 'system', label: '跟随系统', icon: DesktopOutline },
  { key: 'light', label: '浅色', icon: SunnyOutline },
  { key: 'dark', label: '深色', icon: MoonOutline },
]

function setTheme(mode: ThemeMode) {
  store.settings.theme = mode
}

function setAccent(key: string) {
  store.settings.accent = key
}

const customIcon = computed(() => store.settings.launcherIcon.startsWith('data:'))
const iconFileEl = ref<HTMLInputElement | null>(null)

// 预览可用宽度：跟随设置窗口大小自适应，两侧各留 12px 边距
const previewFitWidth = ref(300)
const previewWallEl = ref<HTMLElement | null>(null)
let previewResizeObserver: ResizeObserver | null = null
function measurePreviewWall() {
  const el = previewWallEl.value
  if (!el) return
  // 内层宽度需减去左右内边距与边框，保证等比后的整窗不被裁切
  previewFitWidth.value = Math.max(220, Math.round(el.clientWidth - 26))
}
onMounted(() => {
  measurePreviewWall()
  if (typeof ResizeObserver !== 'undefined' && previewWallEl.value) {
    previewResizeObserver = new ResizeObserver(measurePreviewWall)
    previewResizeObserver.observe(previewWallEl.value)
  }
})
onUnmounted(() => {
  previewResizeObserver?.disconnect()
  previewResizeObserver = null
})
// 等比缩放：整窗在预览墙内尽量放大显示（不引入额外的 1:1 模式）
const previewScale = computed(() => {
  const width = store.settings.searchWidth
  if (!width) return 1
  return Math.min(1, previewFitWidth.value / width)
})
const previewStageStyle = computed(() => ({
  width: `${Math.round(store.settings.searchWidth * previewScale.value)}px`,
  // 品牌图标高度固定 34px，输入框高度较小时搜索行高度仍以图标为准
  height: `${Math.round((Math.max(store.settings.searchHeight, 34) + 24) * previewScale.value)}px`,
}))
const previewRenderStyle = computed(() => ({
  width: `${store.settings.searchWidth}px`,
  '--panel-alpha': String(store.settings.opacity / 100),
  '--search-radius': `${store.settings.searchRadius}px`,
  '--search-height': `${store.settings.searchHeight}px`,
  transform: `scale(${previewScale.value})`,
}))

function pickBuiltinIcon(key: string) {
  store.settings.launcherIcon = key
}

function openIconPicker() {
  iconFileEl.value?.click()
}

function resetIcon() {
  store.settings.launcherIcon = DEFAULT_SETTINGS.launcherIcon
  message.success('已恢复闪电图标')
}

function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(new Error('读取图片失败'))
    reader.onload = () => {
      const img = new Image()
      img.onerror = () => reject(new Error('无法解析该图片'))
      img.onload = () => {
        const MAX = 128
        const scale = Math.min(1, MAX / Math.max(img.width, img.height))
        const width = Math.max(1, Math.round(img.width * scale))
        const height = Math.max(1, Math.round(img.height * scale))
        const canvas = document.createElement('canvas')
        canvas.width = width
        canvas.height = height
        const ctx = canvas.getContext('2d')
        if (!ctx) {
          reject(new Error('当前环境不支持图片处理'))
          return
        }
        ctx.clearRect(0, 0, width, height)
        ctx.drawImage(img, 0, 0, width, height)
        resolve(canvas.toDataURL('image/png'))
      }
      img.src = String(reader.result)
    }
    reader.readAsDataURL(file)
  })
}

async function onIconFileChange(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  if (!/^image\/(png|jpe?g|webp)$/i.test(file.type)) {
    message.error('仅支持 PNG / JPG / WebP 图片')
    return
  }
  if (file.size > 3 * 1024 * 1024) {
    message.error('图片过大，请选择 3MB 以内的图片')
    return
  }
  try {
    store.settings.launcherIcon = await fileToDataUrl(file)
    message.success('自定义图标已更新')
  } catch (error) {
    message.error(String(error))
  }
}

function onPlaceholderBlur() {
  if (!store.settings.placeholderText.trim()) {
    store.settings.placeholderText = DEFAULT_SETTINGS.placeholderText
    message.info('已恢复默认提示文字')
  }
}
</script>

<template>
  <div class="page appearance-page">
    <div class="page-header">
      <div><h1>外观设置</h1><p>让工作空间保持你喜欢的样子。</p></div>
      <span class="save-note" role="status">{{ store.settingsError ? '保存失败' : store.savingSettings ? '正在保存…' : '更改自动保存' }}</span>
    </div>

    <section class="appearance-styles" aria-labelledby="style-heading">
      <div class="appearance-section-head">
        <h2 id="style-heading">界面风格</h2>
        <div class="segmented" aria-label="主题模式">
          <button v-for="t in themes" :key="t.key" class="seg-item"
            :class="{ active: store.settings.theme === t.key }" :aria-pressed="store.settings.theme === t.key" @click="setTheme(t.key)">
            <NIcon :component="t.icon as never" :size="14" />{{ t.label }}
          </button>
        </div>
      </div>
      <div class="theme-styles">
        <button v-for="item in UI_THEMES" :key="item.key" type="button" class="style-item"
          :class="{ active: store.settings.accent === item.key }" :aria-pressed="store.settings.accent === item.key"
          :aria-label="item.label + '，' + item.description" @click="setAccent(item.key)">
          <span class="style-preview" :style="themeVariables(item, isDark)" aria-hidden="true">
            <span class="style-swatch-accent"></span><span class="style-swatch-neutral"></span><span class="style-swatch-surface"></span>
          </span>
          <span class="style-copy"><span class="style-label">{{ item.label }}</span><span class="style-description">{{ item.description }}</span></span>
          <span class="style-selected" aria-hidden="true">{{ store.settings.accent === item.key ? '✓' : '' }}</span>
        </button>
      </div>
    </section>

    <div class="launcher-preview-strip">
      <div class="preview-caption"><span>启动器预览</span><small>{{ store.settings.searchWidth }} × {{ store.settings.searchHeight }} px</small></div>
      <div ref="previewWallEl" class="preview-wall">
        <div class="preview-stage" :style="previewStageStyle">
          <div class="launcher preview-render" :style="previewRenderStyle" aria-hidden="true">
            <div class="search-row">
              <span v-if="store.settings.showIcons" class="brand-chip"><BrandIcon :size="18" /></span>
              <input class="search-input" type="text" readonly tabindex="-1" :placeholder="store.settings.placeholderText || DEFAULT_SETTINGS.placeholderText" />
              <span class="icon-btn"><NIcon :component="SettingsOutline" :size="17" /></span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <section class="appearance-controls" aria-labelledby="window-heading">
      <h2 id="window-heading">搜索窗口</h2>
      <div class="preference-row"><label id="opacity-label">透明度<small>调整启动器背景的通透程度</small></label>
        <div class="slider-row"><NSlider v-model:value="store.settings.opacity" aria-labelledby="opacity-label" :step="1" :min="30" :max="100" :tooltip="false" /><span class="slider-value">{{ store.settings.opacity }}%</span></div>
      </div>
      <div class="preference-row"><label id="width-label">宽度</label>
        <div class="slider-row"><NSlider v-model:value="store.settings.searchWidth" aria-labelledby="width-label" :step="5" :min="320" :max="720" :tooltip="false" /><span class="slider-value">{{ store.settings.searchWidth }} px</span></div>
      </div>
      <div class="preference-row"><label id="height-label">高度</label>
        <div class="slider-row"><NSlider v-model:value="store.settings.searchHeight" aria-labelledby="height-label" :step="2" :min="36" :max="64" :tooltip="false" /><span class="slider-value">{{ store.settings.searchHeight }} px</span></div>
      </div>
      <div class="preference-row"><label id="radius-label">圆角</label>
        <div class="slider-row"><NSlider v-model:value="store.settings.searchRadius" aria-labelledby="radius-label" :step="1" :min="0" :max="32" :tooltip="false" /><span class="slider-value">{{ store.settings.searchRadius }} px</span></div>
      </div>
    </section>

    <section class="appearance-controls" aria-labelledby="personal-heading">
      <h2 id="personal-heading">个性化</h2>
      <div class="preference-row"><label>显示启动器图标<small>搜索结果中的应用图标始终显示</small></label><NSwitch v-model:value="store.settings.showIcons" aria-label="显示启动器图标" size="small" /></div>
      <div class="preference-row icon-preference"><label>启动器图标<small>支持 PNG、JPG、WebP，最大 3 MB</small></label>
        <div class="icon-options">
          <button v-for="opt in LAUNCHER_ICON_OPTIONS" :key="opt.key" type="button" class="icon-option"
            :class="{ active: store.settings.launcherIcon === opt.key }" :aria-pressed="store.settings.launcherIcon === opt.key" @click="pickBuiltinIcon(opt.key)">
            <span class="brand-chip icon-option-chip"><img :src="opt.image" alt="" /></span><span>{{ opt.label }}</span>
          </button>
          <button type="button" class="icon-option" :class="{ active: customIcon }" :aria-pressed="customIcon" @click="openIconPicker">
            <span class="brand-chip icon-option-chip"><img v-if="customIcon" :src="store.settings.launcherIcon" alt="" /><NIcon v-else :component="ImageOutline" :size="16" /></span>
            <span>{{ customIcon ? '更换图片' : '上传图片' }}</span>
          </button>
          <button v-if="customIcon" type="button" class="ghost-btn small" @click="resetIcon">恢复默认</button>
          <input ref="iconFileEl" type="file" accept="image/png,image/jpeg,image/webp" hidden @change="onIconFileChange" />
        </div>
      </div>
      <div class="preference-row"><label for="launcher-placeholder">提示文字</label>
        <div class="placeholder-input-row"><NInput v-model:value="store.settings.placeholderText" :input-props="{ id: 'launcher-placeholder' }" :maxlength="24" clearable :placeholder="DEFAULT_SETTINGS.placeholderText" @blur="onPlaceholderBlur" />
          <button type="button" class="ghost-btn small" @click="store.settings.placeholderText = DEFAULT_SETTINGS.placeholderText">重置</button>
        </div>
      </div>
    </section>
  </div>
</template>
