import { defineComponent, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, Trash2 } from 'lucide-vue-next'
import type { AdminOrder } from '@/api/types'
import { Button, Dialog, FormField, IdCell, Input, Textarea } from '@/components/ui'
import { useOrderFulfillment } from '../useOrderFulfillment'

/** Manual fulfillment modal (note + delivery key/value entries). */
export default defineComponent({
  name: 'OrderFulfillmentModal',
  props: {
    modelValue: Boolean,
    order: { type: Object as PropType<AdminOrder | null>, default: null },
    siteCurrency: { type: String, default: '' },
    parentId: { type: Number as PropType<number | null>, default: null },
  },
  emits: {
    'update:modelValue': (_v: boolean) => true,
    success: (_parentId?: number | null) => true,
  },
  setup(props, { emit }) {
    const { t } = useI18n()
    const f = useOrderFulfillment(() => emit('success', props.parentId))

    watch(
      [() => props.modelValue, () => props.order?.id],
      ([open]) => {
        if (open && props.order) f.open(props.order)
        if (!open) f.close()
      },
      { immediate: true },
    )

    const submit = (e?: Event) => {
      e?.preventDefault()
      void f.submit()
    }

    return () => (
      <Dialog modelValue={props.modelValue} onUpdate:modelValue={(v) => emit('update:modelValue', v)} title={t('admin.orders.fulfillmentModalTitle')} size="lg">
        {{
          default: () =>
            f.loading.value ? (
              <div class="zs-skeleton h-24 rounded-zs" />
            ) : f.loadError.value ? (
              <div class="rounded-zs border border-danger/35 bg-danger-soft px-3 py-2 text-sm text-danger-text">{f.loadError.value}</div>
            ) : f.order.value ? (
              <div class="space-y-4">
                <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <div class="rounded-zs border border-line bg-surface-strong px-3 py-2.5">
                    <div class="text-xs text-muted">{t('admin.orders.table.id')}</div>
                    <div class="mt-1">
                      <IdCell value={f.order.value.id} />
                    </div>
                  </div>
                  <div class="rounded-zs border border-line bg-surface-strong px-3 py-2.5">
                    <div class="text-xs text-muted">{t('admin.orders.detailOrderNo')}</div>
                    <div class="mt-1 break-all font-mono text-sm text-fg">{f.order.value.order_no}</div>
                  </div>
                </div>
                <form class="space-y-4" onSubmit={submit}>
                  <FormField label={t('admin.orders.fulfillmentNote')}>
                    <Textarea v-model={f.form.note} rows={3} placeholder={t('admin.orders.fulfillmentNotePlaceholder')} />
                  </FormField>
                  <div class="space-y-3 rounded-zs border border-line bg-surface-muted/50 p-3">
                    <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                      <div class="text-xs font-medium text-fg/85">{t('admin.orders.fulfillmentDeliveryData')}</div>
                      <Button size="sm" onClick={f.addEntry}>
                        <Plus class="h-3.5 w-3.5" />
                        {t('admin.orders.fulfillmentAddDeliveryField')}
                      </Button>
                    </div>
                    <div class="space-y-2">
                      {f.form.entries.map((entry, i) => (
                        <div key={i} class="grid grid-cols-1 gap-2 md:grid-cols-[1fr_1fr_auto]">
                          <Input v-model={entry.key} placeholder={t('admin.orders.fulfillmentDeliveryKeyPlaceholder')} />
                          <Input v-model={entry.value} placeholder={t('admin.orders.fulfillmentDeliveryValuePlaceholder')} />
                          <Button size="sm" variant="danger" onClick={() => f.removeEntry(i)}>
                            <Trash2 class="h-3.5 w-3.5" />
                            {t('admin.common.delete')}
                          </Button>
                        </div>
                      ))}
                    </div>
                  </div>
                  {f.error.value && <div class="rounded-zs border border-danger/35 bg-danger-soft px-3 py-2 text-sm text-danger-text">{f.error.value}</div>}
                  {f.success.value && <div class="rounded-zs border border-success/40 bg-success-soft px-3 py-2 text-sm text-success-text">{f.success.value}</div>}
                </form>
              </div>
            ) : null,
          ...(f.order.value
            ? {
                footer: () => (
                  <>
                    <Button onClick={f.reset}>{t('admin.common.reset')}</Button>
                    <Button variant="primary" loading={f.submitting.value} onClick={() => submit()}>
                      {f.submitting.value ? t('admin.orders.fulfillmentSubmitting') : t('admin.orders.fulfillmentSubmit')}
                    </Button>
                  </>
                ),
              }
            : {}),
        }}
      </Dialog>
    )
  },
})
