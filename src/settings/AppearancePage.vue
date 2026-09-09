<script setup lang="ts">
import { computed, ref } from 'vue'
import { NIcon, NInput, NSlider, NSwitch, useMessage } from 'naive-ui'
import { motion } from 'motion-v'
import {
  DesktopOutline,
  ImageOutline,
  MoonOutline,
  MoveOutline,
  SettingsOutline,
  SunnyOutline,
} from '@vicons/ionicons5'
import { isDark, store } from '../store'
import {
  ACCENT_THEMES,
  DEFAULT_SETTINGS,
  LAUNCHER_ANIMATION_OPTIONS,
  type LauncherAnimation,
  type ThemeMode,
} from '../types'
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

function setAnimationEffect(effect: LauncherAnimation) {
  store.settings.animationEffect = effect
}

const customIcon = computed(() => store.settings.launcherIcon.startsWith('data:'))
const iconFileEl = ref<HTMLInputElement | null>(null)

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
  <div class="page" :class="{ dark: isDark }">
    <div class="page-header">
      <div>
        <h1>外观设置</h1>
        <p>个性化快速启动的主题、透明度与搜索框显示效果。</p>
      </div>
    </div>

    <div class="appearance-layout">
      <div class="appearance-main">
        <div class="field-group">
          <div class="field-label">主题模式</div>
          <div class="segmented">
            <button
              v-for="t in themes"
              :key="t.key"
              class="seg-item"
              :class="{ active: store.settings.theme === t.key }"
              @click="setTheme(t.key)"
            >
              <NIcon :component="t.icon as never" :size="14" />
              {{ t.label }}
            </button>
          </div>
        </div>

        <div class="field-group">
          <div class="field-label">主题风格</div>
          <div class="theme-styles">
            <button
              v-for="item in ACCENT_THEMES"
              :key="item.key"
              type="button"
              class="style-item"
              :class="{ active: store.settings.accent === item.key }"
              :aria-pressed="store.settings.accent === item.key"
              @click="setAccent(item.key)"
            >
              <span class="style-dot" :style="{ background: item.color }">
                <svg v-if="store.settings.accent === item.key" class="style-check" viewBox="0 0 24 24" width="12" height="12" aria-hidden="true">
                  <path d="M20 6 9 17l-5-5" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </span>
              <span class="style-label">{{ item.label }}</span>
            </button>
          </div>
        </div>

        <div class="field-group">
          <div class="field-label">透明度</div>
          <div class="slider-row">
            <NSlider v-model:value="store.settings.opacity" :step="1" :min="30" :max="100" :tooltip="false" />
            <span class="slider-value">{{ store.settings.opacity }}%</span>
          </div>
        </div>

        <div class="field-group">
          <div class="field-label">显示效果</div>
          <div class="switch-list">
            <div class="switch-row">
              <span>启动时自动隐藏</span>
              <NSwitch v-model:value="store.settings.autoHide" size="small" />
            </div>
            <div class="switch-row">
              <span>鼠标悬停显示窗口</span>
              <NSwitch v-model:value="store.settings.hoverShow" size="small" />
            </div>
            <div class="switch-row">
              <span>显示图标</span>
              <NSwitch v-model:value="store.settings.showIcons" size="small" />
            </div>
          </div>
        </div>

        <div class="field-group narrow">
          <div class="field-label">搜索框圆角</div>
          <div class="slider-row">
            <NSlider v-model:value="store.settings.searchRadius" :step="1" :min="0" :max="32" :tooltip="false" />
            <span class="slider-value">{{ store.settings.searchRadius }}px</span>
          </div>
        </div>

        <div class="field-group">
          <div class="field-label">搜索结果动画</div>
          <div class="animation-options" role="group" aria-label="搜索结果动画">
            <motion.button
              v-for="option in LAUNCHER_ANIMATION_OPTIONS"
              :key="option.key"
              type="button"
              class="animation-option"
              :class="{ active: store.settings.animationEffect === option.key }"
              :aria-pressed="store.settings.animationEffect === option.key"
              :whileHover="{ y: -2 }"
              :whilePress="{ scale: 0.98 }"
              @click="setAnimationEffect(option.key)"
            >
              <span class="animation-option-label">{{ option.label }}</span>
              <span class="animation-option-description">{{ option.description }}</span>
            </motion.button>
          </div>
          <p class="muted-tip">动画仅作用于主窗口的搜索结果列表，清空输入时会立即收起。</p>
        </div>

        <div class="field-group">
          <div class="field-label">启动器图标</div>
          <div class="icon-options">
            <motion.button
              v-for="opt in LAUNCHER_ICON_OPTIONS"
              :key="opt.key"
              type="button"
              class="icon-option"
              :class="{ active: store.settings.launcherIcon === opt.key }"
              :aria-pressed="store.settings.launcherIcon === opt.key"
              :whileHover="{ y: -2 }"
              :whilePress="{ scale: 0.97 }"
              @click="pickBuiltinIcon(opt.key)"
            >
              <span class="brand-chip icon-option-chip"><img :src="opt.image" alt="" /></span>
              <span>{{ opt.label }}</span>
            </motion.button>
            <motion.button
              type="button"
              class="icon-option"
              :class="{ active: customIcon }"
              :aria-pressed="customIcon"
              title="上传自定义图标"
              :whileHover="{ y: -2 }"
              :whilePress="{ scale: 0.97 }"
              @click="openIconPicker"
            >
              <span class="brand-chip icon-option-chip">
                <img v-if="customIcon" :src="store.settings.launcherIcon" alt="" />
                <NIcon v-else :component="ImageOutline" :size="16" />
              </span>
              <span>{{ customIcon ? '自定义' : '自定义…' }}</span>
            </motion.button>
          </div>
          <div v-if="customIcon" class="icon-custom-row">
            <span class="muted-text">已使用自定义图片，上传后自动缩放为 128×128。</span>
            <button type="button" class="ghost-btn small" @click="resetIcon">恢复内置图标</button>
          </div>
          <p class="muted-tip">支持 PNG / JPG / WebP，选择后自动缩放，启动器、设置页与关于页将同步显示。</p>
          <input ref="iconFileEl" type="file" accept="image/png,image/jpeg,image/webp" hidden @change="onIconFileChange" />
        </div>

        <div class="field-group narrow">
          <div class="field-label">搜索框提示文字</div>
          <div class="placeholder-input-row">
            <NInput
              v-model:value="store.settings.placeholderText"
              :maxlength="24"
              clearable
              placeholder="输入内容，快速启动..."
              @blur="onPlaceholderBlur"
            />
            <button type="button" class="ghost-btn small" @click="onPlaceholderBlur">恢复默认</button>
          </div>
          <p class="muted-tip">显示在启动器搜索框中的灰色提示文字，留空会自动恢复默认。</p>
        </div>
      </div>

      <div class="appearance-preview">
        <div class="preview-wall">
          <div
            class="preview-launcher"
            :class="{ dark: isDark }"
            :style="{
              '--panel-alpha': String(store.settings.opacity / 100),
              '--search-radius': `${store.settings.searchRadius}px`,
            }"
          >
            <div v-if="store.settings.showIcons" class="brand-chip small">
              <BrandIcon :size="13" />
            </div>
            <span v-else class="preview-drag-icon" aria-hidden="true">
              <NIcon :component="MoveOutline" :size="17" />
            </span>
            <span class="preview-text">{{ store.settings.placeholderText || '输入内容，快速启动...' }}</span>
            <span class="preview-gear" aria-hidden="true">
              <NIcon :component="SettingsOutline" :size="15" />
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
