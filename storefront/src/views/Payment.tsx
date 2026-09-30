import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, CreditCard, History, ReceiptText, RefreshCw, ShieldCheck, Timer } from 'lucide-vue-next'
import { CheckoutSteps } from '@/components/checkout/CheckoutSteps'
import { OrderStatusBadge } from '@/components/common/OrderStatusBadge'
import { AmountRow, InfoTile } from '@/components/order/AmountRow'
import { GuestAuthForm } from '@/components/order/GuestAuthForm'
import { PaymentAmountBreakdown } from '@/components/payment/PaymentAmountBreakdown'
import { PaymentChannelSelector } from '@/components/payment/PaymentChannelSelector'
import { PaymentQrPanel } from '@/components/payment/PaymentQrPanel'
import { WalletBalanceBox } from '@/components/payment/WalletBalanceBox'
import { Alert, Badge, Button, Card, CardHeader, EmptyState, Mascot, SectionTitle, Skeleton, cn } from '@/components/ui'
import { usePageTitle } from '@/composables/usePageTitle'
import { usePayment } from '@/composables/usePayment'
import { fulfillmentTypeLabel } from '@/utils/fulfillment'
import { hasPositive } from '@/utils/orderPayment'
import { buildSkuDisplayTextFromSnapshot } from '@/utils/sku'
import { useAppStore } from '@/stores/app'

