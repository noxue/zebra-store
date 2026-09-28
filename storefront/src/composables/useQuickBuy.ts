import { ref } from 'vue'
import type { Product } from '@/api/types'

/** Quick-buy modal open state + target product. */
export function useQuickBuy() {
  const product = ref<Product | null>(null)
  const open = ref(false)
  return {
    product,
    open,
    show: (p: Product) => {
      product.value = p
      open.value = true
    },
    close: () => {
      open.value = false
    },
  }
}
