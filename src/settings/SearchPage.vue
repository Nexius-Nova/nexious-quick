<script setup lang="ts">
import { computed } from 'vue'
import { NSelect } from 'naive-ui'
import { store } from '../store'
import { SEARCH_ENGINES } from '../types'

const engineOptions = Object.keys(SEARCH_ENGINES).map((value) => ({ label: value, value }))

const model = computed({
  get: () => store.settings.searchEngine,
  set: (v: string) => (store.settings.searchEngine = v),
})
</script>

<template>
  <section class="settings-section">
    <h2>搜索</h2>

    <div class="field-group narrow">
      <div class="field-label">默认搜索引擎</div>
      <NSelect v-model:value="model" aria-label="默认搜索引擎" :options="engineOptions" style="width: 220px; max-width: 100%" />
    </div>
  </section>
</template>
