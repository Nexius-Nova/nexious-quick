<script setup lang="ts">
import { NIcon } from 'naive-ui'
import { AppsOutline, DocumentOutline, FolderOutline, GlobeOutline } from '@vicons/ionicons5'
import { computed } from 'vue'
import type { ItemType } from '../types'

const props = withDefaults(defineProps<{ icon?: string; type?: ItemType }>(), { icon: '', type: 'application' })
const fallback = computed(() => ({ application: AppsOutline, website: GlobeOutline, folder: FolderOutline, file: DocumentOutline })[props.type])
</script>

<template>
  <span class="item-icon" :class="`item-icon-${type}`">
    <img v-if="icon.startsWith('data:image/')" :src="icon" alt="" />
    <span v-else-if="icon" class="emoji">{{ icon }}</span>
    <NIcon v-else :component="fallback" :size="20" />
  </span>
</template>
