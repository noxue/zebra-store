import { computed, defineComponent, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Crown, Layers, Tag } from 'lucide-vue-next'
import type { useProductPurchase } from '@/composables/useProductPurchase'
import { useLocalized } from '@/composables/useLocalized'
import { Badge, cn } from '@/components/ui'
import { normalizeSkuId } from '@/utils/sku'
import { buildSkuChoices, resolveSkuChoice, skuChoiceRows } from '@/utils/skuChoices'

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
      const pending = props.engine.requiresSkuSelection.value
      const tagLabel = d.tag === 'promotion' ? t('products.promotionTag') : d.tag === 'wholesale' ? t('products.wholesaleTag') : null
      return (
        <div class="zs-soft-bg relative overflow-hidden rounded-zs border border-line px-5 py-4">
          <span class="zs-sparkle pointer-events-none absolute right-4 top-3 text-lg">✦</span>
          <div class="flex flex-wrap items-center gap-2 text-xs font-bold text-muted">
            {t('products.price')}
            {tagLabel && !pending && (
              <Badge tone={d.tag === 'wholesale' ? 'success' : 'danger'} size="xs">
                {tagLabel}
              </Badge>
            )}
            {d.isMember && !pending && (
              <Badge tone="accent" size="xs">
                <Crown class="size-3" />
                {t('products.memberPriceTag')}
              </Badge>
            )}
          </div>
          <div class="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span class={cn('zs-num font-bold leading-none zs-gradient-text', pending ? 'break-words text-lg' : props.size === 'lg' ? 'text-4xl sm:text-5xl' : 'text-3xl')}>{pending ? t('skuChoices.priceAfterSelection') : formatPrice(d.price)}</span>
            {d.original && !pending && <span class="zs-num text-base text-muted line-through">{formatPrice(d.original)}</span>}
          </div>
          {props.engine.saveAmount.value && !pending && (
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
    const { t, locale } = useI18n()
    const { formatPrice } = useLocalized()
    const selections = ref<string[]>([])
    const choices = computed(() => buildSkuChoices(props.engine.activeSkus.value, locale.value))
    // A changed specification invalidates the customer's old choice, even if its
    // SKU ID survived. Price/stock and language changes do not choose another SKU.
    watch(() => JSON.stringify(props.engine.activeSkus.value.map((sku) => [sku.id, sku.spec_values])), () => {
      selections.value = []
      props.engine.syncSelectedSku(false)
    }, { immediate: true, flush: 'sync' })
    watch(() => props.engine.selectedSkuId.value, (id) => {
      selections.value = choices.value?.rows.find((row) => row.sku.id === id)?.values.slice() ?? []
    }, { flush: 'sync' })
    const choose = (index: number, value: string) => {
      const model = choices.value
      if (!model) return
      const prefix = [...selections.value.slice(0, index), value]
      props.engine.selectSku(0)
      selections.value = prefix
      const selected = resolveSkuChoice(model, prefix)
      if (selected && props.engine.skuPurchasable(selected)) props.engine.selectSku(selected.id)
    }
    return () => {
      const e = props.engine
      if (e.activeSkus.value.length === 0) return null
      const model = choices.value
      const shown = model ? e.activeSkus.value.filter((sku) => sku.id === e.selectedSkuId.value) : e.activeSkus.value
      return (
        <div>
          <div class="mb-2 text-sm font-bold text-fg">{t('productDetail.skuTitle')}</div>
          {model ? (
            <div class="mb-3 min-w-0 space-y-3">
              <p class="text-sm text-muted">{t('skuChoices.hint')}</p>
              {model.dimensions.map((dimension, index) => {
                const rows = skuChoiceRows(model, index, selections.value)
                const available = new Set(rows.map((row) => row.values[index]))
                return (
                  <fieldset key={dimension} class="min-w-0 space-y-2">
                    <legend class="max-w-full break-words text-sm font-bold text-fg">{dimension || t('productDetail.skuTitle')}</legend>
                    <div class="flex min-w-0 flex-wrap gap-2">
                      {model.values[index]?.filter((value) => available.has(value.key)).map((value) => {
                        const ok = rows.some((row) => row.values[index] === value.key && e.skuPurchasable(row.sku))
                        const selected = selections.value[index] === value.key
                        return <button type="button" key={value.key} disabled={!ok} aria-pressed={selected}
                          class={cn('min-h-11 max-w-full break-words rounded-zs border-2 px-3 py-2 text-left text-sm font-bold transition focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary', selected ? 'border-primary bg-primary-soft text-primary-text' : 'border-line bg-surface-strong text-fg', !ok && 'cursor-not-allowed opacity-50')}
                          onClick={() => choose(index, value.key)}>{value.label}{!ok && <span class="ml-1 text-xs">{t('productDetail.stockUnavailable')}</span>}</button>
                      })}
                      {rows.length === 0 && <p class="text-sm text-muted">{t('skuChoices.previous')}</p>}
                    </div>
                  </fieldset>
                )
              })}
              <p role="status" aria-live="polite" class="text-sm text-muted">{e.selectedSkuId.value > 0 ? t('skuChoices.selected') : t('productDetail.skuRequired')}</p>
            </div>
          ) : e.activeSkus.value.length > 1 && <p class="mb-3 text-sm text-muted">{t('skuChoices.explicit')}</p>}
          <div class="grid min-w-0 grid-cols-1 gap-2.5 sm:grid-cols-2">
            {shown.map((sku) => {
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
                    e.selectSku(id)
                  }}
                >
                  {selected && (
                    <span class="zs-gradient-bg absolute right-2 top-2 flex size-5 items-center justify-center rounded-full text-on-primary">
                      <Check class="size-3" stroke-width={3} />
                    </span>
                  )}
                  <span class="max-w-full break-words pr-6 text-sm font-bold text-fg">{e.skuDisplayText(sku)}</span>
                  {!model && e.activeSkus.value.length > 1 && <span class="max-w-full break-all text-xs text-muted">{t('skuChoices.code', { code: sku.sku_code || String(id) })}</span>}
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
