import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, ShoppingCart } from 'lucide-vue-next'
import type { Product } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { Badge, SmartImage, cn } from '@/components/ui'
import { hasPromotionPrice, hasWholesalePrices, isProductSoldOut } from '@/utils/productPricing'
import { ProductBadges } from './ProductBadges'

/** Compact row version of ProductCard (list template mode). */
export const ProductListItem = defineComponent({
  name: 'ProductListItem',
  props: {
    product: { type: Object as PropType<Product>, required: true },
    index: { type: Number, default: 0 },
  },
  emits: { click: (_slug: string) => true, quickBuy: (_p: Product) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const { getLocalizedText, formatPrice } = useLocalized()
    return () => {
      const p = props.product
      const soldOut = isProductSoldOut(p)
      const promo = hasPromotionPrice(p)
      return (
        <div
          class={cn(
            'group flex cursor-pointer items-center gap-3 rounded-zs border border-line bg-surface-strong p-3 transition hover:border-line-strong hover:shadow-zs',
            soldOut && 'opacity-70 saturate-50',
          )}
          style={{ animation: `zs-pop .35s ${props.index * 30}ms both` }}
          onClick={() => emit('click', p.slug)}
        >
          <div class="relative size-14 shrink-0 overflow-hidden rounded-zs-sm md:size-16">
            <SmartImage src={p.images?.[0]} alt={getLocalizedText(p.title)} />
          </div>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <h4 class="truncate text-sm font-bold text-fg md:text-base">{getLocalizedText(p.title)}</h4>
              {soldOut && (
                <Badge tone="danger" size="xs">
                  {t('products.stockStatus.outOfStock')}
                </Badge>
              )}
            </div>
            <div class="mt-1">
              <ProductBadges product={p} compactMobile />
            </div>
          </div>
          <div class="flex shrink-0 flex-col items-end">
            <span class={cn('zs-num text-base font-bold', promo ? 'text-danger-text' : 'text-fg')}>
              {formatPrice(promo ? p.promotion_price_amount : p.price_amount)}
            </span>
            {promo ? (
              <span class="zs-num text-xs text-muted line-through">{formatPrice(p.price_amount)}</span>
            ) : hasWholesalePrices(p) ? (
              <Badge tone="success" size="xs">
                {t('products.wholesaleTag')}
              </Badge>
            ) : null}
          </div>
          <button
            type="button"
            aria-label={t('products.quickBuyAria')}
            disabled={soldOut}
            class="flex size-9 shrink-0 items-center justify-center rounded-full bg-primary-soft text-primary-text transition hover:scale-110 disabled:opacity-40"
            onClick={(e: MouseEvent) => {
              e.stopPropagation()
              emit('quickBuy', p)
            }}
          >
            <ShoppingCart class="size-4" />
          </button>
          <ChevronRight class="hidden size-4 text-muted md:block" />
        </div>
      )
    }
  },
})
