import { computed, defineComponent, toRef, watch, type PropType } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowRight, LogIn, ShoppingCart } from 'lucide-vue-next'
import type { Product } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { useProductPurchase } from '@/composables/useProductPurchase'
import { Alert, Button, Modal, QuantityStepper, SmartImage } from '@/components/ui'
import { isProductSoldOut } from '@/utils/productPricing'
import { ProductBadges } from './ProductBadges'
import { PriceBlock, RulePanels, SkuSelector } from './PurchaseParts'

/** Quick-buy dialog opened from product cards. */
export const ProductQuickBuy = defineComponent({
  name: 'ProductQuickBuy',
  props: {
    product: { type: Object as PropType<Product | null>, default: null },
    open: Boolean,
  },
  emits: { close: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const router = useRouter()
    const { getLocalizedText } = useLocalized()
    const productRef = toRef(props, 'product')
    const engine = useProductPurchase(productRef)
    const title = computed(() => getLocalizedText(props.product?.title))

    watch(
      () => [props.product, props.open] as const,
      ([p, open]) => {
        if (open && p) engine.reset()
      },
      { immediate: true },
    )

    const close = () => emit('close')

    return () => {
      const p = props.product
      return (
        <Modal open={props.open && !!p} title={t('quickBuy.title')} size="lg" sheet onClose={close}>
          {{
            default: () =>
              p && (
                <div class="space-y-5">
                  <div class="flex gap-4">
                    <div class="size-20 shrink-0 overflow-hidden rounded-zs border border-line sm:size-24">
                      <SmartImage src={p.images?.[0] || p.category?.icon} alt={title.value} />
                    </div>
                    <div class="min-w-0 space-y-2">
                      <h4 class="zs-title line-clamp-2 text-lg text-fg">{title.value}</h4>
                      <ProductBadges product={p} />
                      <p class="line-clamp-2 text-sm text-muted">{getLocalizedText(p.description)}</p>
                    </div>
                  </div>
                  <PriceBlock engine={engine} size="md" />
                  <RulePanels engine={engine} />
                  <SkuSelector engine={engine} compact />
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-sm font-bold text-fg">{t('quickBuy.quantity')}</span>
                    <QuantityStepper
                      modelValue={engine.quantity.value}
                      min={engine.quantityMin.value}
                      max={engine.quantityLimit.value}
                      disabled={!engine.canPurchase.value}
                      onUpdate:modelValue={(v: number) => {
                        engine.quantity.value = v
                      }}
                      onLimit={engine.onQuantityLimit}
                    />
                  </div>
                  {engine.purchaseWarning.value && <Alert tone="warning">{engine.purchaseWarning.value}</Alert>}
                  {engine.cannotPurchaseReason.value && !engine.purchaseWarning.value && <Alert tone="error">{engine.cannotPurchaseReason.value}</Alert>}
                </div>
              ),
            footer: () =>
              p && (
                <>
                  <Button
                    variant="ghost"
                    onClick={() => {
                      close()
                      void router.push(`/products/${p.slug}`)
                    }}
                  >
                    {t('quickBuy.viewDetail')}
                    <ArrowRight class="size-4" />
                  </Button>
                  {engine.requiresLogin.value ? (
                    <Button
                      onClick={() => {
                        close()
                        engine.goLogin()
                      }}
                    >
                      <LogIn class="size-4" />
                      {t('quickBuy.loginToBuy')}
                    </Button>
                  ) : isProductSoldOut(p) ? (
                    <Button disabled>{t('quickBuy.soldOut')}</Button>
                  ) : (
                    <>
                      <Button
                        variant="secondary"
                        disabled={!engine.canPurchase.value}
                        onClick={() => {
                          if (engine.addToCart()) close()
                        }}
                      >
                        <ShoppingCart class="size-4" />
                        {t('quickBuy.addToCart')}
                      </Button>
                      <Button
                        disabled={!engine.canPurchase.value}
                        onClick={() => {
                          if (engine.buyNow()) close()
                        }}
                      >
                        {t('quickBuy.buyNow')}
                      </Button>
                    </>
                  )}
                </>
              ),
          }}
        </Modal>
      )
    }
  },
})
