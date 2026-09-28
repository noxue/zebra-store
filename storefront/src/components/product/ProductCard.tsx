import { computed, defineComponent, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowRight, ImageIcon, ShoppingCart } from 'lucide-vue-next'
import type { Product } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { Badge, cn } from '@/components/ui'
import { getFirstImageUrl, getImageUrl } from '@/utils/image'
import { hasPromotionPrice, hasWholesalePrices, isProductSoldOut } from '@/utils/productPricing'
import { ProductBadges } from './ProductBadges'

/** Grid card: 4:3 image, tags, category, title, badges, price, quick-buy. */
export const ProductCard = defineComponent({
  name: 'ProductCard',
  props: {
    product: { type: Object as PropType<Product>, required: true },
    index: { type: Number, default: 0 },
    maxTags: { type: Number, default: 2 },
  },
  emits: { click: (_slug: string) => true, quickBuy: (_p: Product) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const { getLocalizedText, formatPrice } = useLocalized()
    const attempt = ref(0)
    const candidates = computed(() => {
      const list: string[] = []
      const primary = getFirstImageUrl(props.product.images)
      if (primary) list.push(primary)
      const icon = props.product.category?.icon
      if (icon) {
        const resolved = getImageUrl(icon)
        if (resolved && resolved !== primary) list.push(resolved)
      }
      return list
    })
    watch(candidates, () => {
      attempt.value = 0
    })
    return () => {
      const p = props.product
      const soldOut = isProductSoldOut(p)
      const promo = hasPromotionPrice(p)
      const src = candidates.value[attempt.value]
      const title = getLocalizedText(p.title)
      return (
        <article
          class={cn(
            'zs-card group relative flex h-full cursor-pointer flex-col overflow-hidden p-0 transition-all duration-300',
            soldOut ? 'cursor-default opacity-85 saturate-50' : 'hover:-translate-y-1.5 hover:shadow-glow',
          )}
          style={{ animation: `zs-pop .4s ${props.index * 50}ms both` }}
          onClick={() => emit('click', p.slug)}
        >
          <div class="relative aspect-[4/3] shrink-0 overflow-hidden rounded-t-[var(--zs-radius-lg)] bg-surface-muted">
            {src ? (
              <img
                src={src}
                alt={title}
                loading="lazy"
                decoding="async"
                class={cn('size-full object-cover transition-transform duration-700', soldOut ? 'grayscale brightness-75' : 'group-hover:scale-105')}
                onError={() => {
                  attempt.value += 1
                }}
              />
            ) : (
              <div class="zs-soft-bg flex size-full items-center justify-center text-muted" role="img" aria-label={title}>
                <ImageIcon class="size-10 opacity-60" />
              </div>
            )}
            <div class="pointer-events-none absolute inset-x-0 bottom-0 h-1/3 bg-gradient-to-t from-black/25 to-transparent" />
            {soldOut && (
              <div class="absolute inset-0 flex items-center justify-center bg-black/35">
                <span class="rotate-[-8deg] rounded-full border-2 border-white/80 px-4 py-1 text-sm font-bold text-white zs-title">
                  {t('products.stockStatus.outOfStock')}
                </span>
              </div>
            )}
            {!soldOut && p.tags && p.tags.length > 0 && (
              <div class="absolute right-2 top-2 flex flex-wrap justify-end gap-1 md:right-3 md:top-3">
                {p.tags.slice(0, props.maxTags).map((tag) => (
                  <span key={tag} class="rounded-full bg-surface-strong/90 px-2.5 py-0.5 text-[11px] font-bold text-primary-text shadow-zs backdrop-blur">
                    ✦ {tag}
                  </span>
                ))}
              </div>
            )}
          </div>
          <div class="flex flex-1 flex-col p-3 md:p-4">
            {p.category?.name && (
              <div class="mb-1 truncate text-[11px] font-bold tracking-wider text-secondary-text">
                {t('products.categoryLabel')} · {getLocalizedText(p.category.name)}
              </div>
            )}
            <h3 class="mb-2 line-clamp-1 text-sm font-bold text-fg md:text-base">{title}</h3>
            <ProductBadges product={p} compactMobile />
            <p class="mt-2 hidden text-sm text-muted md:line-clamp-2">{getLocalizedText(p.description)}</p>
            <div class="mt-auto flex items-end justify-between gap-2 border-t border-dashed border-line pt-3">
              <div class="flex min-w-0 flex-col">
                <span class="hidden text-[11px] text-muted md:block">{t('products.price')}</span>
                <span
                  class={cn('zs-num text-base font-bold leading-tight md:text-lg', promo ? 'text-danger-text' : 'text-fg')}
                  aria-label={t(promo ? 'products.promotionPriceAria' : 'products.priceAria', {
                    price: formatPrice(promo ? p.promotion_price_amount : p.price_amount),
                  })}
                >
                  {formatPrice(promo ? p.promotion_price_amount : p.price_amount)}
                </span>
                {promo ? (
                  <div class="mt-0.5 flex flex-wrap items-center gap-1.5">
                    <span class="zs-num hidden text-xs text-muted line-through md:inline">{formatPrice(p.price_amount)}</span>
                    <Badge tone="danger" size="xs">
                      {t('products.promotionTag')}
                    </Badge>
                  </div>
                ) : hasWholesalePrices(p) ? (
                  <div class="mt-0.5">
                    <Badge tone="success" size="xs">
                      {t('products.wholesaleTag')}
                    </Badge>
                  </div>
                ) : (p.promotion_rules?.length ?? 0) > 0 ? (
                  <div class="mt-0.5">
                    <Badge tone="warning" size="xs">
                      {t('products.promotionBadge')}
                    </Badge>
                  </div>
                ) : null}
              </div>
              <div class="flex shrink-0 items-center gap-1.5">
                <button
                  type="button"
                  aria-label={t('products.quickBuyAria')}
                  disabled={soldOut}
                  class="flex size-9 items-center justify-center rounded-full bg-primary-soft text-primary-text transition hover:scale-110 hover:shadow-zs disabled:opacity-40"
                  onClick={(e: MouseEvent) => {
                    e.stopPropagation()
                    emit('quickBuy', p)
                  }}
                >
                  <ShoppingCart class="size-4" />
                </button>
                <ArrowRight class="hidden size-4 text-muted transition group-hover:translate-x-1 group-hover:text-primary-text md:block" />
              </div>
            </div>
          </div>
        </article>
      )
    }
  },
})
