import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, Clock, CreditCard, PartyPopper, RefreshCw, Wallet } from 'lucide-vue-next'
import { InfoTile } from '@/components/order/AmountRow'
import { PaymentQrPanel } from '@/components/payment/PaymentQrPanel'
import { Badge, Button, Card, CardHeader, EmptyState, Mascot, SectionTitle } from '@/components/ui'
import { usePageTitle } from '@/composables/usePageTitle'
import { useRechargeOrderDetail } from '@/composables/useRechargeOrderDetail'

export default defineComponent({
  name: 'RechargeOrderDetailView',
  setup() {
    const { t } = useI18n()
    const r = useRechargeOrderDetail()
    usePageTitle(() => t('rechargeOrder.title'))
    return () => {
      const rc = r.recharge.value
      return (
        <div class="zs-page py-8 sm:py-10">
          <div class="mb-8 flex flex-wrap items-end justify-between gap-4">
            <SectionTitle as="h1" size="xl" title={t('rechargeOrder.title')} subtitle={t('rechargeOrder.subtitle')} />
            <RouterLink to="/me/orders" class="inline-flex items-center gap-1 text-sm font-bold text-muted transition hover:text-primary-text">
              <ArrowLeft class="size-4" />
              {t('rechargeOrder.backList')}
            </RouterLink>
          </div>
          {r.loading.value ? (
            <div class="zs-card h-40 p-6">
              <div class="zs-skeleton h-full" />
            </div>
          ) : !rc ? (
            <Card>
              <EmptyState variant="error" title={t('rechargeOrder.notFound')}>
                <Button variant="secondary" onClick={() => void r.loadDetail()}>
                  {t('errorBoundary.retry')}
                </Button>
              </EmptyState>
            </Card>
          ) : (
            <div class="space-y-6">
              {rc.status === 'success' && (
                <div class="zs-card flex items-center gap-4 border-success/40 p-5">
                  <div class="h-20 w-16 shrink-0">
                    <Mascot mood="happy" float={false} />
                  </div>
                  <div>
                    <p class="zs-title flex items-center gap-2 text-lg text-success-text">
                      <PartyPopper class="size-5" />
                      {t('personalCenter.wallet.rechargeSuccess')}
                    </p>
                    <p class="mt-1 text-sm text-muted">{t('orderZs.rechargeSuccessHint')}</p>
                  </div>
                  <Button class="ml-auto" size="sm" variant="secondary" to="/me/wallet">
                    {t('orderZs.goWallet')}
                  </Button>
                </div>
              )}
              <Card>
                <div class="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
                  <div class="flex items-start gap-3">
                    <span class="flex size-12 shrink-0 items-center justify-center rounded-zs bg-gold-soft text-warning-text">
                      <Wallet class="size-6" />
                    </span>
                    <div>
                      <div class="text-xs text-muted">{t('personalCenter.wallet.rechargeNoLabel')}</div>
                      <div class="zs-num mt-0.5 break-all text-base font-bold text-fg">{rc.recharge_no}</div>
                      <div class="mt-1 text-xs text-muted">
                        {t('rechargeOrder.createdAtLabel')}：{r.formatDate(rc.created_at)}
                      </div>
                    </div>
                  </div>
                  <div class="flex flex-col items-start gap-2 md:items-end">
                    <div class="text-xs text-muted">{t('rechargeOrder.rechargeAmount')}</div>
                    <div class="zs-num text-2xl font-bold zs-gradient-text">{r.money(rc.amount, rc.currency)}</div>
                    <Badge tone={r.statusTone(rc.status)}>{r.statusText(rc.status)}</Badge>
                  </div>
                </div>
              </Card>
              <Card>
                <CardHeader title={t('rechargeOrder.amountTitle')}>{{ icon: () => <CreditCard class="size-5 text-primary" /> }}</CardHeader>
                <div class={['grid grid-cols-1 gap-3', r.customerFeeApplied.value ? 'md:grid-cols-3' : 'md:grid-cols-2']}>
                  <InfoTile label={t('rechargeOrder.rechargeAmount')} value={r.money(rc.amount, rc.currency)} />
                  {r.customerFeeApplied.value && <InfoTile label={t('payment.feeAmountLabel')} value={r.money(rc.fee_amount, rc.currency)} tone="member" />}
                  <InfoTile label={t('personalCenter.wallet.payAmountLabel')} value={r.money(rc.payable_amount, rc.currency)} />
                </div>
              </Card>
              <Card>
                <CardHeader title={t('rechargeOrder.timeTitle')}>{{ icon: () => <Clock class="size-5 text-primary" /> }}</CardHeader>
                <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
                  <InfoTile label={t('rechargeOrder.createdAtLabel')} value={r.formatDate(rc.created_at)} />
                  {rc.paid_at && <InfoTile label={t('rechargeOrder.paidAtLabel')} value={r.formatDate(rc.paid_at)} />}
                  {r.payment.value?.expires_at && <InfoTile label={t('payment.expiresAt')} value={r.formatDate(r.payment.value.expires_at)} />}
                </div>
              </Card>
              {rc.remark && (
                <Card>
                  <CardHeader title={t('rechargeOrder.remarkLabel')} />
                  <p class="text-sm text-muted">{rc.remark}</p>
                </Card>
              )}
              {r.isPending.value && (
                <Card>
                  <CardHeader title={t('rechargeOrder.paymentTitle')} description={t('personalCenter.wallet.pendingHint')} />
                  <PaymentQrPanel
                    showQr={r.link.showQRCode.value}
                    qrContent={r.link.qrDisplayContent.value}
                    qrImage={r.link.qrDirectImage.value}
                    qrFallback={r.link.qrUsingPayLinkFallback.value}
                    title={t('payment.qrTitle')}
                    payLink={r.link.payLink.value}
                    details={r.link.cryptoDetails.value}
                    walletAddress={r.link.cryptoWalletAddress.value}
                    opened={r.link.openedPayWindow.value}
                    openedTip={t('payment.redirectOpened')}
                    telegramHint={r.link.showTelegramPayHint.value}
                    onOpen={() => r.link.openPayLink()}
                  />
                  <div class="mt-4 flex justify-center">
                    <Button variant="secondary" loading={r.checkingPayment.value} onClick={() => void r.checkPayment()}>
                      <RefreshCw class="size-4" />
                      {r.checkingPayment.value ? t('personalCenter.wallet.checkingPayStatus') : t('personalCenter.wallet.checkPayStatus')}
                    </Button>
                  </div>
                </Card>
              )}
            </div>
          )}
        </div>
      )
    }
  },
})
