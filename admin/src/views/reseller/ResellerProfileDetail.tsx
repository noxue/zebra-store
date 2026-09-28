import { computed, defineComponent, onMounted, ref, type VNodeChild } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { ArrowLeft, ArrowRight, Pencil } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, EmptyState, FormField, Input, Loader, PageHeader, Tabs, type DataTableColumn } from '@/components/ui'
import type { AdminResellerDomain, AdminResellerProfileDetailLedger, AdminResellerProfileDetailOrder, AdminResellerProfileDetailWithdraw } from '@/api/types'
import { adminUrl } from '@/utils/adminBase'
import { formatDate, formatMoney, getLocalizedText } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import { getResellerProfileStatusKey } from '@/utils/resellerManagement'
import { orderStatusLabel, orderStatusTone } from '@/utils/status'
import { RefreshButton } from './components/RefreshButton'
import { ProfileEditDialog } from './components/ProfileEditDialog'
import {
  RESELLER_LEDGER_STATUSES,
  RESELLER_LEDGER_TYPES,
  RESELLER_WITHDRAW_STATUSES,
  domainStatusTone,
  ledgerStatusTone,
  profileStatusTone,
  settlementTone,
  verificationTone,
  withdrawStatusTone,
} from './resellerUtils'
import { canSetPrimaryDomain, isActiveVerifiedDomain, useResellerProfileDetail } from './useResellerProfileDetail'

const DOMAIN_STATUS_KEY: Record<string, string> = {
  pending_review: 'pendingReview',
  active: 'active',
  disabled: 'disabled',
}

