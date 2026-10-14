import { defineComponent, onBeforeUnmount, Teleport, watch, type PropType } from 'vue'
import { X } from 'lucide-vue-next'
import { cn } from './cn'

/** Side drawer for navigation on narrow screens. */
export const Sheet = defineComponent({
  name: 'ZsSheet',
  props: {
    modelValue: Boolean,
    title: { type: String, required: true },
    side: { type: String as PropType<'left' | 'right'>, default: 'left' },
  },
  emits: { 'update:modelValue': (_value: boolean) => true },
  setup(props, { emit, slots }) {
    const close = () => emit('update:modelValue', false)
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close()
    }
    watch(
      () => props.modelValue,
      (open) => {
        if (typeof document === 'undefined') return
        document.body.style.overflow = open ? 'hidden' : ''
        if (open) window.addEventListener('keydown', onKey)
        else window.removeEventListener('keydown', onKey)
      },
    )
    onBeforeUnmount(() => {
      window.removeEventListener('keydown', onKey)
      if (props.modelValue) document.body.style.overflow = ''
    })
    return () =>
      props.modelValue ? (
        <Teleport to="body">
          <div class="fixed inset-0 z-[70]">
            <div class="absolute inset-0 bg-[var(--zs-overlay)] backdrop-blur-[2px]" onClick={close} />
            <aside
              role="dialog"
              aria-modal="true"
              aria-label={props.title}
              class={cn(
                'absolute inset-y-0 flex w-[min(85vw,20rem)] max-w-full flex-col border-line bg-surface-solid shadow-zs-lg',
                props.side === 'left' ? 'left-0 border-r' : 'right-0 border-l',
              )}
            >
              <header class="flex items-center justify-between gap-3 border-b border-line px-5 py-4">
                <h2 class="zs-title text-lg text-fg">{props.title}</h2>
                <button type="button" aria-label="Close" class="rounded-full p-2 text-muted hover:bg-primary-soft hover:text-primary-text" onClick={close}>
                  <X class="size-5" />
                </button>
              </header>
              <div class="min-h-0 flex-1 overflow-y-auto px-3 py-4">{slots.default?.()}</div>
            </aside>
          </div>
        </Teleport>
      ) : null
  },
})