export default defineComponent({
  name: 'PaymentView',
  setup() {
    const { t } = useI18n()
    const appStore = useAppStore()
    const p = usePayment()
    usePageTitle(() => t('payment.title'))

    const statusText = (s: string) => t(`order.status.${s}`)
    const disc = (v?: string) => (hasPositive(v) ? `-${p.money(v)}` : p.money(v))

    const skeleton = () => (
      <Card>
        <div class="space-y-4">
          <div class="h-5 w-40">
            <Skeleton />
          </div>
          <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
            <div class="mx-auto aspect-square w-full max-w-[260px] lg:col-span-2 lg:mx-0">
              <Skeleton />
            </div>
            <div class="space-y-3">
              {[1, 2, 3].map((i) => (
                <div key={i} class="h-4">
                  <Skeleton />
                </div>
              ))}
            </div>
          </div>
        </div>
      </Card>
    )

    const resultView = () => {
      const order = p.order.value
      const result = p.paymentResult.value
      if (!order || !result) return null
      return (
        <Card>
          <div class="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
            <div>
              <h2 class="zs-title text-2xl text-fg">{p.paymentResultTitle.value}</h2>
              <p class="mt-1 text-sm text-muted">{p.paymentGuideTip.value}</p>
              <div class="mt-2 text-xs text-muted">
                {t('payment.methodLabel')}：<span class="font-bold text-fg">{p.resultChannelName.value}</span>
              </div>
            </div>
            <div class="flex flex-wrap gap-2">
              <Button variant="secondary" size="sm" disabled={p.loading.value} onClick={() => void p.handleRefresh()}>
                <RefreshCw class="size-4" />
                {t('payment.refreshStatus')}
              </Button>
              <Button variant="secondary" size="sm" onClick={p.handleChangePaymentMethod}>
                <CreditCard class="size-4" />
                {t('payment.changeMethod')}
              </Button>
            </div>
          </div>
          <div class="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-3">
            <div class="lg:col-span-2">
              <PaymentQrPanel
                showQr={p.link.showQRCode.value}
                qrContent={p.link.qrDisplayContent.value}
                qrImage={p.link.qrDirectImage.value}
                qrFallback={p.link.qrUsingPayLinkFallback.value}
                title={p.paymentGuideTitle.value}
                payLink={p.link.payLink.value}
                details={p.link.cryptoDetails.value}
                walletAddress={p.link.cryptoWalletAddress.value}
                opened={p.link.openedPayWindow.value}
                openedTip={p.payLinkOpenedTip.value}
                telegramHint={p.link.showTelegramPayHint.value}
                onOpen={() => p.link.openPayLink()}
              />
            </div>
            <div class="space-y-4">
              <div class="rounded-zs-lg border border-line bg-surface-strong p-4 text-xs text-muted">
                <div>{t('payment.orderNo')}</div>
                <div class="zs-num mt-1 break-all text-sm font-bold text-fg">{order.order_no}</div>
                <div class="mt-3 flex items-center gap-2">
                  {t('payment.orderStatus')}：<OrderStatusBadge status={order.status} />
                </div>
                <div class="mt-2">
                  {t('payment.methodLabel')}：{p.resultChannelName.value}
                </div>
                <div class="mt-2">
                  {t('payment.interactionLabel')}：{p.interactionLabel.value}
                </div>
              </div>
              <PaymentAmountBreakdown
                order={order}
                payment={result}
                money={(v) => p.money(v)}
                payableDisplay={p.payableAmountDisplay.value}
                feeDisplay={p.customerFeeAmountDisplay.value}
                walletPaidDisplay={p.paymentWalletPaidDisplay.value}
                onlinePayDisplay={p.paymentOnlinePayDisplay.value}
                showCountdown={p.showCountdown.value}
                countdownText={p.countdownText.value}
                polling={p.pollingActive.value}
              />
              {result.expires_at && (
                <div class="rounded-zs border border-line bg-surface-strong p-3 text-xs text-muted">
                  {t('payment.expiresAt')}：{p.formatDate(result.expires_at)}
                </div>
              )}
            </div>
          </div>
        </Card>
      )
    }

    const closedView = () => {
      const order = p.order.value
      if (!order) return null
      return (
        <Card>
          <div class="flex flex-col items-center gap-4 py-4 text-center md:flex-row md:text-left">
            <div class="h-28 w-24 shrink-0">
              <Mascot mood="sad" float={false} />
            </div>
            <div class="flex-1">
              <h2 class="zs-title text-2xl text-fg">{p.orderCanceled.value ? t('payment.orderCanceled') : t('payment.orderExpired')}</h2>
              <p class="zs-num mt-1 text-sm text-muted">{order.order_no}</p>
            </div>
            <Button variant="secondary" to={p.backLink.value}>
              {t('payment.backToOrders')}
            </Button>
          </div>
          <div class="mt-6 grid grid-cols-1 gap-3 text-sm sm:grid-cols-3">
            <InfoTile label={t('payment.orderNo')} value={order.order_no} />
            <InfoTile label={t('payment.orderStatus')} value={statusText(order.status)} mono={false} />
            <InfoTile label={t('orderDetail.amountTotal')} value={p.money(order.total_amount)} />
          </div>
        </Card>
      )
    }

    const prePaymentView = () => {
      const order = p.order.value
      if (!order) return null
      return (
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
          <div class="space-y-6 lg:col-span-2">
            <Card>
              <CardHeader title={t('payment.orderInfo')}>{{ icon: () => <ReceiptText class="size-5 text-primary" /> }}</CardHeader>
              <div class="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
                <div>
                  <div class="text-xs text-muted">{t('payment.orderNo')}</div>
                  <div class="zs-num mt-1 break-all text-sm font-bold text-fg">{order.order_no}</div>
                  <div class="mt-2 text-xs text-muted">
                    {t('orderDetail.createdAtLabel')}：{p.formatDate(order.created_at)}
                  </div>
                  {order.expires_at && (
                    <div class="mt-1 text-xs text-muted">
                      {t('payment.expiresAt')}：{p.formatDate(order.expires_at)}
                    </div>
                  )}
                  {p.showCountdown.value && (
                    <div class="mt-3">
                      <Badge tone={p.countdownExpired.value ? 'danger' : 'success'} size="md">
                        <Timer class="size-3.5" />
                        {t('payment.countdownLabel')}
                        <span class="zs-num">{p.countdownText.value}</span>
                      </Badge>
                    </div>
                  )}
                  {p.pollingActive.value && <div class="mt-2 text-xs text-muted">{t('payment.pollingHint')}</div>}
                </div>
                <div class="w-full rounded-zs-lg border border-line zs-soft-bg p-4 md:w-auto md:min-w-[280px]">
                  <div class="text-xs text-muted md:text-right">{t('payment.payableAmountLabel')}</div>
                  <div class="zs-num mt-1 text-3xl font-bold zs-gradient-text md:text-right">{p.payableAmountDisplay.value}</div>
                  <div class="mt-4 space-y-2 text-xs">
                    <AmountRow label={t('orderDetail.amountTotal')} value={p.money(order.total_amount)} />
                    {p.showBalanceOption.value && p.useBalance.value && (
                      <>
                        <AmountRow label={t('payment.walletDeductLabel')} value={p.expectedWalletPaidDisplay.value} />
                        <AmountRow label={t('payment.onlinePayLabel')} value={p.expectedOnlinePayDisplay.value} />
                      </>
                    )}
                  </div>
                  <div class="mt-3 flex items-center justify-between border-t border-line pt-3 text-xs">
                    <span class="text-muted">{t('payment.orderStatus')}</span>
                    <OrderStatusBadge status={order.status} />
                  </div>
                </div>
              </div>
              <div class="mt-5 grid grid-cols-1 gap-3 text-sm sm:grid-cols-3">
                <InfoTile label={t('orderDetail.amountOriginal')} value={p.money(order.original_amount)} />
                <InfoTile label={t('orderDetail.amountDiscount')} value={disc(order.discount_amount)} tone={hasPositive(order.discount_amount) ? 'discount' : 'default'} />
                <InfoTile label={t('orderDetail.promotionDiscountLabel')} value={disc(order.promotion_discount_amount)} tone={hasPositive(order.promotion_discount_amount) ? 'discount' : 'default'} />
                {hasPositive(order.wholesale_discount_amount) && <InfoTile label={t('orderDetail.amountWholesaleDiscount')} value={disc(order.wholesale_discount_amount)} tone="wholesale" />}
              </div>
            </Card>

            {p.orderItems.value.length > 0 && (
              <Card>
                <CardHeader title={t('payment.itemsTitle')} />
                <div class="divide-y divide-line">
                  {p.orderItems.value.map((item, idx) => {
                    const sku = buildSkuDisplayTextFromSnapshot(item.sku_snapshot, { locale: appStore.locale, fallback: t('productDetail.skuFallback') })
                    return (
                      <div key={idx} class="flex flex-col gap-2 py-3 first:pt-0 last:pb-0 md:flex-row md:items-center md:justify-between">
                        <div>
                          <div class="font-bold text-fg">{p.getLocalizedText(item.title)}</div>
                          <div class="mt-1 text-xs text-muted">
                            {t('orderDetail.quantityLabel')}：{item.quantity} · {t('orderDetail.itemFulfillmentLabel')}：{fulfillmentTypeLabel(t, item.fulfillment_type)}
                          </div>
                          {sku && (
                            <div class="mt-1 text-xs text-muted">
                              {t('orderDetail.itemSkuLabel')}：{sku}
                            </div>
                          )}
                        </div>
                        <div class="zs-num text-sm font-bold text-fg">{p.money(item.total_price)}</div>
                      </div>
                    )
                  })}
                </div>
              </Card>
            )}

            <Card>
              <CardHeader title={t('payment.channelTitle')}>{{ icon: () => <CreditCard class="size-5 text-primary" /> }}</CardHeader>
              {!p.configReady.value ? (
                <div class="text-sm text-muted">{t('common.loading')}</div>
              ) : (
                <div class="space-y-4">
                  {p.showBalanceOption.value && (
                    <WalletBalanceBox
                      v-model={p.useBalance.value}
                      balanceText={p.walletBalanceDisplay.value}
                      loading={p.walletLoading.value}
                      walletOnly={p.walletOnlyPayment.value}
                      walletPaidText={p.expectedWalletPaidDisplay.value}
                      onlinePayText={p.expectedOnlinePayDisplay.value}
                      insufficient={p.expectedOnlinePayCents.value > 0}
                    />
                  )}
                  {p.cachedPayment.value && (
                    <div class="space-y-2 rounded-zs border border-warning/40 bg-warning-soft p-4 text-sm text-warning-text">
                      <div class="flex items-center gap-2 font-bold">
                        <History class="size-4" />
                        {t('payment.cachedTitle')}
                      </div>
                      <div>{t('payment.cachedHint', { channel: p.cachedChannelName.value })}</div>
                      <div class="flex flex-wrap items-center gap-3">
                        <Button size="sm" variant="secondary" onClick={p.restoreCachedPayment}>
                          {t('payment.useCached')}
                        </Button>
                        <span class="text-xs opacity-80">{t('payment.cachedCreateHint')}</span>
                      </div>
                    </div>
                  )}
                  {!p.walletOnlyPayment.value && p.selectedChannel.value && !p.showChannelSelector.value ? (
                    <div class="flex items-center justify-between gap-3 rounded-zs border border-primary/30 bg-primary-soft p-3">
                      <div class="min-w-0 text-sm">
                        <div class="text-xs text-muted">{t('payment.methodLabel')}</div>
                        <div class="truncate font-bold text-fg">{p.selectedChannelName.value}</div>
                      </div>
                      <Button variant="secondary" size="sm" onClick={p.handleChangePaymentMethod}>
                        <CreditCard class="size-4" />
                        {t('payment.changeMethod')}
                      </Button>
                    </div>
                  ) : !p.walletOnlyPayment.value && (
                    <PaymentChannelSelector
                      channels={p.channels.value}
                      modelValue={p.selectedChannelId.value}
                      channelType={p.selectedChannelType.value}
                      isDisabled={p.isChannelDisabledForAmount}
                      limitHint={p.channelAmountLimitHint}
                      emptyText={p.showBalanceOption.value ? t('payment.channelEmptyUseBalance') : t('payment.channelEmpty')}
                      onUpdate:modelValue={(id: number) => (p.selectedChannelId.value = id)}
                      onUpdate:channelType={(type: string) => (p.selectedChannelType.value = type)}
                    />
                  )}
                </div>
              )}
            </Card>
          </div>

          <Card class="h-fit lg:sticky lg:top-24">
            <CardHeader title={t('payment.actionTitle')} sparkle />
            {p.showCountdown.value && (
              <div class="mb-3 text-xs text-muted">
                {t('payment.countdownLabel')}：<span class="zs-num font-bold text-fg">{p.countdownText.value}</span>
              </div>
            )}
            {p.paymentAlert.value && (
              <div class="mb-4">
                <Alert tone={p.paymentAlert.value.level}>{p.paymentAlert.value.message}</Alert>
              </div>
            )}
            {p.selectedChannel.value ? (
              <div class="mb-4 rounded-zs border border-success/35 bg-success-soft p-3 text-xs font-bold text-success-text">
                {t('payment.methodLabel')}：{p.selectedChannelName.value}
              </div>
            ) : !p.orderExpired.value && !p.orderCanceled.value ? (
              <div
                class={cn(
                  'mb-4 rounded-zs border p-3 text-xs',
                  !p.requiresOnlineChannel.value ? 'border-success/35 bg-success-soft text-success-text' : 'border-warning/40 bg-warning-soft text-warning-text',
                )}
              >
                {!p.requiresOnlineChannel.value
                  ? t('payment.walletPayOnly')
                  : p.walletOnlyPayment.value && p.expectedOnlinePayCents.value > 0
                    ? t('payment.walletInsufficientHint')
                    : t('payment.selectChannelError')}
              </div>
            ) : null}
            <Button block size="lg" loading={p.submitting.value} disabled={!p.canSubmitPayment.value} onClick={() => p.handlePayment()}>
              {p.submitting.value ? t('payment.submitting') : t('payment.submitButton')}
            </Button>
            <Button class="mt-3" block variant="secondary" disabled={p.loading.value} onClick={() => void p.handleRefresh()}>
              <RefreshCw class="size-4" />
              {t('payment.refreshStatus')}
            </Button>
            <div class="mt-4 flex items-center justify-center gap-1.5 text-xs text-muted">
              <ShieldCheck class="size-3.5" />
              {t('orderZs.secureHint')}
            </div>
          </Card>
        </div>
      )
    }

    const body = () => {
      if (p.loading.value) return skeleton()
      if (p.showGuestAuthForm.value) {
        return (
          <GuestAuthForm
            v-model={p.guestAuth.value}
            title={t('payment.guestAuthTitle')}
            hint={t('payment.guestAuthHint')}
            error={p.guestAuthError.value}
            submitText={t('payment.guestAuthSubmit')}
            onSubmit={() => void p.handleGuestAuthSubmit()}
          />
        )
      }
      if (!p.order.value) {
        return (
          <Card>
            <EmptyState variant="error" title={t('payment.orderNotFound')}>
              <Button variant="secondary" to={p.backLink.value}>
                {t('payment.backToOrders')}
              </Button>
            </EmptyState>
          </Card>
        )
      }
      if (p.showResultView.value) return resultView()
      if (p.orderExpired.value || p.orderCanceled.value) return closedView()
      return prePaymentView()
    }

    return () => (
      <div class="zs-page py-8 sm:py-10">
        <div class="mb-6 flex items-end justify-between gap-4">
          <SectionTitle as="h1" size="xl" title={t('payment.title')} subtitle={t('payment.subtitle')} />
          <div class="flex items-end gap-3">
            <RouterLink to={p.backLink.value} class="inline-flex items-center gap-1 text-sm font-bold text-muted transition hover:text-primary-text">
              <ArrowLeft class="size-4" />
              {t('payment.backToOrders')}
            </RouterLink>
            <div class="hidden h-24 w-20 md:block" title={t('orderZs.paymentMascot')}>
              <Mascot />
            </div>
          </div>
        </div>
        <div class="mb-8">
          <CheckoutSteps current="payment" />
        </div>
        {body()}
      </div>
    )
  },
})
