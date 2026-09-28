import { defineComponent, onBeforeUnmount, onMounted, ref, Teleport, Transition, TransitionGroup } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowUp, CheckCircle2, Info, X, XCircle } from 'lucide-vue-next'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { useToast } from '@/composables/useToast'
import { Button, Modal, PetalLoader, cn } from '@/components/ui'

export const BackToTop = defineComponent({
  name: 'BackToTop',
  setup() {
    const { t } = useI18n()
    const visible = ref(false)
    const onScroll = () => {
      visible.value = window.scrollY > 480
    }
    onMounted(() => window.addEventListener('scroll', onScroll, { passive: true }))
    onBeforeUnmount(() => window.removeEventListener('scroll', onScroll))
    return () => (
      <Transition name="zs-fade">
        {visible.value && (
          <button
            type="button"
            aria-label={t('common.backToTop')}
            class="zs-gradient-bg fixed bottom-24 right-4 z-30 flex size-11 items-center justify-center rounded-full text-on-primary shadow-zs-lg transition hover:-translate-y-1 lg:bottom-8 lg:right-8"
            onClick={() => window.scrollTo({ top: 0, behavior: 'smooth' })}
          >
            <ArrowUp class="size-5" />
          </button>
        )}
      </Transition>
    )
  },
})

export const ToastHost = defineComponent({
  name: 'ToastHost',
  setup() {
    const { toasts, removeToast } = useToast()
    const icons = { success: CheckCircle2, error: XCircle, info: Info }
    const tones = { success: 'text-success-text', error: 'text-danger-text', info: 'text-accent-text' }
    return () => (
      <Teleport to="body">
        <div class="pointer-events-none fixed inset-x-0 top-20 z-[80] flex flex-col items-center gap-2 px-4">
          <TransitionGroup name="zs-fade">
            {toasts.value.map((item) => {
              const Icon = icons[item.type]
              return (
                <div key={item.id} role="status" class="zs-pop pointer-events-auto flex max-w-md items-center gap-3 rounded-full border border-line bg-surface-solid py-2.5 pl-4 pr-2 shadow-zs-lg">
                  <Icon class={cn('size-5 shrink-0', tones[item.type])} />
                  <span class="text-sm font-bold text-fg">{item.message}</span>
                  {item.action && (
                    <button
                      type="button"
                      class="rounded-full bg-primary-soft px-3 py-1 text-xs font-bold text-primary-text"
                      onClick={() => {
                        item.action?.onClick()
                        removeToast(item.id)
                      }}
                    >
                      {item.action.label}
                    </button>
                  )}
                  <button type="button" aria-label="close" class="flex size-7 items-center justify-center rounded-full text-muted hover:bg-surface-muted" onClick={() => removeToast(item.id)}>
                    <X class="size-4" />
                  </button>
                </div>
              )
            })}
          </TransitionGroup>
        </div>
      </Teleport>
    )
  },
})

export const ConfirmHost = defineComponent({
  name: 'ConfirmHost',
  setup() {
    const { t } = useI18n()
    const { visible, options, handleCancel, handleConfirm } = useConfirmDialog()
    return () => (
      <Modal open={visible.value} title={options.value.title} size="sm" onClose={handleCancel}>
        {{
          default: () => <p class="text-sm leading-relaxed text-muted">{options.value.message}</p>,
          footer: () => (
            <>
              <Button variant="secondary" onClick={handleCancel}>
                {options.value.cancelText || t('common.cancel')}
              </Button>
              <Button variant={options.value.variant === 'danger' ? 'danger' : 'primary'} onClick={handleConfirm}>
                {options.value.confirmText || t('common.confirm')}
              </Button>
            </>
          ),
        }}
      </Modal>
    )
  },
})

export const LoadingOverlay = defineComponent({
  name: 'LoadingOverlay',
  props: { loading: Boolean },
  setup(props) {
    const { t } = useI18n()
    return () => (
      <Transition name="zs-fade">
        {props.loading && (
          <div class="fixed inset-0 z-[90] flex items-center justify-center bg-bg/70 backdrop-blur-sm">
            <PetalLoader label={t('zs.loadingText')} />
          </div>
        )}
      </Transition>
    )
  },
})
