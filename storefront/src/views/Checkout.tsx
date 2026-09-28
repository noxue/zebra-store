import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { LogIn, ShieldCheck, ShoppingBag, Sparkles, Ticket, UserRound } from 'lucide-vue-next'
import { CheckoutManualForm } from '@/components/checkout/CheckoutManualForm'
import { CheckoutSteps } from '@/components/checkout/CheckoutSteps'
import { CaptchaField } from '@/components/common/CaptchaField'
import { AmountRow } from '@/components/order/AmountRow'
import { PaymentChannelSelector } from '@/components/payment/PaymentChannelSelector'
import { WalletBalanceBox } from '@/components/payment/WalletBalanceBox'
import { Alert, Badge, Button, Card, CardHeader, EmptyState, Field, Input, Mascot, SectionTitle, SmartImage, cn } from '@/components/ui'
import { useCheckout } from '@/composables/useCheckout'
import { usePageTitle } from '@/composables/usePageTitle'
import type { ManualFormValues } from '@/utils/manualForm'
import { hasPositive } from '@/utils/orderPayment'

export default defineComponent({
  name: 'CheckoutView',
  setup() {
    const { t } = useI18n()
    const c = useCheckout()
    usePageTitle(() => t('checkout.title'))

    const discount = (amount: string) => (hasPositive(amount) ? `-${c.formatPrice(amount, c.previewCurrency.value)}` : c.formatPrice(amount, c.previewCurrency.value))

    const itemsCard = () => (
      <Card>
        <CardHeader title={t('checkout.itemsTitle')}>{{ icon: () => <ShoppingBag class="size-5 text-primary" /> }}</CardHeader>
        <div class="space-y-3">
          {c.cartItems.value.map((item) => {
            const exceeded = c.itemStockExceeded(item)
            const hint = c.itemStockHint(item)
            const sku = c.itemSkuDisplay(item)
            return (
              <div key={c.cartItemKey(item)} class={cn('flex gap-3 rounded-zs border p-3 sm:p-4', exceeded ? 'border-warning/40 bg-warning-soft' : 'border-line bg-surface-strong')}>
                <div class="size-16 shrink-0 overflow-hidden rounded-zs-sm border border-line sm:size-20">
                  <SmartImage src={c.itemImage(item)} alt={c.getLocalizedText(item.title)} />
                </div>
                <div class="min-w-0 flex-1">
                  <RouterLink to={`/products/${item.slug}`} class="line-clamp-2 font-bold text-fg transition hover:text-primary-text">
                    {c.getLocalizedText(item.title)}
                  </RouterLink>
                  <div class="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted">
                    <span>
                      {t('checkout.quantityLabel')}：<span class="zs-num font-bold text-fg">{item.quantity}</span>
                    </span>
                    {sku && (
                      <span>
                        {t('checkout.skuLabel')}：{sku}
                      </span>
                    )}
                  </div>
                  {hint && <div class={cn('mt-1 text-xs', exceeded ? 'text-warning-text' : 'text-muted')}>{hint}</div>}
                  <div class="mt-2 flex flex-wrap items-baseline gap-2">
                    <span class={cn('zs-num text-lg font-bold', c.itemHasDiscount(item) ? 'text-primary-text' : 'text-fg')}>
                      {c.formatPrice(c.itemPayableAmount(item), c.previewCurrency.value)}
                    </span>
                    {c.itemHasDiscount(item) && (
                      <>
                        <Badge tone="primary" size="xs">
                          {t('checkout.discountedPriceLabel')}
                        </Badge>
                        <span class="zs-num text-xs text-muted line-through">{c.formatPrice(c.itemOriginalAmount(item), c.previewCurrency.value)}</span>
                      </>
                    )}
                  </div>
                </div>
              </div>
            )
          })}
        </div>
      </Card>
    )

    const guestCard = () => (
      <Card>
        <CardHeader title={t('checkout.modeTitle')}>{{ icon: () => <UserRound class="size-5 text-primary" /> }}</CardHeader>
        <div class="flex flex-wrap gap-3">
          <Button variant={c.checkoutMode.value === 'guest' ? 'primary' : 'secondary'} onClick={() => (c.checkoutMode.value = 'guest')}>
            <UserRound class="size-4" />
            {t('checkout.guestPurchase')}
          </Button>
          <Button variant="secondary" to={{ path: '/auth/login', query: { redirect: '/checkout' } }}>
            <LogIn class="size-4" />
            {t('checkout.memberPurchase')}
          </Button>
        </div>
        {c.checkoutMode.value === 'guest' && (
          <div class="mt-5 space-y-4">
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
              <Field error={c.guestEmail.value && !c.guestEmailValid.value ? t('error.email_invalid') : ''}>
                <Input v-model={c.guestEmail.value} type="email" autocomplete="email" placeholder={t('checkout.guestEmailPlaceholder')} invalid={!!c.guestEmail.value && !c.guestEmailValid.value} />
              </Field>
              <Input v-model={c.guestPassword.value} type="password" autocomplete="new-password" placeholder={t('checkout.guestPasswordPlaceholder')} />
            </div>
            {c.captcha.enabled.value && (
              <Field label={t('auth.common.captchaLabel')}>
                <CaptchaField captcha={c.captcha} />
              </Field>
            )}
            <div class="rounded-zs border border-success/35 bg-success-soft p-4 text-sm text-success-text">
              <p class="font-bold">{t('checkout.guestInstructions.title')}</p>
              <ul class="mt-2 list-disc space-y-1 pl-5 text-xs leading-relaxed">
                <li>{t('checkout.guestInstructions.email')}</li>
                <li>{t('checkout.guestInstructions.password')}</li>
              </ul>
            </div>
          </div>
        )}
      </Card>
    )

    const summaryCard = () => {
      const cur = c.previewCurrency.value
      return (
        <Card class="h-fit lg:sticky lg:top-24">
          <CardHeader title={t('checkout.submitTitle')} sparkle />
          <div class="mb-4 rounded-zs border border-line bg-surface-muted p-3 text-xs text-muted">{t('checkout.submitHint')}</div>
          <div class="space-y-3 text-sm">
            <AmountRow label={t('cart.itemsCount')} value={String(c.totalItems.value)} />
            <AmountRow label={t('checkout.previewOriginal')} value={c.formatPrice(c.previewOriginal.value, cur)} />
            {!c.isResellerTenant.value && (
              <>
                <AmountRow label={t('checkout.previewCoupon')} value={discount(c.previewCoupon.value)} tone={hasPositive(c.previewCoupon.value) ? 'discount' : 'default'} />
                <AmountRow label={t('checkout.previewPromotion')} value={discount(c.previewPromotion.value)} tone={hasPositive(c.previewPromotion.value) ? 'discount' : 'default'} />
                <AmountRow label={t('checkout.previewWholesale')} value={discount(c.previewWholesale.value)} tone={hasPositive(c.previewWholesale.value) ? 'wholesale' : 'default'} />
              </>
            )}
            {Number(c.previewMemberDiscount.value) > 0 && (
              <AmountRow label={t('checkout.previewMemberDiscount')} value={`-${c.formatPrice(c.previewMemberDiscount.value, cur)}`} tone="member" />
            )}
            <AmountRow label={t('checkout.previewTotal')} value={c.formatPrice(c.previewTotal.value, cur)} tone="strong" />
          </div>
          {(c.previewLoading.value || c.couponRefreshing.value) && <div class="mt-3 text-xs text-muted">{c.previewStatusText.value}</div>}
          {c.checkoutAlert.value && (
            <div class="mt-4">
              <Alert tone={c.checkoutAlert.value.level}>{c.checkoutAlert.value.message}</Alert>
            </div>
          )}
          <div class="mt-5 space-y-3 border-t border-line pt-4">
            <h3 class="text-sm font-bold text-fg">{t('checkout.paymentMethod')}</h3>
            {c.showBalanceOption.value && (
              <WalletBalanceBox
                v-model={c.useBalance.value}
                balanceText={c.formatPrice(c.walletBalance.value, cur)}
                loading={c.walletLoading.value}
                walletOnly={c.walletOnlyPayment.value}
                walletPaidText={c.expectedWalletPaidDisplay.value}
                onlinePayText={c.expectedOnlinePayDisplay.value}
                insufficient={c.expectedOnlinePayCents.value > 0}
              />
            )}
            {!c.walletOnlyPayment.value && c.requiresOnlineChannel.value && (
              <PaymentChannelSelector
                compact
                channels={c.paymentChannels.value}
                modelValue={c.selectedChannelId.value}
                isDisabled={c.isChannelDisabledForAmount}
                limitHint={c.channelAmountLimitHint}
                fixedFee={c.formatChannelFixedFee}
                emptyText={t('checkout.noPaymentChannels')}
                onUpdate:modelValue={(id: number) => {
                  const ch = c.paymentChannels.value.find((x) => Number(x.id) === id)
                  if (ch) c.selectChannel(ch)
                }}
              />
            )}
            {!c.requiresOnlineChannel.value && <div class="text-xs font-bold text-success-text">{t('checkout.walletCoversAll')}</div>}
          </div>
          <Button class="mt-5" size="lg" block loading={c.submitting.value} disabled={!c.canSubmit.value} onClick={() => void c.handleSubmit()}>
            <Sparkles class="size-4" />
            {c.submitting.value ? t('checkout.submitting') : t('checkout.submitButton')}
          </Button>
          <div class="mt-3 flex items-center justify-center gap-1.5 text-xs text-muted">
            <ShieldCheck class="size-3.5" />
            {t('orderZs.secureHint')}
          </div>
        </Card>
      )
    }

    return () => (
      <div class="zs-page py-8 sm:py-10">
        <div class="mb-6 flex items-end justify-between gap-4">
          <SectionTitle as="h1" size="xl" title={t('checkout.title')} subtitle={t('checkout.subtitle')} />
          <div class="hidden h-24 w-20 shrink-0 md:block" title={t('orderZs.checkoutMascot')}>
            <Mascot mood="wink" />
          </div>
        </div>
        <div class="mb-8">
          <CheckoutSteps current="checkout" stepKeys={c.isBuyNowMode.value ? ['checkout', 'payment'] : ['cart', 'checkout', 'payment']} />
        </div>
        {c.cartItems.value.length === 0 ? (
          <Card>
            <EmptyState title={t('checkout.empty')}>
              <Button to="/products">{t('checkout.emptyAction')}</Button>
            </EmptyState>
          </Card>
        ) : (
          <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
            <div class="space-y-6 lg:col-span-2">
              {itemsCard()}
              <CheckoutManualForm
                modelValue={c.manualFormData.value}
                onUpdate:modelValue={(v: ManualFormValues) => (c.manualFormData.value = v)}
                products={c.manualFormProducts.value}
                submitAttempted={c.submitAttempted.value}
                fieldError={c.manualFieldError}
              />
              {!c.isResellerTenant.value && (
                <Card>
                  <CardHeader title={t('checkout.couponTitle')}>{{ icon: () => <Ticket class="size-5 text-primary" /> }}</CardHeader>
                  <Input v-model={c.couponCode.value} placeholder={t('checkout.couponPlaceholder')} />
                </Card>
              )}
              {!c.auth.isAuthenticated && guestCard()}
            </div>
            {summaryCard()}
          </div>
        )}
      </div>
    )
  },
})
