import type { Product, ProductSku } from '@/api/types'
import type { CartItem } from '@/stores/cart'
import { amountToCents, centsToAmount } from './money'

// 购物车只同步目录基础价和批发阶梯；活动、会员与优惠券金额仍以服务端结算预览为准。
export const resolveCartPricingSnapshot = (product: Product | null | undefined, sku: ProductSku | null | undefined): Partial<CartItem> => {
  const patch: Partial<CartItem> = {
    wholesalePrices: Array.isArray(product?.wholesale_prices) ? product.wholesale_prices : undefined,
  }
  const cents = amountToCents(sku?.price_amount)
  if (cents !== null && cents > 0) patch.priceAmount = centsToAmount(cents)
  return patch
}
