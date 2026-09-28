import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminResellerDomain } from '@/api/types'
import { formatDate } from '@/utils/format'
import { getResellerDomainActionState } from '@/utils/resellerManagement'
import { CreatedRange } from './components/CreatedRange'
import { RefreshButton } from './components/RefreshButton'
import { ResellerCell } from './components/ResellerCell'
import { RESELLER_DOMAIN_STATUSES, RESELLER_DOMAIN_TYPES, RESELLER_DOMAIN_VERIFICATIONS, domainStatusTone, verificationTone } from './resellerUtils'
import { useResellerDomains } from './useResellerDomains'

const STATUS_KEY: Record<string, string> = {
  pending_review: 'pendingReview',
  active: 'active',
  disabled: 'disabled',
}

export default defineComponent({
  name: 'ResellerDomainsView',
  setup() {
    const { t } = useI18n()
    const { filters, list, refreshing, refresh, operatingId, approve, disable } = useResellerDomains()
    onMounted(() => void list.fetchData(1))

    const typeLabel = (v?: string) => (v && (RESELLER_DOMAIN_TYPES as readonly string[]).includes(v) ? t(`admin.resellerDomains.type.${v}`) : v || '-')
    const statusLabel = (v?: string) => (v && STATUS_KEY[v] ? t(`admin.resellerDomains.status.${STATUS_KEY[v]}`) : v || '-')
    const verificationLabel = (v?: string) =>
      v && (RESELLER_DOMAIN_VERIFICATIONS as readonly string[]).includes(v) ? t(`admin.resellerDomains.verification.${v}`) : v || '-'

    const columns = (): DataTableColumn<AdminResellerDomain>[] => [
      {
        key: 'id',
        title: t('admin.resellerDomains.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'reseller',
        title: t('admin.resellerDomains.table.reseller'),
        render: (r) => <ResellerCell profile={r.profile} resellerId={r.reseller_id} />,
      },
      {
        key: 'domain',
        title: t('admin.resellerDomains.table.domain'),
        class: 'min-w-[200px] break-all font-mono text-xs',
        render: (r) => r.domain,
      },
      {
        key: 'type',
        title: t('admin.resellerDomains.table.type'),
        class: 'text-xs whitespace-nowrap',
        render: (r) => typeLabel(r.type),
      },
      {
        key: 'verification',
        title: t('admin.resellerDomains.table.verificationStatus'),
        render: (r) => <Badge tone={verificationTone(r.verification_status)}>{verificationLabel(r.verification_status)}</Badge>,
      },
      {
        key: 'status',
        title: t('admin.resellerDomains.table.status'),
        render: (r) => (
          <Badge tone={domainStatusTone(r.status)} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      {
        key: 'primary',
        title: t('admin.resellerDomains.table.primary'),
        class: 'text-xs text-muted',
        render: (r) => (r.is_primary ? t('admin.common.yes') : t('admin.common.no')),
      },
      {
        key: 'verifiedAt',
        title: t('admin.resellerDomains.table.verifiedAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.verified_at) || '-',
      },
      {
        key: 'createdAt',
        title: t('admin.resellerDomains.table.createdAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.created_at),
      },
      {
        key: 'action',
        title: t('admin.resellerDomains.table.action'),
        align: 'right',
        render: (r) => {
          const state = getResellerDomainActionState(r.status)
          const busy = operatingId.value === r.id
          return (
            <div class="flex flex-wrap items-center justify-end gap-2">
              <Button size="xs" variant="primary" disabled={busy || !state.canApprove} onClick={() => approve(r)}>
                {t('admin.resellerDomains.actions.approve')}
              </Button>
              <Button size="xs" disabled={busy || !state.canDisable} onClick={() => disable(r)}>
                {t('admin.resellerDomains.actions.disable')}
              </Button>
            </div>
          )
        },
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.resellerDomains.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={filters.keyword}
                  placeholder={t('admin.resellerDomains.filters.keyword')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input v-model={filters.resellerId} placeholder={t('admin.resellerDomains.filters.resellerId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.userId} placeholder={t('admin.resellerDomains.filters.userId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.domain} placeholder={t('admin.resellerDomains.filters.domain')} onUpdate:modelValue={list.debouncedSearch} />
                <Select
                  v-model={filters.type}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: t('admin.resellerDomains.filters.typeAll'),
                      value: '__all__',
                    },
                    ...RESELLER_DOMAIN_TYPES.map((v) => ({
                      label: typeLabel(v),
                      value: v,
                    })),
                  ]}
                />
                <Select
                  v-model={filters.status}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: t('admin.resellerDomains.filters.statusAll'),
                      value: '__all__',
                    },
                    ...RESELLER_DOMAIN_STATUSES.map((v) => ({
                      label: statusLabel(v),
                      value: v,
                    })),
                  ]}
                />
                <Select
                  v-model={filters.verificationStatus}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: t('admin.resellerDomains.filters.verificationAll'),
                      value: '__all__',
                    },
                    ...RESELLER_DOMAIN_VERIFICATIONS.map((v) => ({
                      label: verificationLabel(v),
                      value: v,
                    })),
                  ]}
                />
                <CreatedRange
                  label={t('admin.resellerDomains.filters.createdRange')}
                  v-model:from={filters.createdFrom}
                  v-model:to={filters.createdTo}
                  onChange={list.handleSearch}
                />
              </>
            ),
            actions: () => <RefreshButton loading={refreshing.value} onClick={refresh} />,
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.resellerDomains.empty')}
            minWidth="1100px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>
      </div>
    )
  },
})
