import { defineComponent, onBeforeUnmount, Teleport, watch, type PropType } from 'vue'
import { X } from 'lucide-vue-next'
import { cn } from './cn'

export type DialogSize = 'sm' | 'md' | 'lg' | 'xl' | '2xl' | '3xl' | 'full'

const widths: Record<DialogSize, string> = {
  sm: 'max-w-sm',
  md: 'max-w-lg',
  lg: 'max-w-2xl',
  xl: 'max-w-4xl',
  '2xl': 'max-w-5xl',
  '3xl': 'max-w-6xl',
  full: 'max-w-[min(96vw,1400px)]',
}

let openCount = 0
const lockScroll = (on: boolean) => {
  openCount = Math.max(0, openCount + (on ? 1 : -1))
  document.body.style.overflow = openCount > 0 ? 'hidden' : ''
}

/** Modal dialog. `v-model` controls visibility. */
export const Dialog = defineComponent({
  name: 'ZsDialog',
  props: {
    modelValue: Boolean,
    title: String,
    description: String,
    size: { type: String as PropType<DialogSize>, default: 'md' },
    closeOnOverlay: { type: Boolean, default: true },
    hideClose: Boolean,
    bodyClass: String,
  },
  emits: { 'update:modelValue': (_v: boolean) => true, close: () => true },
  setup(props, { emit, slots }) {
    const close = () => {
      emit('update:modelValue', false)
      emit('close')
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && props.modelValue && !props.hideClose) close()
    }
    watch(
      () => props.modelValue,
      (open, prev) => {
        if (open === !!prev) return
        lockScroll(open)
        if (open) window.addEventListener('keydown', onKey)
        else window.removeEventListener('keydown', onKey)
      },
      { immediate: true },
    )
    onBeforeUnmount(() => {
      if (props.modelValue) lockScroll(false)
      window.removeEventListener('keydown', onKey)
    })
    return () =>
      props.modelValue ? (
        <Teleport to="body">
          <div class="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto p-3 sm:p-6">
            <div
              class="zs-fade-in fixed inset-0 bg-[var(--zs-overlay)] backdrop-blur-[3px]"
              onClick={() => props.closeOnOverlay && !props.hideClose && close()}
            />
            <div
              role="dialog"
              aria-modal="true"
              class={cn(
                'zs-pop-in relative my-auto flex max-h-[calc(100vh-2rem)] w-full flex-col overflow-hidden rounded-zs-lg border border-line bg-surface-solid shadow-zs',
                widths[props.size],
              )}
            >
              <div class="zs-gradient-bg absolute inset-x-0 top-0 h-1" />
              {(props.title || slots.title) && (
                <header class="flex items-start justify-between gap-4 border-b border-line px-6 pb-4 pt-5">
                  <div class="min-w-0">
                    <h2 class="zs-display flex items-center gap-2 text-lg text-fg">
                      <span class="zs-sparkle text-sm">✦</span>
                      {slots.title ? slots.title() : props.title}
                    </h2>
                    {props.description && <p class="mt-1 text-xs text-muted">{props.description}</p>}
                  </div>
                  {!props.hideClose && (
                    <button
                      type="button"
                      aria-label="close"
                      onClick={close}
                      class="rounded-full p-1.5 text-muted transition-colors hover:bg-primary-soft hover:text-primary"
                    >
                      <X class="h-4 w-4" />
                    </button>
                  )}
                </header>
              )}
              <div class={cn('min-h-0 flex-1 overflow-y-auto px-6 py-5', props.bodyClass)}>{slots.default?.()}</div>
              {slots.footer && (
                <footer class="flex flex-wrap items-center justify-end gap-2 border-t border-line bg-surface-muted/60 px-6 py-3.5">
                  {slots.footer()}
                </footer>
              )}
            </div>
          </div>
        </Teleport>
      ) : null
  },
})

export default Dialog
