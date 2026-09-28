import { defineComponent, Transition, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { LogIn, ShoppingCart } from 'lucide-vue-next'
import { useLocalized } from '@/composables/useLocalized'
import { Button } from '@/components/ui'
import type { PurchaseEngine } from './PurchaseParts'

/** Fixed bottom purchase bar on mobile (above the bottom nav). */
export const ProductMobileBar = defineComponent({
  name: 'ProductMobileBar',
  props: {
    engine: { type: Object as PropType<PurchaseEngine>, required: true },
    visible: Boolean,
  },
  setup(props) {
    const { t } = useI18n()
    const { formatPrice } = useLocalized()
    return () => {
      const e = props.engine
      const d = e.priceDisplay.value
      return (
        <Transition name="zs-fade">
          {props.visible && (
            <div class="fixed inset-x-3 bottom-[calc(4.5rem+env(safe-area-inset-bottom))] z-30 lg:hidden">
              <div class="zs-glass flex items-center gap-3 rounded-full py-2 pl-5 pr-2 shadow-zs-lg">
                <div class="min-w-0 flex-1">
                  <div class="zs-num truncate text-lg font-bold zs-gradient-text">{formatPrice(d.price)}</div>
                  {d.original && <div class="zs-num text-xs text-muted line-through">{formatPrice(d.original)}</div>}
                </div>
                {e.requiresLogin.value ? (
                  <Button size="sm" onClick={e.goLogin}>
                    <LogIn class="size-4" />
                    {t('productDetail.loginToBuy')}
                  </Button>
                ) : (
                  <>
                    <Button size="icon" variant="soft" disabled={!e.canPurchase.value} onClick={() => e.addToCart()}>
                      <ShoppingCart class="size-4" />
                    </Button>
                    <Button size="sm" disabled={!e.canPurchase.value} onClick={() => e.buyNow()}>
                      {t('productDetail.buyNow')}
                    </Button>
                  </>
                )}
              </div>
            </div>
          )}
        </Transition>
      )
    }
  },
})
