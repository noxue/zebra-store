import { computed, defineComponent } from 'vue'
import { sanitizeHtml } from '@/utils/content'

/** Sanitised rich HTML (DOMPurify) with prose styles. */
export const RichContent = defineComponent({
  name: 'RichContent',
  props: { html: { type: String, default: '' } },
  setup(props) {
    const safe = computed(() => sanitizeHtml(props.html))
    return () => <div class="zs-prose" innerHTML={safe.value} />
  },
})
