import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, ArrowRight, Lock, Pencil, ShoppingBag, Trash2, UserPlus, Zap } from 'lucide-vue-next'
import { CheckoutSteps } from '@/components/checkout/CheckoutSteps'
import { useCart } from '@/composables/useCart'
import { Alert, Badge, Button, Card, EmptyState, Mascot, QuantityStepper, SectionTitle, SmartImage } from '@/components/ui'

export default defineComponent({
  name: 'Cart',
  setup() {
    const { t } = useI18n()
    const c = useCart()

    return () => (
      <div class="zs-page space-y-6 py-6">
        <SectionTitle as="h1" size="xl" title={t('cart.title')} subtitle={t('cart.subtitle')} />
        <CheckoutSteps current="cart" />
        {c.items.value.length === 0 ? (
          <Card>
            <div class="flex flex-col items-center gap-2 py-8">
              <div class="h-44 w-36">
                <Mascot mood="sad" />
              </div>
              <EmptyState size="sm" title={t('cart.empty')} description={t('zs.mascotEmpty')}>
                <Button to="/products">
                  <ShoppingBag class="size-4" />
                  {t('cart.emptyAction')}
                </Button>
              </EmptyState>
            </div>
          </Card>
        ) : (
          <div class="grid gap-6 lg:grid-cols-[1fr_22rem]">
            <div class="space-y-3">
              {c.items.value.map((item) => {
                const stockHint = c.stockHint(item)
                const warning = c.warning(item)
                return (
                  <Card key={`${item.productId}:${item.skuId}`} padding="sm" class="zs-card-hover">
                    <div class="flex gap-4">
                      <RouterLink to={`/products/${item.slug}`} class="size-20 shrink-0 overflow-hidden rounded-zs border border-line sm:size-24">
                        <SmartImage src={c.itemImage(item)} alt={c.getLocalizedText(item.title)} />
                      </RouterLink>
                      <div class="min-w-0 flex-1 space-y-2">
                        <div class="flex items-start justify-between gap-3">
                          <RouterLink to={`/products/${item.slug}`} class="line-clamp-2 font-bold text-fg hover:text-primary-text">
                            {c.getLocalizedText(item.title)}
                          </RouterLink>
                          <button
                            type="button"
                            aria-label={t('cart.remove')}
                            class="flex size-8 shrink-0 items-center justify-center rounded-full text-muted transition hover:bg-danger-soft hover:text-danger-text"
                            onClick={() => c.removeWithUndo(item)}
                          >
                            <Trash2 class="size-4" />
                          </button>
                        </div>
                        <div class="flex flex-wrap items-center gap-1.5">
                          <Badge size="xs" tone="neutral">
                            {t('cart.skuLabel')}: {c.itemSku(item)}
                          </Badge>
                          <Badge size="xs" tone={item.purchaseType === 'guest' ? 'warning' : 'success'}>
                            {item.purchaseType === 'guest' ? <UserPlus class="size-3" /> : <Lock class="size-3" />}
                            {item.purchaseType === 'guest' ? t('productPurchase.guest') : t('productPurchase.member')}
                          </Badge>
                          <Badge size="xs" tone={item.fulfillmentType === 'auto' ? 'info' : 'neutral'}>
                            {item.fulfillmentType === 'auto' ? <Zap class="size-3" /> : <Pencil class="size-3" />}
                            {item.fulfillmentType === 'auto' ? t('products.fulfillmentType.auto') : t('products.fulfillmentType.manual')}
                          </Badge>
                          {c.hasWholesale(item) && (
                            <Badge size="xs" tone="success">
                              {t('products.wholesaleTag')}
                            </Badge>
                          )}
                        </div>
                        <div class="flex flex-wrap items-end justify-between gap-3">
                          <div class="space-y-1">
                            <div class="text-xs text-muted">
                              {t('cart.priceLabel')}{' '}
                              <span class="zs-num font-bold text-fg">{c.formatPrice(c.unitPrice(item))}</span>
                              {c.hasWholesale(item) && <span class="zs-num ml-1.5 line-through">{c.formatPrice(item.priceAmount)}</span>}
                            </div>
                            <QuantityStepper
                              size="sm"
                              modelValue={item.quantity}
                              min={c.minQty(item)}
                              max={null}
                              onUpdate:modelValue={(v: number) => c.updateQty(item, v)}
                            />
                          </div>
                          <div class="text-right">
                            <div class="text-xs text-muted">{t('checkout.totalPriceLabel')}</div>
                            <div class="zs-num text-lg font-bold text-primary-text">{c.formatPrice(c.subtotal(item))}</div>
                          </div>
                        </div>
                        {stockHint && <p class="text-xs text-muted">{stockHint}</p>}
                        {warning && (
                          <Alert tone="warning">
                            {warning}
                          </Alert>
                        )}
                      </div>
                    </div>
                  </Card>
                )
              })}
            </div>
            <aside>
              <Card class="sticky top-24 space-y-4">
                <h3 class="zs-title text-xl">{t('cart.summaryTitle')}</h3>
                <div class="flex items-center justify-between text-sm">
                  <span class="text-muted">{t('cart.itemsCount')}</span>
                  <span class="zs-num font-bold">{c.totalItems.value}</span>
                </div>
                <div class="zs-divider" />
                <div class="flex items-baseline justify-between">
                  <span class="font-bold">{t('cart.totalLabel')}</span>
                  <span class="zs-num text-3xl font-bold zs-gradient-text">{c.formatPrice(c.totalAmount.value)}</span>
                </div>
                <p class="text-xs leading-relaxed text-muted">{t('cart.disclaimer')}</p>
                <Button size="lg" block to="/checkout">
                  {t('cart.checkout')}
                  <ArrowRight class="size-4" />
                </Button>
                <Button variant="ghost" block to="/products">
                  <ArrowLeft class="size-4" />
                  {t('cart.emptyAction')}
                </Button>
              </Card>
            </aside>
          </div>
        )}
      </div>
    )
  },
})
