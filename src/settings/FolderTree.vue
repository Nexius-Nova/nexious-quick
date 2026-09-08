<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { NIcon } from 'naive-ui'
import {
  AddCircleOutline,
  CheckmarkCircleOutline,
  ChevronDownOutline,
  ChevronForwardOutline,
  FolderOpenOutline,
  FolderOutline,
} from '@vicons/ionicons5'
import { isTauri } from '../adapter'

interface TreeNode {
  name: string
  path: string
  is_dir: boolean
  depth: number
  expanded: boolean
  loading: boolean
  children: TreeNode[] | null
}

const props = withDefaults(
  defineProps<{
    node?: TreeNode
    addedPaths?: string[]
  }>(),
  { addedPaths: () => [] },
)
const emit = defineEmits<{ (e: 'add', path: string): void }>()

const drives = ref<TreeNode[]>([])
const errorText = ref('')

function isAdded(path: string): boolean {
  return props.addedPaths.some((p) => p.toLowerCase() === path.toLowerCase())
}

async function loadChildren(node: TreeNode) {
  if (node.loading) return
  node.loading = true
  node.children = []
  try {
    const entries = await invoke<Array<{ name: string; path: string; is_dir: boolean }>>('list_directory', {
      path: node.path,
    })
    node.children = entries.map((e) => ({
      name: e.name,
      path: e.path,
      is_dir: e.is_dir,
      depth: node.depth + 1,
      expanded: false,
      loading: false,
      children: null,
    }))
    errorText.value = ''
  } catch (err) {
    errorText.value = String(err)
    node.children = null
  } finally {
    node.loading = false
  }
}

function toggle(node: TreeNode) {
  if (!node.expanded && node.children == null) void loadChildren(node)
  if (!node.loading) node.expanded = !node.expanded
}

onMounted(async () => {
  if (!isTauri) return
  try {
    const entries = await invoke<Array<{ name: string; path: string; is_dir: boolean }>>('list_drives')
    drives.value = entries.map((e) => ({
      name: e.name,
      path: e.path,
      is_dir: e.is_dir,
      depth: 0,
      expanded: false,
      loading: false,
      children: null,
    }))
  } catch (err) {
    errorText.value = String(err)
  }
})
</script>

<template>
  <div class="ftree">
    <div v-if="!isTauri" class="ftree-empty">浏览器预览模式不支持浏览本机目录</div>
    <div v-else-if="errorText && !node && !drives.length" class="ftree-empty">{{ errorText }}</div>

    <template v-if="!node">
      <div v-if="drives.length" class="ftree-caption">此电脑</div>
      <FolderTree
        v-for="drive in drives"
        :key="drive.path"
        :node="drive"
        :added-paths="addedPaths"
        @add="(p) => emit('add', p)"
      />
      <div v-if="!drives.length" class="ftree-empty">未检测到磁盘目录</div>
    </template>

    <div v-else class="ftree-block">
      <div
        class="ftree-row"
        :class="{ added: isAdded(node.path) }"
        :style="{ paddingLeft: node.depth * 14 + 'px' }"
      >
        <button
          type="button"
          class="ftree-toggle"
          :disabled="node.loading || !node.is_dir"
          @click="toggle(node)"
        >
          <span v-if="node.loading" class="ftree-spinner" />
          <NIcon v-else-if="node.expanded" :component="ChevronDownOutline" :size="13" />
          <NIcon v-else :component="ChevronForwardOutline" :size="13" />
        </button>
        <span class="ftree-icon">
          <NIcon
            :component="node.expanded ? FolderOpenOutline : FolderOutline"
            :size="16"
          />
        </span>
        <span class="ftree-name" :title="node.path">{{ node.name }}</span>
        <button
          v-if="!isAdded(node.path)"
          type="button"
          class="ftree-add"
          title="将目录加入同步"
          @click="emit('add', node.path)"
        >
          <NIcon :component="AddCircleOutline" :size="16" />
        </button>
        <span v-else class="ftree-check" title="已在同步目录中">
          <NIcon :component="CheckmarkCircleOutline" :size="15" />
        </span>
      </div>

      <template v-if="node.expanded">
        <div v-if="node.loading" class="ftree-empty ftree-leaf-empty">加载中…</div>
        <div v-else-if="node.children && !node.children.length" class="ftree-empty ftree-leaf-empty">（空）</div>
        <FolderTree
          v-for="child in node.children ?? []"
          :key="child.path"
          :node="child"
          :added-paths="addedPaths"
          @add="(p) => emit('add', p)"
        />
      </template>
    </div>
  </div>
</template>
