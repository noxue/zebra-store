import { computed, defineComponent, onMounted, watch, type VNodeChild } from 'vue'
import { RouterLink, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { BadgeAlert, BadgeCheck, ChevronLeft, Copy, CreditCard, Receipt, ShieldCheck, Ticket, Wallet } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, FormField, IdCell, Input, ListPagination, PageHeader, Select, Tabs, type DataTableColumn } from '@/components/ui'
import type { AdminOrder, AdminPayment, AdminUserOAuthIdentity, AdminWalletTransaction } from '@/api/types'
import type { ListPage } from '@/composables/useListPage'
import { formatDate, formatMoney, getLocalizedText } from '@/utils/format'
import { copyText } from '@/utils/clipboard'
import { orderStatusLabel, orderStatusTone, paymentStatusLabel, paymentStatusTone, userStatusLabel, userStatusTone } from '@/utils/status'
import { formatOAuthIdentityAccount, formatOAuthIdentityUsername, formatOAuthProviderLabel, managedOAuthProvider } from '@/utils/oauthIdentity'
import { LinkButton } from './components/LinkButton'
import { formatLocale, formatScopeProducts } from './usersUtils'
import { useUserDetail, type UserDetailCouponUsage, type UserDetailTab } from './useUserDetail'

const linkClass = 'break-all text-primary underline-offset-4 hover:underline'

const CopyButton = (props: { value?: string; title: string }) =>
  props.value ? (
    <button
      type="button"
      title={props.title}
      class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-zs-sm border border-line text-muted transition-colors hover:border-primary hover:text-primary"
      onClick={() => void copyText(props.value || '').catch(() => undefined)}
    >
      <Copy class="h-3 w-3" />
    </button>
  ) : null

const InfoTile = (props: { label: string; class?: string }, { slots }: { slots: { default?: () => VNodeChild } }) => (
  <div class={['rounded-zs border border-line bg-surface px-4 py-3', props.class]}>
    <div class="text-xs text-muted">{props.label}</div>
    <div class="mt-1 text-sm text-fg">{slots.default?.()}</div>
  </div>
)

