import { defineComponent, type PropType } from 'vue'
import type { Product } from '@/api/types'
import { cn } from '@/components/ui'
import { ProductCard } from './ProductCard'

/** Responsive ProductCard grid with skeleton state. */
export const ProductGrid = defineComponent({
  name: 'ProductGrid',
  props: {
    products: { type: Array as PropType<Product[]>, default: () => [] },
    loading: Boolean,
    skeletonCount: { type: Number, default: 8 },
    cols: { type: String as PropType<'wide' | 'narrow'>, default: 'wide' },
  },
  emits: { open: (_slug: string) => true, quickBuy: (_p: Product) => true },
  setup(props, { emit }) {
    return () => {
      const grid = cn('grid grid-cols-2 gap-3 md:gap-5', props.cols === 'wide' ? 'md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5' : 'md:grid-cols-3 xl:grid-cols-4')
      if (props.loading) {
        return (
          <div class={grid}>
            {Array.from({ length: props.skeletonCount }).map((_, i) => (
              <div key={i} class="zs-card overflow-hidden p-0">
                <div class="zs-skeleton aspect-[4/3] rounded-none" />
                <div class="space-y-2 p-4">
                  <div class="zs-skeleton h-3 w-1/3" />
                  <div class="zs-skeleton h-5 w-3/4" />
                  <div class="zs-skeleton h-6 w-1/2" />
                </div>
              </div>
            ))}
          </div>
        )
      }
      return (
        <div class={grid}>
          {props.products.map((p, i) => (
            <ProductCard key={p.id} product={p} index={i} onClick={(slug: string) => emit('open', slug)} onQuickBuy={(x: Product) => emit('quickBuy', x)} />
          ))}
        </div>
      )
    }
  },
})
