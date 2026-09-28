import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import { toRowSelection } from '@/composables/useSelection'
import { formatDate } from '@/utils/format'
import { adminUrl } from '@/utils/adminBase'
import {
  AFFILIATE_PROFILE_STATUS_ACTIVE,
  AFFILIATE_PROFILE_STATUS_DISABLED,
  conversionRateText,
  pickStatAmount,
  pickStatNumber,
  resolveProfileID,
  resolveProfileStatus,
  resolveUserID,
  type AffiliateUserRow,
} from './affiliateUtils'
import { useAffiliateUsers } from './useAffiliateUsers'

export default defineComponent({
  name: 'AffiliateUsersView',
  setup() {
    const { t } = useI18n()
    const p = useAffiliateUsers()
    onMounted(() => void p.list.fetchData(1))

    const statusLabel = (s: string) =>
      s === AFFILIATE_PROFILE_STATUS_ACTIVE ? t('admin.affiliatesUsers.status.active') : s === AFFILIATE_PROFILE_STATUS_DISABLED ? t('admin.affiliatesUsers.status.disabled') : s || '-'
    const statusTone = (s: string) => (s === AFFILIATE_PROFILE_STATUS_ACTIVE ? 'success' : s === AFFILIATE_PROFILE_STATUS_DISABLED ? 'neutral' : 'secondary')

    const num = 'zs-num font-mono text-xs text-fg'
    const columns = (): DataTableColumn<AffiliateUserRow>[] => [
      { key: 'id', title: t('admin.affiliatesUsers.table.id'), render: (r) => <IdCell value={resolveProfileID(r)} /> },
      {
        key: 'user',
        title: t('admin.affiliatesUsers.table.user'),
        class: 'min-w-[160px] text-xs text-muted',
        render: (r) => {
          const uid = resolveUserID(r)
          return (
            <div>
              {uid > 0 ? (
                <a href={adminUrl(`/users/${uid}`)} target="_blank" rel="noopener" class="font-mono text-primary underline-offset-4 hover:underline">
                  #{uid}
                </a>
              ) : (
                <span class="text-fg">-</span>
              )}
              {r.profile?.user?.display_name && <div class="mt-0.5 break-words text-fg">{r.profile.user.display_name}</div>}
              {r.profile?.user?.email && <div class="mt-0.5 break-all">{r.profile.user.email}</div>}
            </div>
          )
        },
      },
      {
        key: 'code',
        title: t('admin.affiliatesUsers.table.code'),
        render: (r) => (
          <span class="inline-block whitespace-nowrap rounded-zs-sm border border-line bg-surface-muted px-2 py-1 font-mono text-xs text-fg">{r.profile?.code || r.profile?.affiliate_code || '-'}</span>
        ),
      },
      { key: 'clicks', title: t('admin.affiliatesUsers.table.clicks'), class: num, render: (r) => pickStatNumber(r.stats, 'ClickCount', 'click_count') },
      { key: 'validOrders', title: t('admin.affiliatesUsers.table.validOrders'), class: num, render: (r) => pickStatNumber(r.stats, 'ValidOrderCount', 'valid_order_count') },
      { key: 'conversionRate', title: t('admin.affiliatesUsers.table.conversionRate'), class: num, render: (r) => conversionRateText(r.stats) },
      { key: 'pending', title: t('admin.affiliatesUsers.table.pending'), class: num, render: (r) => pickStatAmount(r.stats, 'PendingCommission', 'pending_commission') },
      { key: 'available', title: t('admin.affiliatesUsers.table.available'), class: num, render: (r) => pickStatAmount(r.stats, 'AvailableCommission', 'available_commission') },
      { key: 'withdrawn', title: t('admin.affiliatesUsers.table.withdrawn'), class: num, render: (r) => pickStatAmount(r.stats, 'WithdrawnCommission', 'withdrawn_commission') },
      {
        key: 'status',
        title: t('admin.affiliatesUsers.table.status'),
        render: (r) => {
          const s = resolveProfileStatus(r)
          return (
            <Badge tone={statusTone(s)} dot>
              {statusLabel(s)}
            </Badge>
          )
        },
      },
      { key: 'createdAt', title: t('admin.affiliatesUsers.table.createdAt'), class: 'whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.profile?.created_at || r.created_at) },
      {
        key: 'action',
        title: t('admin.affiliatesUsers.table.action'),
        align: 'right',
        render: (r) => {
          const id = resolveProfileID(r)
          const active = resolveProfileStatus(r) === AFFILIATE_PROFILE_STATUS_ACTIVE
          return (
            <Button size="sm" variant={active ? 'outline' : 'soft'} disabled={id <= 0 || p.operatingId.value === id} onClick={() => p.toggleStatus(r)}>
              {active ? t('admin.affiliatesUsers.actions.disable') : t('admin.affiliatesUsers.actions.enable')}
            </Button>
          )
        },
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.affiliatesUsers.title')} />
        <FilterBar cols={3}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={p.filters.keyword} placeholder={t('admin.affiliatesUsers.filters.keyword')} onEnter={p.list.handleSearch} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.code} placeholder={t('admin.affiliatesUsers.filters.code')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Select
                  v-model={p.filters.status}
                  onChange={p.list.handleSearch}
                  options={[
                    { label: t('admin.affiliatesUsers.filters.statusAll'), value: '__all__' },
                    { label: t('admin.affiliatesUsers.status.active'), value: AFFILIATE_PROFILE_STATUS_ACTIVE },
                    { label: t('admin.affiliatesUsers.status.disabled'), value: AFFILIATE_PROFILE_STATUS_DISABLED },
                  ]}
                />
              </>
            ),
            actions: () => (
              <>
                {p.selection.selectedIds.value.length > 0 && (
                  <>
                    <Button size="sm" variant="soft" onClick={() => p.batchUpdateStatus(AFFILIATE_PROFILE_STATUS_ACTIVE)}>
                      {t('admin.affiliatesUsers.batch.enable')}
                    </Button>
                    <Button size="sm" variant="danger" onClick={() => p.batchUpdateStatus(AFFILIATE_PROFILE_STATUS_DISABLED)}>
                      {t('admin.affiliatesUsers.batch.disable')}
                    </Button>
                  </>
                )}
                <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {t('admin.common.refresh')}
                </Button>
              </>
            ),
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => resolveProfileID(r)}
            loading={p.list.loading.value}
            selection={toRowSelection(p.selection)}
            emptyText={t('admin.affiliatesUsers.empty')}
            minWidth="1100px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>
      </div>
    )
  },
})
