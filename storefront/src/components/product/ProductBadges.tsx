import { defineComponent, type PropType } from 'vue'
import { Lock, Pencil, UserPlus, Zap } from 'lucide-vue-next'
import type { Product } from '@/api/types'
import { useProductLabels } from '@/composables/useProductLabels'
import { Badge } from '@/components/ui'

/** purchase type / fulfillment / stock badges row. */
export const ProductBadges = defineComponent({
  name: 'ProductBadges',
  props: {
    product: { type: Object as PropType<Product>, required: true },
    size: { type: String as PropType<'xs' | 'sm'>, default: 'xs' },
    showStock: { type: Boolean, default: true },
    compactMobile: Boolean,
  },
  setup(props) {
    const { getPurchaseTypeLabel, getFulfillmentTypeLabel, getStockBadgeVariant, getStockStatusLabel } = useProductLabels()
    return () => {
      const p = props.product
      const hide = props.compactMobile ? 'hidden md:inline-flex' : ''
      return (
        <div class="flex flex-wrap items-center gap-1.5">
          <span class={hide}>
            <Badge size={props.size} tone={p.purchase_type === 'guest' ? 'warning' : 'success'}>
              {p.purchase_type === 'guest' ? <UserPlus class="size-3" /> : <Lock class="size-3" />}
              {getPurchaseTypeLabel(p.purchase_type)}
            </Badge>
          </span>
          <Badge size={props.size} tone={p.fulfillment_type === 'auto' ? 'info' : 'neutral'}>
            {p.fulfillment_type === 'auto' ? <Zap class="size-3" /> : <Pencil class="size-3" />}
            {getFulfillmentTypeLabel(p.fulfillment_type)}
          </Badge>
          {props.showStock && (
            <span class={hide}>
              <Badge size={props.size} tone={getStockBadgeVariant(p.stock_status)}>
                {getStockStatusLabel(p)}
              </Badge>
            </span>
          )}
        </div>
      )
    }
  },
})
