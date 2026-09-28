<script setup lang="ts">
import { onMounted, ref } from 'vue'

const props = defineProps<{ src: string; alt: string }>()
const missing = ref(false)
const img = ref<HTMLImageElement | null>(null)

onMounted(() => {
  // the error event may fire before hydration: check the loaded state as well
  const el = img.value
  if (el && el.complete && el.naturalWidth === 0) missing.value = true
})
</script>

<template>
  <span class="zs-shot" :class="{ 'is-missing': missing }">
    <img v-if="!missing" ref="img" :src="props.src" :alt="props.alt" loading="lazy" @error="missing = true" />
    <span v-else class="zs-shot__placeholder" role="img" :aria-label="props.alt">
      <svg viewBox="0 0 24 24" width="28" height="28" aria-hidden="true">
        <path fill="currentColor" d="M9 3 7.2 5H4a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-3.2L15 3H9Zm3 5a4.5 4.5 0 1 1 0 9 4.5 4.5 0 0 1 0-9Zm0 2a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z" />
      </svg>
      <span class="zs-shot__title">截图待补：{{ props.alt }}</span>
      <code class="zs-shot__path">{{ props.src }}</code>
    </span>
    <span class="zs-shot__caption">{{ props.alt }}</span>
  </span>
</template>
