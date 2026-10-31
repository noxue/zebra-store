import { defineComponent, toRef, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { LoaderCircle, RefreshCw } from 'lucide-vue-next'
import type { Order } from '@/api/types'
import { Button, Modal } from '@/components/ui'
import { useOrderProcessing } from '@/composables/useOrderProcessing'

/** Shared member/guest delivery wait screen; existing detail polling stays active. */
export const OrderProcessingOverlay = defineComponent({
  name: 'OrderProcessingOverlay',
  props: { order: { type: Object as PropType<Order | null>, default: null } },
  emits: { refresh: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const wait = useOrderProcessing(toRef(props, 'order'))
    return () => (
      <Modal open={wait.open.value} closable={false} size="sm">
        <div class="space-y-5 pt-6 text-center" aria-live="polite" aria-atomic="true">
          <div class="mx-auto flex size-20 items-center justify-center rounded-full bg-primary-soft text-primary-text">
            <LoaderCircle class="size-11 zs-spin" aria-hidden="true" />
          </div>
          <div class="space-y-2">
            <h2 class="zs-title text-xl text-fg">{t('orderProcessing.title')}</h2>
            <p class="text-sm leading-relaxed text-muted">{t('orderProcessing.hint')}</p>
          </div>
          {wait.longWait.value && (
            <div class="space-y-4 rounded-zs border border-line bg-surface-muted p-4">
              <p class="text-sm leading-relaxed text-fg">{t('orderProcessing.longWait')}</p>
              <Button block onClick={() => emit('refresh')}>
                <RefreshCw class="size-4" aria-hidden="true" />
                {t('orderProcessing.refresh')}
              </Button>
              <Button block variant="secondary" onClick={wait.continueWaiting}>{t('orderProcessing.continue')}</Button>
            </div>
          )}
          <Button variant="ghost" onClick={wait.dismiss}>{t('orderProcessing.viewOrder')}</Button>
        </div>
      </Modal>
    )
  },
})
