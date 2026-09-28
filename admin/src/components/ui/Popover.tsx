import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue'
import { cn } from './cn'

/** Minimal click-outside popover: `trigger` slot + panel `default` slot. */
export const Popover = defineComponent({
  name: 'ZsPopover',
  props: {
    open: Boolean,
    panelClass: String,
    align: { type: String as () => 'left' | 'right', default: 'left' },
  },
  emits: { 'update:open': (_v: boolean) => true },
  setup(props, { slots, emit }) {
    const root = ref<HTMLElement | null>(null)
    const onDoc = (e: MouseEvent) => {
      if (props.open && root.value && !root.value.contains(e.target as Node)) emit('update:open', false)
    }
    onMounted(() => document.addEventListener('mousedown', onDoc))
    onBeforeUnmount(() => document.removeEventListener('mousedown', onDoc))
    return () => (
      <div ref={root} class="relative">
        {slots.trigger?.()}
        {props.open && (
          <div
            class={cn(
              'zs-pop-in absolute z-40 mt-1.5 min-w-full rounded-zs border border-line bg-surface-solid p-1.5 shadow-zs',
              props.align === 'right' ? 'right-0' : 'left-0',
              props.panelClass,
            )}
          >
            {slots.default?.()}
          </div>
        )}
      </div>
    )
  },
})

export default Popover
