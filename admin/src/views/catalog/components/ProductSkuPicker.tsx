import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import { Button, Card, Input, Select } from '@/components/ui'
import type { ProductSkuPicker } from '../useProductSkuPicker'

/**
 * Product (auto-fulfilment) + SKU selector card shared by the card-secret pages.
 * Slots: `extra` (extra control in the grid, e.g. refresh), `default` (extra info lines under the hint).
 */
export default defineComponent({
  name: 'ProductSkuPicker',
  props: {
    picker: { type: Object as PropType<ProductSkuPicker>, required: true },
    title: String,
    description: String,
    /** First info line (e.g. "未选择商品，默认展示全部卡密"). */
    hint: String,
  },
  emits: { productChange: () => true, skuChange: () => true },
  setup(props, { emit, slots }) {
    const { t } = useI18n()
    return () => {
      const p = props.picker
      const hasExtra = !!slots.extra
      return (
        <Card title={props.title} description={props.description}>
          <div class="grid grid-cols-1 gap-3 md:grid-cols-12">
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center md:col-span-4">
              <Input
                icon={Search}
                v-model={p.keyword.value}
                placeholder={t('admin.cardSecrets.productSearchPlaceholder')}
                onUpdate:modelValue={p.debouncedLoadOptions}
                onEnter={() => void p.loadOptions()}
              />
              <Button size="sm" class="shrink-0" disabled={p.optionsLoading.value} onClick={() => void p.loadOptions()}>
                {p.optionsLoading.value ? t('admin.common.loading') : t('admin.cardSecrets.searchProducts')}
              </Button>
            </div>
            <div class={hasExtra ? 'md:col-span-3' : 'md:col-span-4'}>
              <Select
                v-model={p.productValue.value}
                placeholder={t('admin.cardSecrets.productSelectPlaceholder')}
                options={p.productOptions.value}
                onChange={() => emit('productChange')}
              />
            </div>
            <div class={hasExtra ? 'md:col-span-3' : 'md:col-span-4'}>
              <Select
                v-model={p.skuValue.value}
                placeholder={t('admin.cardSecrets.skuPlaceholder')}
                options={p.skuOptions.value}
                disabled={p.skuDisabled.value}
                onChange={() => emit('skuChange')}
              />
            </div>
            {hasExtra && <div class="md:col-span-2">{slots.extra?.()}</div>}
          </div>
          <div class="mt-3 space-y-1 text-xs text-muted">
            {props.hint && <p>{props.hint}</p>}
            {p.productName.value && (
              <p>
                {t('admin.cardSecrets.productNameLabel')}：
                {p.productId.value ? (
                  <a href={p.productLink(p.productId.value)} target="_blank" rel="noopener" class="text-accent underline-offset-4 hover:underline">
                    {p.productName.value}
                  </a>
                ) : (
                  <span>{p.productName.value}</span>
                )}
              </p>
            )}
            {p.productId.value && (
              <p>
                {t('admin.cardSecrets.skuLabel')}：{p.currentSkuLabel.value}
              </p>
            )}
            {slots.default?.()}
          </div>
        </Card>
      )
    }
  },
})