export default defineComponent({
  name: 'ResellerProfileDetailView',
  setup() {
    const { t, te } = useI18n()
    const route = useRoute()
    const router = useRouter()
    const profileId = computed(() => Number(route.params.id || 0))
    const d = useResellerProfileDetail(profileId)
    const currentTab = ref('overview')
    onMounted(d.fetchDetail)

    const keyed = (key: string, raw?: string) => (raw && te(key) ? t(key) : raw || '-')
    const statusLabel = (s?: string) => t(`admin.resellerProfiles.status.${getResellerProfileStatusKey(s)}`)
    const settlementLabel = (s?: string) => (s === 'normal' || s === 'frozen' ? t(`admin.resellerProfiles.settlement.${s}`) : s || '-')
    const domainStatusLabel = (s?: string) => (s && DOMAIN_STATUS_KEY[s] ? t(`admin.resellerProfileDetail.domainStatus.${DOMAIN_STATUS_KEY[s]}`) : s || '-')
    const domainTypeLabel = (v?: string) => (v === 'subdomain' || v === 'custom' ? t(`admin.resellerProfileDetail.domainType.${v}`) : v || '-')
    const verificationLabel = (v?: string) => keyed(`admin.resellerProfileDetail.verification.${v}`, v)
    const ledgerTypeLabel = (v?: string) =>
      (RESELLER_LEDGER_TYPES as readonly string[]).includes(v || '') ? t(`admin.resellerLedgerEntries.types.${v}`) : v || '-'
    const ledgerStatusLabel = (v?: string) =>
      (RESELLER_LEDGER_STATUSES as readonly string[]).includes(v || '') ? t(`admin.resellerLedgerEntries.status.${v}`) : v || '-'
    const withdrawStatusLabel = (v?: string) =>
      (RESELLER_WITHDRAW_STATUSES as readonly string[]).includes(v || '') ? t(`admin.resellerWithdraws.status.${v}`) : v || '-'

    const link = (path: string) => `${path}?reseller_id=${profileId.value}`
    const linkButton = (href: string, label: string, size: 'xs' | 'sm' = 'sm') => (
      <RouterLink
        to={href}
        class={
          size === 'xs'
            ? 'inline-flex items-center gap-1 rounded-full border border-line-strong px-2.5 py-1 text-xs font-medium text-fg transition-colors hover:border-primary hover:text-primary'
            : 'inline-flex items-center gap-1.5 rounded-full border border-line-strong px-3.5 py-1.5 text-sm font-medium text-fg transition-colors hover:border-primary hover:text-primary'
        }
      >
        {label}
        <ArrowRight class="h-3.5 w-3.5" />
      </RouterLink>
    )

    const kv = (label: string, value: VNodeChild) => (
      <div class="flex justify-between gap-4 text-sm">
        <span class="text-muted">{label}</span>
        <span class="min-w-0 break-all text-right">{value}</span>
      </div>
    )

    const domainColumns = (): DataTableColumn<AdminResellerDomain>[] => [
      {
        key: 'domain',
        title: t('admin.resellerProfileDetail.domainTable.domain'),
        class: 'font-mono text-xs break-all',
        render: (r) => r.domain,
      },
      {
        key: 'type',
        title: t('admin.resellerProfileDetail.domainTable.type'),
        class: 'text-xs whitespace-nowrap',
        render: (r) => domainTypeLabel(r.type),
      },
      {
        key: 'verification',
        title: t('admin.resellerProfileDetail.domainTable.verification'),
        render: (r) => <Badge tone={verificationTone(r.verification_status)}>{verificationLabel(r.verification_status)}</Badge>,
      },
      {
        key: 'status',
        title: t('admin.resellerProfileDetail.domainTable.status'),
        render: (r) => (
          <Badge tone={domainStatusTone(r.status)} dot>
            {domainStatusLabel(r.status)}
          </Badge>
        ),
      },
      {
        key: 'primary',
        title: t('admin.resellerProfileDetail.domainTable.primary'),
        class: 'text-xs',
        render: (r) => (r.is_primary && isActiveVerifiedDomain(r) ? <Badge tone="primary">{t('admin.common.yes')}</Badge> : t('admin.common.no')),
      },
      {
        key: 'verifiedAt',
        title: t('admin.resellerProfileDetail.domainTable.verifiedAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.verified_at) || '-',
      },
      {
        key: 'action',
        title: t('admin.resellerProfileDetail.domainTable.action'),
        align: 'right',
        render: (r) => (
          <Button size="xs" disabled={d.operatingDomainId.value === r.id || !canSetPrimaryDomain(r)} onClick={() => d.setPrimary(r)}>
            {t('admin.resellerProfileDetail.domainTable.setPrimary')}
          </Button>
        ),
      },
    ]

    const orderColumns = (): DataTableColumn<AdminResellerProfileDetailOrder>[] => [
      {
        key: 'orderNo',
        title: t('admin.resellerProfileDetail.finance.orderTable.orderNo'),
        class: 'font-mono text-xs',
        render: (r) => r.order_no,
      },
      {
        key: 'status',
        title: t('admin.resellerProfileDetail.finance.orderTable.status'),
        render: (r) => <Badge tone={orderStatusTone(r.status)}>{orderStatusLabel(t, r.status)}</Badge>,
      },
      {
        key: 'domain',
        title: t('admin.resellerProfileDetail.finance.orderTable.domain'),
        class: 'font-mono text-xs',
        render: (r) => r.domain || '-',
      },
      {
        key: 'amount',
        title: t('admin.resellerProfileDetail.finance.orderTable.amount'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.total_amount, r.currency),
      },
      {
        key: 'profit',
        title: t('admin.resellerProfileDetail.finance.orderTable.profit'),
        class: 'zs-num text-sm text-success-text',
        render: (r) => formatMoney(r.profit_amount, r.currency),
      },
      {
        key: 'createdAt',
        title: t('admin.resellerProfileDetail.finance.orderTable.createdAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.created_at),
      },
    ]

    const ledgerColumns = (): DataTableColumn<AdminResellerProfileDetailLedger>[] => [
      {
        key: 'type',
        title: t('admin.resellerProfileDetail.finance.ledgerTable.type'),
        class: 'text-xs',
        render: (r) => ledgerTypeLabel(r.type),
      },
      {
        key: 'amount',
        title: t('admin.resellerProfileDetail.finance.ledgerTable.amount'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.amount, r.currency),
      },
      {
        key: 'status',
        title: t('admin.resellerProfileDetail.finance.ledgerTable.status'),
        render: (r) => <Badge tone={ledgerStatusTone(r.status)}>{ledgerStatusLabel(r.status)}</Badge>,
      },
      {
        key: 'time',
        title: t('admin.resellerProfileDetail.finance.ledgerTable.time'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.created_at),
      },
    ]

    const withdrawColumns = (): DataTableColumn<AdminResellerProfileDetailWithdraw>[] => [
      {
        key: 'channel',
        title: t('admin.resellerProfileDetail.finance.withdrawTable.channel'),
        class: 'text-xs',
        render: (r) => r.channel || '-',
      },
      {
        key: 'amount',
        title: t('admin.resellerProfileDetail.finance.withdrawTable.amount'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.amount, r.currency),
      },
      {
        key: 'status',
        title: t('admin.resellerProfileDetail.finance.withdrawTable.status'),
        render: (r) => <Badge tone={withdrawStatusTone(r.status)}>{withdrawStatusLabel(r.status)}</Badge>,
      },
      {
        key: 'time',
        title: t('admin.resellerProfileDetail.finance.withdrawTable.time'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.created_at),
      },
    ]

    const renderBody = () => {
      const detail = d.detail.value
      const profile = d.profile.value
      if (d.loading.value && !detail) return <Loader />
      if (!detail || !profile) return <EmptyState title={t('admin.resellerProfileDetail.notFound')} />
      const ps = detail.product_summary
      const site = detail.site_config
      const tab = currentTab.value
      return (
        <>
          <div class="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
            <Card>
              <p class="text-xs text-muted">{t('admin.resellerProfileDetail.cards.profileStatus')}</p>
              <div class="mt-3 flex flex-wrap items-center gap-2">
                <Badge tone={profileStatusTone(profile.status)} size="md" dot>
                  {statusLabel(profile.status)}
                </Badge>
                <Badge tone={settlementTone(profile.settlement_status)} size="md">
                  {settlementLabel(profile.settlement_status)}
                </Badge>
              </div>
            </Card>
            <Card>
              <p class="text-xs text-muted">{t('admin.resellerProfileDetail.cards.markup')}</p>
              <p class="zs-num mt-2 text-xl font-bold">
                {profile.default_markup_percent || '0.00'}% / {profile.max_markup_percent || '0.00'}%
              </p>
              <p class="mt-1 text-xs text-muted">{t('admin.resellerProfileDetail.cards.markupHint')}</p>
            </Card>
            <Card>
              <p class="text-xs text-muted">{t('admin.resellerProfileDetail.cards.primaryDomain')}</p>
              <p class="mt-3 truncate font-mono text-sm font-medium">{d.primaryDomain.value?.domain || t('admin.resellerProfileDetail.unset')}</p>
              <p class="mt-1 text-xs text-muted">
                {t('admin.resellerProfileDetail.cards.domainCount', {
                  count: d.domains.value.length,
                })}
              </p>
            </Card>
            <Card>
              <p class="text-xs text-muted">{t('admin.resellerProfileDetail.cards.productRules')}</p>
              <p class="zs-num mt-2 text-xl font-bold">{ps.configured_products}</p>
              <p class="mt-1 text-xs text-muted">
                {t('admin.resellerProfileDetail.cards.productRulesHint', {
                  hidden: ps.hidden_products,
                  sku: ps.sku_overrides,
                })}
              </p>
            </Card>
          </div>

          <Tabs
            v-model={currentTab.value}
            items={['overview', 'profile', 'domains', 'site', 'products', 'finance'].map((key) => ({
              key,
              label: t(`admin.resellerProfileDetail.tabs.${key}`),
            }))}
          />

          {tab === 'overview' && (
            <div class="grid gap-4 lg:grid-cols-3">
              <Card title={t('admin.resellerProfileDetail.overview.account')}>
                <div class="space-y-2">
                  {kv(
                    t('admin.resellerProfileDetail.overview.userId'),
                    <a class="font-mono text-accent hover:underline" href={adminUrl(`/users/${profile.user_id}`)} target="_blank" rel="noopener">
                      #{profile.user_id}
                    </a>,
                  )}
                  {kv(t('admin.resellerProfileDetail.overview.email'), profile.user?.email || '-')}
                  {kv(t('admin.resellerProfileDetail.overview.displayName'), profile.user?.display_name || '-')}
                </div>
              </Card>
              <Card title={t('admin.resellerProfileDetail.overview.balanceSummary')}>
                <div class="space-y-2">
                  {detail.finance_summary.balances.map((b) => (
                    <div key={b.id} class="flex justify-between gap-4 text-sm">
                      <span class="font-mono">{b.currency}</span>
                      <span class="zs-num">
                        {b.available_amount} / {b.locked_amount}
                      </span>
                    </div>
                  ))}
                  {detail.finance_summary.balances.length === 0 && <p class="text-sm text-muted">{t('admin.resellerProfileDetail.overview.noBalance')}</p>}
                </div>
              </Card>
              <Card title={t('admin.resellerProfileDetail.overview.recentOps')}>
                <div class="grid grid-cols-3 gap-3 text-center">
                  {(
                    [
                      [detail.recent_orders.length, 'orders'],
                      [detail.recent_ledger_entries.length, 'ledger'],
                      [detail.recent_withdraws.length, 'withdraws'],
                    ] as const
                  ).map(([n, key]) => (
                    <div key={key} class="rounded-zs-sm bg-surface-muted py-2">
                      <p class="zs-num text-lg font-bold">{n}</p>
                      <p class="text-xs text-muted">{t(`admin.resellerProfileDetail.overview.${key}`)}</p>
                    </div>
                  ))}
                </div>
              </Card>
            </div>
          )}

          {tab === 'profile' && (
            <Card>
              <div class="grid gap-5 md:grid-cols-2">
                <div>
                  <p class="text-xs text-muted">{t('admin.resellerProfileDetail.profile.applyReason')}</p>
                  <p class="mt-2 whitespace-pre-wrap text-sm">{profile.apply_reason || '-'}</p>
                </div>
                <div>
                  <p class="text-xs text-muted">{t('admin.resellerProfileDetail.profile.rejectReason')}</p>
                  <p class="mt-2 whitespace-pre-wrap text-sm">{profile.reject_reason || '-'}</p>
                </div>
                <div>
                  <p class="text-xs text-muted">{t('admin.resellerProfileDetail.profile.reviewer')}</p>
                  <p class="mt-2 text-sm">
                    <span class="font-mono">{profile.reviewed_by || '-'}</span> · {formatDate(profile.reviewed_at) || '-'}
                  </p>
                </div>
                <div>
                  <p class="text-xs text-muted">{t('admin.resellerProfileDetail.profile.createdUpdated')}</p>
                  <p class="mt-2 text-sm">
                    {formatDate(profile.created_at)} · {formatDate(profile.updated_at)}
                  </p>
                </div>
              </div>
            </Card>
          )}

          {tab === 'domains' && (
            <div class="space-y-4">
              <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
                <Card>
                  <div class="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                    <div>
                      <p class="text-sm font-semibold">{t('admin.resellerProfileDetail.systemDomain.title')}</p>
                      <p class="mt-1 max-w-2xl text-sm text-muted">{t('admin.resellerProfileDetail.systemDomain.description')}</p>
                    </div>
                    <Badge tone={d.systemDomain.value ? 'success' : 'warning'} size="md">
                      {d.systemDomain.value ? t('admin.resellerProfileDetail.systemDomain.assigned') : t('admin.resellerProfileDetail.systemDomain.unassigned')}
                    </Badge>
                  </div>
                  <form
                    class="mt-4 grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-end"
                    onSubmit={(e: Event) => {
                      e.preventDefault()
                      void d.submitSystemDomain()
                    }}
                  >
                    <FormField label={t('admin.resellerProfileDetail.systemDomain.inputLabel')}>
                      <Input
                        v-model={d.systemDomainForm.subdomain}
                        placeholder={t('admin.resellerProfileDetail.systemDomain.inputPlaceholder')}
                        disabled={d.savingSystemDomain.value}
                        mono
                      />
                    </FormField>
                    <Button type="submit" variant="primary" loading={d.savingSystemDomain.value} disabled={!d.systemDomainForm.subdomain.trim()}>
                      {d.savingSystemDomain.value ? t('admin.resellerProfileDetail.systemDomain.saving') : t('admin.resellerProfileDetail.systemDomain.save')}
                    </Button>
                  </form>
                  <p class="mt-3 text-xs leading-relaxed text-muted">{t('admin.resellerProfileDetail.systemDomain.note')}</p>
                </Card>
                <Card>
                  <p class="text-xs text-muted">{t('admin.resellerProfileDetail.systemDomain.current')}</p>
                  <p class="mt-2 break-all font-mono text-sm font-medium">
                    {d.systemDomain.value?.domain || t('admin.resellerProfileDetail.systemDomain.unassigned')}
                  </p>
                  <p class="mt-4 text-xs text-muted">{t('admin.resellerProfileDetail.systemDomain.currentPrimary')}</p>
                  <p class="mt-2 break-all font-mono text-sm font-medium">{d.primaryDomain.value?.domain || t('admin.resellerProfileDetail.unset')}</p>
                  <p class="mt-3 text-xs text-muted">
                    {t('admin.resellerProfileDetail.systemDomain.aside', {
                      count: d.domains.value.length,
                    })}
                  </p>
                </Card>
              </div>
              <DataTable
                columns={domainColumns()}
                rows={d.domains.value}
                rowKey={(r) => r.id}
                emptyText={t('admin.resellerProfileDetail.domainTable.empty')}
                minWidth="920px"
              />
            </div>
          )}

          {tab === 'site' && (
            <Card>
              <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div class="flex items-center gap-3">
                  {site?.logo ? (
                    <img src={getImageUrl(site.logo)} class="h-12 w-12 shrink-0 rounded-zs-sm border border-line bg-surface-muted object-contain" alt="Logo" />
                  ) : (
                    <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-zs-sm border border-dashed border-line bg-surface-muted text-[10px] text-muted">
                      {t('admin.resellerProfileDetail.site.noLogo')}
                    </div>
                  )}
                  <div>
                    <p class="text-sm font-semibold">{site?.site_name || t('admin.resellerProfileDetail.site.noSiteName')}</p>
                    <p class="mt-1 text-xs text-muted">{t('admin.resellerProfileDetail.site.brandInfo')}</p>
                  </div>
                </div>
                {linkButton(link('/resellers/site-configs'), t('admin.resellerProfileDetail.site.openConfig'))}
              </div>
              <div class="mt-4 grid gap-3 md:grid-cols-3">
                {(
                  [
                    ['announcement', getLocalizedText(site?.announcement?.title)],
                    ['seoTitle', getLocalizedText(site?.seo?.title)],
                    ['supportEmail', site?.support?.email || ''],
                  ] as const
                ).map(([key, value]) => (
                  <div key={key} class="rounded-zs-sm border border-line bg-surface-muted/50 p-3">
                    <p class="text-xs text-muted">{t(`admin.resellerProfileDetail.site.${key}`)}</p>
                    <p class="mt-2 text-sm">{value || '-'}</p>
                  </div>
                ))}
              </div>
            </Card>
          )}

          {tab === 'products' && (
            <Card>
              <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div>
                  <p class="text-sm font-semibold">{t('admin.resellerProfileDetail.products.summaryTitle')}</p>
                  <p class="mt-1 text-sm text-muted">
                    {t('admin.resellerProfileDetail.products.summary', {
                      configured: ps.configured_products,
                      hidden: ps.hidden_products,
                      sku: ps.sku_overrides,
                      pricing: ps.pricing_overrides,
                    })}
                  </p>
                </div>
                {linkButton(link('/resellers/product-settings'), t('admin.resellerProfileDetail.products.openRules'))}
              </div>
            </Card>
          )}

          {tab === 'finance' && (
            <div class="space-y-4">
              <Card title={t('admin.resellerProfileDetail.finance.recentOrders')} padded={false}>
                <div class="pt-3">
                  <DataTable
                    bare
                    columns={orderColumns()}
                    rows={detail.recent_orders}
                    rowKey={(r) => r.order_no}
                    emptyText={t('admin.resellerProfileDetail.finance.noOrders')}
                    minWidth="900px"
                  />
                </div>
              </Card>
              <div class="grid gap-4 xl:grid-cols-2 [&>*]:min-w-0">
                <Card title={t('admin.resellerProfileDetail.finance.recentLedger')} padded={false}>
                  {{
                    extra: () => linkButton(link('/resellers/ledger-entries'), t('admin.resellerProfileDetail.finance.viewAll'), 'xs'),
                    default: () => (
                      <div class="pt-3">
                        <DataTable
                          bare
                          columns={ledgerColumns()}
                          rows={detail.recent_ledger_entries}
                          rowKey={(r) => r.id}
                          emptyText={t('admin.resellerProfileDetail.finance.noLedger')}
                          minWidth="560px"
                        />
                      </div>
                    ),
                  }}
                </Card>
                <Card title={t('admin.resellerProfileDetail.finance.recentWithdraws')} padded={false}>
                  {{
                    extra: () => linkButton(link('/resellers/withdraws'), t('admin.resellerProfileDetail.finance.viewAll'), 'xs'),
                    default: () => (
                      <div class="pt-3">
                        <DataTable
                          bare
                          columns={withdrawColumns()}
                          rows={detail.recent_withdraws}
                          rowKey={(r) => r.id}
                          emptyText={t('admin.resellerProfileDetail.finance.noWithdraws')}
                          minWidth="560px"
                        />
                      </div>
                    ),
                  }}
                </Card>
              </div>
            </div>
          )}
        </>
      )
    }

    return () => {
      const profile = d.profile.value
      return (
        <div class="space-y-6">
          <div class="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="ghost" onClick={() => router.back()}>
              <ArrowLeft class="h-4 w-4" />
              {t('admin.resellerProfileDetail.back')}
            </Button>
            <span class="font-mono text-xs text-muted">R#{profileId.value}</span>
          </div>
          <PageHeader
            title={t('admin.resellerProfileDetail.title')}
            subtitle={profile?.user?.display_name || profile?.user?.email || (d.loading.value ? t('admin.common.loading') : '-')}
          >
            {{
              actions: () => (
                <>
                  <RefreshButton loading={d.loading.value && !!d.detail.value} onClick={d.fetchDetail} />
                  <Button size="sm" variant="primary" disabled={!profile} onClick={() => (d.showEditDialog.value = true)}>
                    <Pencil class="h-3.5 w-3.5" />
                    {t('admin.resellerProfileDetail.editOperations')}
                  </Button>
                </>
              ),
            }}
          </PageHeader>
          {renderBody()}
          <ProfileEditDialog v-model={d.showEditDialog.value} profile={profile} saving={d.saving.value} variant="detail" onSubmit={d.submitEdit} />
        </div>
      )
    }
  },
})
