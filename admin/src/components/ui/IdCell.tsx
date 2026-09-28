import { defineComponent, ref } from 'vue'
import { Check, Copy } from 'lucide-vue-next'
import { copyText } from '@/utils/clipboard'

/** `#123` with a copy button (also used for order numbers via `prefix=""`). */
export const IdCell = defineComponent({
  name: 'ZsIdCell',
  props: {
    value: { type: [String, Number], required: true },
    prefix: { type: String, default: '#' },
    mono: { type: Boolean, default: true },
  },
  setup(props) {
    const copied = ref(false)
    const copy = async (e: MouseEvent) => {
      e.stopPropagation()
      try {
        await copyText(String(props.value))
        copied.value = true
        window.setTimeout(() => (copied.value = false), 1200)
      } catch {
        /* ignore */
      }
    }
    return () => (
      <span class="inline-flex items-center gap-1.5">
        <span class={props.mono ? 'zs-num text-sm text-fg' : 'text-sm'}>
          {props.prefix}
          {props.value}
        </span>
        <button
          type="button"
          onClick={copy}
          class="rounded-md border border-line p-1 text-muted transition-colors hover:border-primary hover:text-primary"
          aria-label="copy"
        >
          {copied.value ? <Check class="h-3 w-3 text-success-text" /> : <Copy class="h-3 w-3" />}
        </button>
      </span>
    )
  },
})

export default IdCell
