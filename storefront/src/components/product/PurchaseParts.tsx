import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Crown, Layers, Tag } from 'lucide-vue-next'
import type { useProductPurchase } from '@/composables/useProductPurchase'
import { useLocalized } from '@/composables/useLocalized'
import { Badge, cn } from '@/components/ui'
import { normalizeSkuId } from '@/utils/sku'

export type PurchaseEngine = ReturnType<typeof useProductPurchase>

/** Final price, struck original, tag and "save X". */
export const PriceBlock = defineComponent({
  name: 'PriceBlock',
  props: {
    engine: { type: Object as PropType<PurchaseEngine>, required: true },
    size: { type: String as PropType<'md' | 'lg'>, default: 'lg' },
  },
  setup(props) {
    const { t } = useI18n()
    const { formatPrice } = useLocalized()
    return () => {
      const d = props.engine.priceDisplay.value
      const tagLabel = d.tag === 'promotion' ? t('products.promotionTag') : d.tag === 'wholesale' ? t('products.wholesaleTag') : null
      return (
        <div class="zs-soft-bg relative overflow-hidden rounded-zs border border-line px-5 py-4">
          <span class="zs-sparkle pointer-events-none absolute right-4 top-3 text-lg">✦</span>
          <div class="flex flex-wrap items-center gap-2 text-xs font-bold text-muted">
            {t('products.price')}
            {tagLabel && (
              <Badge tone={d.tag === 'wholesale' ? 'success' : 'danger'} size="xs">
                {tagLabel}
              </Badge>
            )}
            {d.isMember && (
              <Badge tone="accent" size="xs">
                <Crown class="size-3" />
                {t('products.memberPriceTag')}
              </Badge>
            )}
          </div>
          <div class="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span class={cn('zs-num font-bold leading-none zs-gradient-text', props.size === 'lg' ? 'text-4xl sm:text-5xl' : 'text-3xl')}>{formatPrice(d.price)}</span>
            {d.original && <span class="zs-num text-base text-muted line-through">{formatPrice(d.original)}</span>}
          </div>
          {props.engine.saveAmount.value && (
            <div class="mt-2 text-sm font-bold text-danger-text">
              {t('products.saveAmount')} <span class="zs-num">{props.engine.saveAmount.value}</span>
            </div>
          )}
        </div>
      )
    }
  },
})

/** Promotion + wholesale rule panels. */
export const RulePanels = defineComponent({
  name: 'RulePanels',
  props: { engine: { type: Object as PropType<PurchaseEngine>, required: true } },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const e = props.engine
      const promos = e.promotionRules.value
      const tiers = e.wholesaleRules.value
      if (promos.length === 0 && tiers.length === 0) return null
      return (
        <div class="grid gap-3">
          {promos.length > 0 && (
            <div class="rounded-zs border border-warning/40 bg-warning-soft px-4 py-3">
              <div class="flex items-center gap-2 text-sm font-bold text-warning-text">
                <Tag class="size-4" />
                {t('products.promotionRulesTitle')}
              </div>
              <ul class="mt-1.5 space-y-1 text-sm text-warning-text">
                {promos.map((rule, i) => (
                  <li key={rule.id ?? i} class="flex items-center gap-2">
                    <span class="size-1.5 rounded-full bg-warning" />
                    {e.formatPromotionRule(rule)}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {tiers.length > 0 && (
            <div class="rounded-zs border border-success/40 bg-success-soft px-4 py-3">
              <div class="flex items-center gap-2 text-sm font-bold text-success-text">
                <Layers class="size-4" />
                {t('products.wholesaleRulesTitle')}
              </div>
              <ul class="mt-1.5 space-y-1 text-sm text-success-text">
                {tiers.map((tier, i) => (
                  <li key={i} class="flex items-center gap-2">
                    <span class="size-1.5 rounded-full bg-success" />
                    {e.formatWholesaleTier(tier)}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )
    }
  },
})

/** SKU option cards with stock badge; sold-out SKUs disabled. */
export const SkuSelector = defineComponent({
  name: 'SkuSelector',
  props: {
    engine: { type: Object as PropType<PurchaseEngine>, required: true },
    compact: Boolean,
  },
  setup(props) {
    const { t } = useI18n()
    const { formatPrice } = useLocalized()
    return () => {
      const e = props.engine
      if (e.activeSkus.value.length === 0) return null
      return (
        <div>
          <div class="mb-2 text-sm font-bold text-fg">{t('productDetail.skuTitle')}</div>
          <div class={cn('grid gap-2.5', props.compact ? 'grid-cols-1 sm:grid-cols-2' : 'grid-cols-1 sm:grid-cols-2')}>
            {e.activeSkus.value.map((sku) => {
              const id = normalizeSkuId(sku.id)
              const selected = e.selectedSkuId.value === id
              const ok = e.skuPurchasable(sku)
              return (
                <button
                  key={id}
                  type="button"
                  disabled={!ok}
                  aria-pressed={selected}
                  class={cn(
                    'relative flex flex-col items-start gap-1.5 rounded-zs border-2 px-3.5 py-2.5 text-left transition-all',
                    selected ? 'border-primary bg-primary-soft shadow-zs' : 'border-line bg-surface-strong hover:border-line-strong',
                    !ok && 'cursor-not-allowed opacity-50',
                  )}
                  onClick={() => {
                    e.selectedSkuId.value = id
                  }}
                >
                  {selected && (
                    <span class="zs-gradient-bg absolute right-2 top-2 flex size-5 items-center justify-center rounded-full text-on-primary">
                      <Check class="size-3" stroke-width={3} />
                    </span>
                  )}
                  <span class="pr-6 text-sm font-bold text-fg">{e.skuDisplayText(sku)}</span>
                  <span class="flex flex-wrap items-center gap-1.5">
                    <span class="zs-num text-xs font-bold text-primary-text">{formatPrice(sku.promotion_price_amount || sku.price_amount)}</span>
                    <Badge tone={e.skuStockTone(sku)} size="xs">
                      {e.skuStockText(sku)}
                    </Badge>
                  </span>
                </button>
              )
            })}
          </div>
        </div>
      )
    }
  },
})
