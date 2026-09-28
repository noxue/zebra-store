import { defineComponent, onBeforeUnmount, Teleport, Transition, watch, type PropType } from 'vue'
import { X } from 'lucide-vue-next'
import { cn } from './cn'

/** Centered dialog (teleported to body). Closes on Esc/backdrop when `closable`. */
export const Modal = defineComponent({
  name: 'ZsModal',
  props: {
    open: Boolean,
    title: { type: String, default: '' },
    size: { type: String as PropType<'sm' | 'md' | 'lg' | 'xl'>, default: 'md' },
    closable: { type: Boolean, default: true },
    /** Render as a bottom sheet on small screens. */
    sheet: Boolean,
  },
  emits: { close: () => true },
  setup(props, { emit, slots }) {
    const widths = { sm: 'max-w-sm', md: 'max-w-lg', lg: 'max-w-2xl', xl: 'max-w-4xl' }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && props.closable) emit('close')
    }
    watch(
      () => props.open,
      (open) => {
        if (typeof document === 'undefined') return
        document.body.style.overflow = open ? 'hidden' : ''
        if (open) window.addEventListener('keydown', onKey)
        else window.removeEventListener('keydown', onKey)
      },
      { immediate: true },
    )
    onBeforeUnmount(() => {
      window.removeEventListener('keydown', onKey)
      if (props.open) document.body.style.overflow = ''
    })
    return () => (
      <Teleport to="body">
        <Transition name="zs-fade">
          {props.open && (
            <div
              class={cn('fixed inset-0 z-[70] flex justify-center bg-[var(--zs-overlay)] backdrop-blur-sm p-4', props.sheet ? 'items-end sm:items-center p-0 sm:p-4' : 'items-center')}
              onClick={(e: MouseEvent) => {
                if (e.target === e.currentTarget && props.closable) emit('close')
              }}
            >
              <div
                role="dialog"
                aria-modal="true"
                class={cn(
                  'zs-pop relative flex max-h-[90vh] w-full flex-col overflow-hidden border border-line bg-surface-solid shadow-zs-lg',
                  widths[props.size],
                  props.sheet ? 'rounded-t-[var(--zs-radius-lg)] sm:rounded-zs-lg' : 'rounded-zs-lg',
                )}
              >
                <div class="pointer-events-none absolute inset-x-0 top-0 h-1.5 zs-gradient-bg" />
                {(props.title || props.closable || slots.header) && (
                  <div class="flex items-center justify-between gap-3 px-6 pt-6 pb-2">
                    <div class="min-w-0 flex-1">
                      {slots.header ? slots.header() : <h3 class="zs-title truncate text-xl text-fg">{props.title}</h3>}
                    </div>
                    {props.closable && (
                      <button
                        type="button"
                        aria-label="close"
                        class="flex size-9 items-center justify-center rounded-full text-muted transition hover:bg-primary-soft hover:text-primary-text hover:rotate-90"
                        onClick={() => emit('close')}
                      >
                        <X class="size-5" />
                      </button>
                    )}
                  </div>
                )}
                <div class="flex-1 overflow-y-auto px-6 pb-6 pt-2">{slots.default?.()}</div>
                {slots.footer && <div class="flex flex-wrap justify-end gap-3 border-t border-line bg-surface-muted px-6 py-4">{slots.footer()}</div>}
              </div>
            </div>
          )}
        </Transition>
      </Teleport>
    )
  },
})
