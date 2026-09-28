import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Coins, Copy, HandCoins, Link2, Megaphone, MousePointerClick, RefreshCw, Send, Wallet } from 'lucide-vue-next'
import type { AffiliateCommissionData, AffiliateWithdrawData } from '@/api/types'
import { Badge, Button, Card, DataTable, Field, Input, Pagination, Select, StatCard, columns } from '@/components/ui'
import { useLocalized } from '@/composables/useLocalized'
import { useAffiliatePanel } from '@/composables/personal/useAffiliatePanel'
import { formatDateTime } from '@/utils/format'
import { DashedNote, PanelAlertBox, PanelHeading, SkeletonRows } from './PanelParts'

/** Affiliate program panel. */
export const AffiliatePanel = defineComponent({
  name: 'AffiliatePanel',
  setup() {
    const { t } = useI18n()
    const { formatPrice } = useLocalized()
    const a = useAffiliatePanel()

    const commissionColumns = () =>
      columns<AffiliateCommissionData>([
        { key: 'order_no', title: t('personalCenter.affiliate.table.orderNo'), render: (r) => <span class="zs-num text-xs">{r.order_no || '-'}</span> },
        { key: 'commission_amount', title: t('personalCenter.affiliate.table.amount'), render: (r) => <span class="zs-num font-bold">{formatPrice(r.commission_amount)}</span> },
        { key: 'status', title: t('personalCenter.affiliate.table.status'), render: (r) => <Badge tone={a.commissionTone(r.status)}>{a.commissionLabel(r.status)}</Badge> },
        { key: 'created_at', title: t('personalCenter.affiliate.table.createdAt'), render: (r) => <span class="text-xs text-muted">{formatDateTime(r.created_at)}</span> },
      ])
    const withdrawColumns = () =>
      columns<AffiliateWithdrawData>([
        { key: 'amount', title: t('personalCenter.affiliate.withdrawTable.amount'), render: (r) => <span class="zs-num font-bold">{formatPrice(r.amount)}</span> },
        { key: 'channel', title: t('personalCenter.affiliate.withdrawTable.channel'), render: (r) => `${r.channel}${r.account ? ` · ${r.account}` : ''}` },
        {
          key: 'status',
          title: t('personalCenter.affiliate.withdrawTable.status'),
          render: (r) => (
            <span class="inline-flex flex-col gap-1">
              <Badge tone={a.withdrawTone(r.status)}>{a.withdrawLabel(r.status)}</Badge>
              {r.reject_reason && <span class="text-xs text-danger-text">{r.reject_reason}</span>}
            </span>
          ),
        },
        { key: 'created_at', title: t('personalCenter.affiliate.withdrawTable.createdAt'), render: (r) => <span class="text-xs text-muted">{formatDateTime(r.created_at)}</span> },
      ])

    const dashboardView = () => {
      const d = a.dashboard.value
      if (!d) return null
      return (
        <div class="space-y-4">
          <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
            <div class="zs-soft-bg rounded-zs-lg border border-line p-5 md:col-span-2">
              <div class="text-xs font-bold text-muted">{t('personalCenter.affiliate.affiliateCode')}</div>
              <div class="mt-2 flex flex-wrap items-center gap-2">
                <span class="zs-num rounded-zs-sm border border-line-strong bg-surface-strong px-3 py-1 text-lg font-bold tracking-wider text-primary-text">{d.affiliate_code || '-'}</span>
                <Button size="sm" variant="soft" onClick={a.copyPromotionUrl}>
                  <Copy class="size-3.5" />
                  {t('personalCenter.affiliate.copyPromotionUrl')}
                </Button>
              </div>
              <div class="mt-3 flex items-center gap-2 break-all text-xs text-muted">
                <Link2 class="size-3.5 shrink-0" />
                {a.promotionUrl.value || '-'}
              </div>
            </div>
            <StatCard label={t('personalCenter.affiliate.conversionRate')} value={a.conversionRate.value} tone="accent">
              {{
                icon: () => <MousePointerClick class="size-5" />,
                default: () => (
                  <div>
                    <div>{a.conversionRate.value}</div>
                    <div class="mt-1 text-xs font-medium text-muted">
                      {t('personalCenter.affiliate.conversionDetail', { clicks: d.click_count || 0, orders: d.valid_order_count || 0 })}
                    </div>
                  </div>
                ),
              }}
            </StatCard>
          </div>
          <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <StatCard label={t('personalCenter.affiliate.pendingCommission')} value={formatPrice(d.pending_commission)} tone="gold">
              {{ icon: () => <Coins class="size-5" /> }}
            </StatCard>
            <StatCard label={t('personalCenter.affiliate.availableCommission')} value={formatPrice(d.available_commission)} tone="success">
              {{ icon: () => <Wallet class="size-5" /> }}
            </StatCard>
            <StatCard label={t('personalCenter.affiliate.withdrawnCommission')} value={formatPrice(d.withdrawn_commission)} tone="secondary">
              {{ icon: () => <HandCoins class="size-5" /> }}
            </StatCard>
          </div>
        </div>
      )
    }

    return () => (
      <div class="space-y-5">
        <Card>
          <PanelHeading title={t('personalCenter.affiliate.title')} description={t('personalCenter.affiliate.subtitle')} icon={Megaphone}>
            {{ actions: () => <Badge tone="accent">{t('personalCenter.tabs.affiliate')}</Badge> }}
          </PanelHeading>
          <PanelAlertBox alert={a.alert.value} />
          {a.loading.value ? (
            <SkeletonRows />
          ) : a.opened.value ? (
            dashboardView()
          ) : (
            <DashedNote>
              <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <span class="leading-relaxed">{t('personalCenter.affiliate.notOpened')}</span>
                <Button loading={a.opening.value} onClick={() => void a.open()}>
                  {a.opening.value ? t('personalCenter.affiliate.opening') : t('personalCenter.affiliate.openButton')}
                </Button>
              </div>
            </DashedNote>
          )}
        </Card>

        {a.opened.value && (
          <Card>
            <PanelHeading title={t('personalCenter.affiliate.withdrawTitle')} description={t('personalCenter.affiliate.withdrawSubtitle')} icon={Send} />
            <form
              class="grid grid-cols-1 gap-4 md:grid-cols-[1fr_1fr_1.4fr_auto] md:items-end"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void a.submitWithdraw()
              }}
            >
              <Field label={t('personalCenter.affiliate.withdrawAmountLabel')}>
                <Input v-model={a.form.amount} inputmode="decimal" placeholder={t('personalCenter.affiliate.withdrawAmountPlaceholder')} />
              </Field>
              <Field label={t('personalCenter.affiliate.withdrawChannelLabel')}>
                {a.channelOptions.value.length > 0 ? (
                  <Select
                    v-model={a.form.channel}
                    placeholder={t('personalCenter.affiliate.withdrawChannelPlaceholder')}
                    options={a.channelOptions.value.map((c) => ({ value: c, label: c }))}
                  />
                ) : (
                  <Input v-model={a.form.channel} placeholder={t('personalCenter.affiliate.withdrawChannelPlaceholder')} />
                )}
              </Field>
              <Field label={t('personalCenter.affiliate.withdrawAccountLabel')}>
                <Input v-model={a.form.account} placeholder={t('personalCenter.affiliate.withdrawAccountPlaceholder')} />
              </Field>
              <Button type="submit" loading={a.submitting.value}>
                {a.submitting.value ? t('personalCenter.affiliate.withdrawing') : t('personalCenter.affiliate.withdrawSubmit')}
              </Button>
            </form>
          </Card>
        )}

        {a.opened.value && (
          <Card>
            <PanelHeading title={t('personalCenter.affiliate.commissionTitle')} icon={Coins}>
              {{
                actions: () => (
                  <Button size="sm" variant="secondary" onClick={() => void a.commissions.load(a.commissions.pagination.page)}>
                    <RefreshCw class="size-4" />
                    {t('orders.filters.refresh')}
                  </Button>
                ),
              }}
            </PanelHeading>
            {!a.commissions.loading.value && a.commissions.rows.value.length === 0 ? (
              <DashedNote>{t('personalCenter.affiliate.commissionEmpty')}</DashedNote>
            ) : (
              <DataTable columns={commissionColumns()} rows={a.commissions.rows.value} loading={a.commissions.loading.value} />
            )}
            <Pagination page={a.commissions.pagination.page} totalPages={a.commissions.pagination.total_page} onChange={(pg: number) => void a.commissions.load(pg)} />
          </Card>
        )}

        {a.opened.value && (
          <Card>
            <PanelHeading title={t('personalCenter.affiliate.withdrawRecordTitle')} icon={HandCoins}>
              {{
                actions: () => (
                  <Button size="sm" variant="secondary" onClick={() => void a.withdraws.load(a.withdraws.pagination.page)}>
                    <RefreshCw class="size-4" />
                    {t('orders.filters.refresh')}
                  </Button>
                ),
              }}
            </PanelHeading>
            {!a.withdraws.loading.value && a.withdraws.rows.value.length === 0 ? (
              <DashedNote>{t('personalCenter.affiliate.withdrawEmpty')}</DashedNote>
            ) : (
              <DataTable columns={withdrawColumns()} rows={a.withdraws.rows.value} loading={a.withdraws.loading.value} />
            )}
            <Pagination page={a.withdraws.pagination.page} totalPages={a.withdraws.pagination.total_page} onChange={(pg: number) => void a.withdraws.load(pg)} />
          </Card>
        )}
      </div>
    )
  },
})