export default defineComponent({
  name: 'UserDetailView',
  setup() {
    const { t, te } = useI18n()
    const route = useRoute()
    const userId = computed(() => Number(route.params.id))
    const p = useUserDetail(userId)

    onMounted(() => void p.init())
    watch(
      () => route.params.id,
      () => p.reloadForUser(),
    )

    const copyTitle = () => t('admin.common.copy')
    const tabs = () => [
      { key: 'orders', label: t('admin.userDetail.tabs.orders'), icon: Receipt },
      { key: 'payments', label: t('admin.userDetail.tabs.payments'), icon: CreditCard },
      { key: 'coupons', label: t('admin.userDetail.tabs.coupons'), icon: Ticket },
      { key: 'wallet', label: t('admin.userDetail.tabs.wallet'), icon: Wallet },
    ]

    const memberLevelOptions = () => [
      { label: '-', value: 0 },
      ...p.memberLevels.value.map((l) => ({
        label: `${l.icon && !l.icon.includes('/') ? `${l.icon} ` : ''}${getLocalizedText(l.name)}${l.is_default ? ` (${t('admin.memberLevels.default')})` : ''}`,
        value: l.id,
      })),
    ]

    const walletTypeLabel = (type?: string) => {
      const key = `admin.userDetail.wallet.types.${type || ''}`
      return type && te(key) ? t(key) : type || '-'
    }
    const walletDirection = (direction?: string) => {
      if (direction === 'in') return { tone: 'success' as const, label: t('admin.userDetail.wallet.directionIn') }
      if (direction === 'out') return { tone: 'danger' as const, label: t('admin.userDetail.wallet.directionOut') }
      return { tone: 'warning' as const, label: direction || '-' }
    }
    const couponTypeLabel = (raw?: string) => {
      if (!raw) return '-'
      if (raw === 'percent') return t('admin.common.discountTypes.percent')
      if (raw === 'fixed') return t('admin.common.discountTypes.fixed')
      return raw
    }

    // ---------- columns ----------
    const orderColumns = (): DataTableColumn<AdminOrder>[] => [
      { key: 'id', title: t('admin.userDetail.orders.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'orderNo',
        title: t('admin.userDetail.orders.orderNo'),
        class: 'min-w-[220px] font-mono text-xs',
        render: (r) => (
          <div class="flex items-center gap-1.5">
            <RouterLink to={`/orders?order_id=${r.id}`} class={linkClass}>
              {r.order_no}
            </RouterLink>
            <CopyButton value={r.order_no} title={copyTitle()} />
          </div>
        ),
      },
      { key: 'status', title: t('admin.userDetail.orders.status'), render: (r) => <Badge tone={orderStatusTone(r.status)}>{orderStatusLabel(t, r.status)}</Badge> },
      { key: 'amount', title: t('admin.userDetail.orders.amount'), class: 'zs-num', render: (r) => formatMoney(r.total_amount, r.currency) },
      { key: 'createdAt', title: t('admin.userDetail.orders.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
    ]

    const paymentColumns = (): DataTableColumn<AdminPayment>[] => [
      { key: 'id', title: t('admin.userDetail.payments.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'order',
        title: t('admin.userDetail.payments.orderId'),
        class: 'min-w-[220px] font-mono text-xs',
        render: (r) => (
          <div>
            {r.order_id ? (
              <div class="flex items-center gap-1.5">
                <RouterLink to={`/orders?order_id=${r.order_id}`} class={linkClass}>
                  {r.order_no || `#${r.order_id}`}
                </RouterLink>
                <CopyButton value={r.order_no} title={copyTitle()} />
              </div>
            ) : r.recharge_no ? (
              <div class="flex items-center gap-1.5">
                <RouterLink to={`/payments?payment_id=${r.id}`} class={linkClass}>
                  {r.recharge_no}
                </RouterLink>
                <CopyButton value={r.recharge_no} title={copyTitle()} />
              </div>
            ) : (
              <span>-</span>
            )}
            {r.recharge_no && (
              <div class="mt-1 font-sans text-xs text-muted">
                {t('admin.payments.rechargeStatus')}: {r.recharge_status ? paymentStatusLabel(t, r.recharge_status) : '-'}
              </div>
            )}
          </div>
        ),
      },
      { key: 'status', title: t('admin.userDetail.payments.status'), render: (r) => <Badge tone={paymentStatusTone(r.status)}>{paymentStatusLabel(t, r.status)}</Badge> },
      { key: 'amount', title: t('admin.userDetail.payments.amount'), class: 'zs-num', render: (r) => formatMoney(r.amount, r.currency) },
      { key: 'createdAt', title: t('admin.userDetail.payments.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
    ]

    const couponColumns = (): DataTableColumn<UserDetailCouponUsage>[] => [
      { key: 'id', title: t('admin.userDetail.coupons.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'coupon',
        title: t('admin.userDetail.coupons.coupon'),
        class: 'min-w-[180px]',
        render: (r) => (
          <div>
            <div class="font-mono">{r.coupon_code || '-'}</div>
            <div class="text-xs text-muted">#{r.coupon_id}</div>
          </div>
        ),
      },
      { key: 'type', title: t('admin.userDetail.coupons.type'), class: 'text-xs text-muted', render: (r) => couponTypeLabel(r.coupon_type) },
      { key: 'products', title: t('admin.userDetail.coupons.products'), class: 'min-w-[220px] text-xs text-muted break-words', render: (r) => formatScopeProducts(r.scope_products) },
      {
        key: 'orderId',
        title: t('admin.userDetail.coupons.orderId'),
        class: 'font-mono',
        render: (r) =>
          r.order_id ? (
            <RouterLink to={`/orders?order_id=${r.order_id}`} class={linkClass}>
              #{r.order_id}
            </RouterLink>
          ) : (
            '-'
          ),
      },
      { key: 'discount', title: t('admin.userDetail.coupons.discount'), class: 'zs-num', render: (r) => formatMoney(r.discount_amount) },
      { key: 'createdAt', title: t('admin.userDetail.coupons.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
    ]

    const walletColumns = (): DataTableColumn<AdminWalletTransaction>[] => [
      { key: 'id', title: t('admin.userDetail.wallet.table.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'type', title: t('admin.userDetail.wallet.table.type'), class: 'text-xs', render: (r) => walletTypeLabel(r.type) },
      {
        key: 'direction',
        title: t('admin.userDetail.wallet.table.direction'),
        render: (r) => {
          const d = walletDirection(r.direction)
          return <Badge tone={d.tone}>{d.label}</Badge>
        },
      },
      { key: 'amount', title: t('admin.userDetail.wallet.table.amount'), class: 'zs-num text-xs', render: (r) => formatMoney(r.amount, r.currency) },
      { key: 'balanceAfter', title: t('admin.userDetail.wallet.table.balanceAfter'), class: 'zs-num text-xs', render: (r) => formatMoney(r.balance_after, r.currency) },
      { key: 'remark', title: t('admin.userDetail.wallet.table.remark'), class: 'min-w-[220px] text-xs text-muted break-words', render: (r) => r.remark || '-' },
      { key: 'createdAt', title: t('admin.userDetail.wallet.table.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
    ]

    const listBlock = <T,>(list: ListPage<T>, columns: DataTableColumn<T>[], rowKey: (r: T) => number, minWidth: string) => (
      <div>
        <DataTable columns={columns} rows={list.items.value} rowKey={rowKey} loading={list.loading.value} emptyText={t('admin.userDetail.empty')} minWidth={minWidth} />
        <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
      </div>
    )

    // ---------- sections ----------
    const profile = () => {
      const u = p.user.value
      const verified = Boolean(u?.email_verified_at)
      return (
        <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
          <InfoTile label={t('admin.userDetail.fields.id')}>{u?.id ? <IdCell value={u.id} /> : '-'}</InfoTile>
          <InfoTile label={t('admin.userDetail.fields.email')}>
            <div class="flex flex-wrap items-center gap-2">
              <span class="break-all">{u?.email || '-'}</span>
              <Badge tone={verified ? 'success' : 'warning'}>
                {verified ? <BadgeCheck class="h-3.5 w-3.5" /> : <BadgeAlert class="h-3.5 w-3.5" />}
                {verified ? t('admin.userDetail.emailVerification.verified') : t('admin.userDetail.emailVerification.unverified')}
              </Badge>
            </div>
          </InfoTile>
          <InfoTile label={t('admin.userDetail.fields.nickname')}>
            <span class="break-words">{u?.display_name || '-'}</span>
          </InfoTile>
          <InfoTile label={t('admin.userDetail.fields.status')}>
            <Badge tone={userStatusTone(u?.status)} dot>
              {userStatusLabel(t, u?.status)}
            </Badge>
          </InfoTile>
          <InfoTile label={t('admin.userDetail.fields.locale')}>{formatLocale(t, u?.locale)}</InfoTile>
          <InfoTile label={t('admin.userDetail.fields.createdAt')}>{formatDate(u?.created_at) || '-'}</InfoTile>
          <InfoTile label={t('admin.userDetail.fields.lastLoginAt')}>{formatDate(u?.last_login_at) || '-'}</InfoTile>
          <InfoTile label={t('admin.userDetail.fields.walletBalance')}>
            <span class="zs-num text-base font-semibold text-primary">{formatMoney(u?.wallet_balance, p.siteCurrency.value)}</span>
          </InfoTile>
          <InfoTile label={t('admin.userDetail.fields.memberLevel')}>
            <Select
              size="sm"
              modelValue={Number(u?.member_level_id || 0)}
              options={memberLevelOptions()}
              disabled={p.memberLevelUpdating.value || !u}
              onUpdate:modelValue={(v) => void p.changeMemberLevel(v)}
            />
          </InfoTile>
          <InfoTile label={t('admin.userDetail.fields.adminNote')} class="md:col-span-3">
            <span class="whitespace-pre-wrap">{u?.admin_note || '-'}</span>
          </InfoTile>
          <div class="space-y-2 rounded-zs border border-line bg-surface px-4 py-3 md:col-span-3">
            <div class="flex flex-wrap items-center gap-3">
              <span class="inline-flex items-center gap-1.5 text-xs text-muted">
                <ShieldCheck class="h-3.5 w-3.5" />
                {t('admin.userDetail.fields.twofa')}
              </span>
              <Badge tone={p.twofaEnabled.value ? 'success' : 'warning'}>
                {p.twofaEnabled.value ? t('admin.userDetail.twofa.enabled') : t('admin.userDetail.twofa.disabled')}
              </Badge>
              {p.twofaEnabled.value && (
                <span class="text-xs text-muted">
                  {t('admin.userDetail.twofa.enabledAt')}: {formatDate(p.twofaEnabledAt.value)}
                </span>
              )}
              {p.twofaEnabled.value && (
                <Button size="sm" variant="danger" loading={p.twofaResetting.value} onClick={p.resetUser2FA}>
                  {p.twofaResetting.value ? t('admin.userDetail.twofa.resetting') : t('admin.userDetail.twofa.reset')}
                </Button>
              )}
            </div>
            <p class="text-xs text-muted">{t('admin.userDetail.twofa.hint')}</p>
            {p.twofaError.value && <p class="text-xs text-danger-text">{p.twofaError.value}</p>}
            {p.twofaSuccess.value && <p class="text-xs text-success-text">{p.twofaSuccess.value}</p>}
          </div>
        </div>
      )
    }

    const identityCard = (identity: AdminUserOAuthIdentity) => {
      const provider = managedOAuthProvider(identity)
      const isGoogle = provider === 'google'
      const busy = p.oauthUnbindingId.value === identity.id
      return (
        <div key={identity.id} class="rounded-zs border border-line bg-surface px-4 py-3">
          <div class="flex items-start justify-between gap-3">
            <div class="flex min-w-0 items-center gap-3">
              {identity.avatar_url && (
                <img src={identity.avatar_url} alt={identity.username || identity.provider_user_id} class="h-10 w-10 rounded-full border border-line object-cover" />
              )}
              <div class="min-w-0">
                <div class="text-sm font-medium text-fg">{formatOAuthProviderLabel(identity.provider)}</div>
                <div class="truncate text-xs text-muted">{formatOAuthIdentityAccount(identity)}</div>
              </div>
            </div>
            {provider && (
              <Button size="sm" variant="danger" disabled={busy} onClick={() => void p.unbindOAuthIdentity(identity)}>
                {busy
                  ? t(isGoogle ? 'admin.userDetail.oauth.unbindingGoogle' : 'admin.userDetail.oauth.unbindingTelegram')
                  : t(isGoogle ? 'admin.userDetail.oauth.unbindGoogle' : 'admin.userDetail.oauth.unbindTelegram')}
              </Button>
            )}
          </div>
          <div class="mt-3 space-y-1 text-xs text-muted">
            <div>
              {t('admin.userDetail.oauth.providerUserId')}: <span class="font-mono text-fg">{identity.provider_user_id || '-'}</span>
            </div>
            <div>
              {t(isGoogle ? 'admin.userDetail.oauth.email' : 'admin.userDetail.oauth.username')}: <span class="text-fg">{formatOAuthIdentityUsername(identity)}</span>
            </div>
            <div>
              {t('admin.userDetail.oauth.boundAt')}: <span class="text-fg">{formatDate(identity.created_at)}</span>
            </div>
          </div>
        </div>
      )
    }

    const walletTab = () => (
      <div class="space-y-4">
        <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
          <InfoTile label={t('admin.userDetail.wallet.balanceLabel')}>
            <span class="zs-num text-xl font-bold text-primary">{formatMoney(p.walletAccount.value?.balance, p.siteCurrency.value)}</span>
          </InfoTile>
          <InfoTile label={t('admin.userDetail.wallet.updatedAtLabel')}>{formatDate(p.walletAccount.value?.updated_at) || '-'}</InfoTile>
        </div>
        <Card title={t('admin.userDetail.wallet.adjustTitle')}>
          <form
            class="grid grid-cols-1 items-end gap-3 md:grid-cols-[180px_1fr_2fr_auto]"
            onSubmit={(e: Event) => {
              e.preventDefault()
              void p.submitWalletAdjust()
            }}
          >
            <FormField label={t('admin.userDetail.wallet.operationLabel')}>
              <Select
                v-model={p.walletForm.operation}
                options={[
                  { label: t('admin.userDetail.wallet.operations.add'), value: 'add' },
                  { label: t('admin.userDetail.wallet.operations.subtract'), value: 'subtract' },
                ]}
              />
            </FormField>
            <FormField label={t('admin.userDetail.wallet.amountLabel')}>
              <Input v-model={p.walletForm.amount} mono placeholder={t('admin.userDetail.wallet.amountPlaceholder')} />
            </FormField>
            <FormField label={t('admin.userDetail.wallet.remarkLabel')} required>
              <Input v-model={p.walletForm.remark} placeholder={t('admin.userDetail.wallet.remarkPlaceholder')} />
            </FormField>
            <Button type="submit" variant="primary" loading={p.walletSubmitting.value}>
              {p.walletSubmitting.value ? t('admin.userDetail.wallet.adjusting') : t('admin.userDetail.wallet.adjustSubmit')}
            </Button>
          </form>
          {p.walletError.value && <p class="mt-3 rounded-zs-sm bg-danger-soft px-3 py-2 text-sm text-danger-text">{p.walletError.value}</p>}
          {p.walletSuccess.value && <p class="mt-3 rounded-zs-sm bg-success-soft px-3 py-2 text-sm text-success-text">{p.walletSuccess.value}</p>}
        </Card>
        {listBlock(p.walletTx, walletColumns(), (r) => r.id, '920px')}
      </div>
    )

    const tabContent = () => {
      switch (p.activeTab.value) {
        case 'orders':
          return listBlock(p.orders, orderColumns(), (r) => r.id, '720px')
        case 'payments':
          return listBlock(p.payments, paymentColumns(), (r) => r.id, '760px')
        case 'coupons':
          return listBlock(p.coupons, couponColumns(), (r) => r.id, '900px')
        default:
          return walletTab()
      }
    }

    return () => (
      <div class="space-y-6">
        <div class="flex flex-wrap items-end justify-between gap-4">
          <div class="flex items-center gap-3">
            <RouterLink
              to="/users"
              class="inline-flex h-9 w-9 items-center justify-center rounded-zs border border-line bg-surface text-muted transition-colors hover:border-primary hover:text-primary"
              aria-label={t('admin.userDetail.back')}
              title={t('admin.userDetail.back')}
            >
              <ChevronLeft class="h-5 w-5" />
            </RouterLink>
            <PageHeader title={t('admin.userDetail.title')} />
          </div>
          <div class="flex flex-wrap gap-2">
            <LinkButton to={`/orders?user_id=${userId.value}`}>
              <Receipt class="h-3.5 w-3.5" />
              {t('admin.userDetail.actions.orders')}
            </LinkButton>
            <LinkButton to={`/payments?user_id=${userId.value}`}>
              <CreditCard class="h-3.5 w-3.5" />
              {t('admin.userDetail.actions.payments')}
            </LinkButton>
          </div>
        </div>

        {p.userError.value ? <div class="rounded-zs bg-danger-soft px-4 py-3 text-sm text-danger-text">{p.userError.value}</div> : profile()}

        <Card title={t('admin.userDetail.oauth.title')} description={t('admin.userDetail.oauth.subtitle')}>
          {p.oauthError.value && <p class="mb-3 text-xs text-danger-text">{p.oauthError.value}</p>}
          {p.oauthSuccess.value && <p class="mb-3 text-xs text-success-text">{p.oauthSuccess.value}</p>}
          {p.oauthIdentities.value.length === 0 ? (
            <p class="text-sm text-muted">{t('admin.userDetail.oauth.empty')}</p>
          ) : (
            <div class="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">{p.oauthIdentities.value.map(identityCard)}</div>
          )}
        </Card>

        <Tabs modelValue={p.activeTab.value} items={tabs()} onChange={(k: string) => p.changeTab(k as UserDetailTab)} />
        {tabContent()}
      </div>
    )
  },
})
