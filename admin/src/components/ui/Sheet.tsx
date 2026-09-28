import { defineComponent, onBeforeUnmount, Teleport, watch, type PropType } from 'vue'
import { X } from 'lucide-vue-next'
import { cn } from './cn'

/** Side drawer (mobile nav, detail panels). */
export const Sheet = defineComponent({
  name: 'ZsSheet',
  props: {
    modelValue: Boolean,
    side: { type: String as PropType<'left' | 'right'>, default: 'right' },
    title: String,
    width: { type: String, default: 'w-[min(92vw,640px)]' },
  },
  emits: { 'update:modelValue': (_v: boolean) => true },
  setup(props, { emit, slots }) {
    const close = () => emit('update:modelValue', false)
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && close()
    watch(
      () => props.modelValue,
      (open) => {
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
          <div class="fixed inset-0 z-50">
            <div class="zs-fade-in absolute inset-0 bg-[var(--zs-overlay)] backdrop-blur-[2px]" onClick={close} />
            <aside
              class={cn(
                'absolute inset-y-0 flex max-w-full flex-col border-line bg-surface-solid shadow-zs',
                props.side === 'left' ? 'zs-slide-left left-0 border-r' : 'zs-slide-right right-0 border-l',
                props.width,
              )}
            >
              {props.title !== undefined && (
                <header class="flex items-center justify-between gap-3 border-b border-line px-5 py-4">
                  <h2 class="zs-display text-lg text-fg">{props.title}</h2>
                  <button type="button" aria-label="close" onClick={close} class="rounded-full p-1.5 text-muted hover:bg-primary-soft hover:text-primary">
                    <X class="h-4 w-4" />
                  </button>
                </header>
              )}
              <div class="min-h-0 flex-1 overflow-y-auto">{slots.default?.()}</div>
              {slots.footer && <footer class="border-t border-line px-5 py-3">{slots.footer()}</footer>}
            </aside>
          </div>
        </Teleport>
      ) : null
  },
})

export default Sheet
