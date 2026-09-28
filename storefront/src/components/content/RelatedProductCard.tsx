import { defineComponent, type PropType } from 'vue'
import { RouterLink } from 'vue-router'
import type { Product } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { hasPromotionPrice } from '@/utils/productPricing'
import { Price } from '@/components/common/Price'
import { SmartImage } from '@/components/ui'

/** Compact product tile for "related products" under a post. */
export const RelatedProductCard = defineComponent({
  name: 'RelatedProductCard',
  props: { product: { type: Object as PropType<Product>, required: true } },
  setup(props) {
    const { getLocalizedText } = useLocalized()
    return () => {
      const p = props.product
      const promo = hasPromotionPrice(p)
      return (
        <RouterLink to={`/products/${p.slug}`} class="zs-card zs-card-hover group flex items-center gap-4 p-3">
          <div class="size-20 shrink-0 overflow-hidden rounded-zs">
            <SmartImage src={p.images?.[0]} alt={getLocalizedText(p.title)} imgClass="transition-transform duration-500 group-hover:scale-110" />
          </div>
          <div class="min-w-0 flex-1">
            <div class="line-clamp-2 font-bold text-fg group-hover:text-primary-text">{getLocalizedText(p.title)}</div>
            <div class="mt-2 flex items-baseline gap-2">
              <Price amount={promo ? p.promotion_price_amount : p.price_amount} highlight />
              {promo && <Price amount={p.price_amount} size="sm" strike />}
            </div>
          </div>
        </RouterLink>
      )
    }
  },
})
