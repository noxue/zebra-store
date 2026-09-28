import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, RangeFilter, Select, type DataTableColumn } from '@/components/ui'
import type { AdminUserLoginLog } from '@/api/types'
import { formatDate } from '@/utils/format'
import { adminUrl } from '@/utils/adminBase'
import { LOGIN_FAIL_REASONS, useUserLoginLogs } from './useUserLoginLogs'

export default defineComponent({
  name: 'UserLoginLogsView',
  setup() {
    const { t, te } = useI18n()
    const { filters, list, refreshing, refresh } = useUserLoginLogs()
    onMounted(() => void list.fetchData(1))

    const failReasonLabel = (reason?: string) => {
      const r = (reason || '').trim()
      if (!r) return '-'
      const key = `admin.userLoginLogs.failReason.${r}`
      return te(key) ? t(key) : r
    }

    const columns = (): DataTableColumn<AdminUserLoginLog>[] => [
      { key: 'id', title: t('admin.userLoginLogs.table.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'user',
        title: t('admin.userLoginLogs.table.user'),
        render: (r) => (
          <div class="space-y-0.5">
            <p class="text-sm">{r.email || '-'}</p>
            {r.user_id > 0 && (
              <a href={adminUrl(`/users/${r.user_id}`)} class="text-xs text-accent hover:underline">
                {t('admin.userLoginLogs.userIdLabel')}: #{r.user_id}
              </a>
            )}
          </div>
        ),
      },
      {
        key: 'status',
        title: t('admin.userLoginLogs.table.status'),
        render: (r) => (
          <Badge tone={r.status === 'success' ? 'success' : 'danger'} dot>
            {t(`admin.userLoginLogs.status.${r.status || 'failed'}`)}
          </Badge>
        ),
      },
      { key: 'failReason', title: t('admin.userLoginLogs.table.failReason'), class: 'text-xs text-muted', render: (r) => failReasonLabel(r.fail_reason) },
      { key: 'clientIp', title: t('admin.userLoginLogs.table.clientIp'), class: 'font-mono text-xs', render: (r) => r.client_ip || '-' },
      { key: 'loginSource', title: t('admin.userLoginLogs.table.loginSource'), class: 'text-xs', render: (r) => r.login_source || '-' },
      { key: 'createdAt', title: t('admin.userLoginLogs.table.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.userLoginLogs.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={filters.userId} placeholder={t('admin.userLoginLogs.filterUserId')} onEnter={list.handleSearch} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.email} placeholder={t('admin.userLoginLogs.filterEmail')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.clientIp} placeholder={t('admin.userLoginLogs.filterClientIp')} onUpdate:modelValue={list.debouncedSearch} />
                <Select
                  v-model={filters.status}
                  onChange={list.handleSearch}
                  options={[
                    { label: t('admin.userLoginLogs.filterStatusAll'), value: '__all__' },
                    { label: t('admin.userLoginLogs.status.success'), value: 'success' },
                    { label: t('admin.userLoginLogs.status.failed'), value: 'failed' },
                  ]}
                />
                <Select
                  v-model={filters.failReason}
                  onChange={list.handleSearch}
                  options={[
                    { label: t('admin.userLoginLogs.filterFailReasonAll'), value: '__all__' },
                    ...LOGIN_FAIL_REASONS.map((r) => ({ label: failReasonLabel(r), value: r })),
                  ]}
                />
                {/* original captions this range with the "from" label */}
                <RangeFilter
                  label={t('admin.userLoginLogs.filterCreatedFrom')}
                  from={filters.createdFrom}
                  to={filters.createdTo}
                  fromPlaceholder={t('admin.userLoginLogs.filterCreatedFrom')}
                  toPlaceholder={t('admin.userLoginLogs.filterCreatedTo')}
                  onUpdate:from={(v: string) => (filters.createdFrom = v)}
                  onUpdate:to={(v: string) => (filters.createdTo = v)}
                  onChange={list.handleSearch}
                />
              </>
            ),
            actions: () => (
              <Button size="sm" loading={refreshing.value} onClick={refresh}>
                <RefreshCw class="h-3.5 w-3.5" />
                {t('admin.common.refresh')}
              </Button>
            ),
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.userLoginLogs.empty')}
            minWidth="900px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>
      </div>
    )
  },
})
