import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, RotateCcw, Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, DateTimeInput, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminAuthzAuditLog } from '@/api/types'
import { formatDate } from '@/utils/format'
import { AUDIT_ACTIONS, POLICY_ACTIONS } from './authz/authzUtils'
import { useAuthzAuditLogs } from './authz/useAuthzAuditLogs'

const actionTone = (a: string) =>
  a === 'role_create' || a === 'policy_grant' ? 'success' : a === 'role_delete' || a === 'policy_revoke' ? 'danger' : a === 'admin_roles_update' ? 'secondary' : 'neutral'

export default defineComponent({
  name: 'AuthzAuditLogsView',
  setup() {
    const { t } = useI18n()
    const tx = (k: string) => t(`admin.authzAudit.${k}`)
    const { filters, list, reset, refreshing, refresh } = useAuthzAuditLogs()
    onMounted(() => void list.fetchData(1))

    const columns = (): DataTableColumn<AdminAuthzAuditLog>[] => [
      { key: 'id', title: tx('table.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'operator',
        title: tx('table.operator'),
        class: 'text-xs break-words',
        render: (r) => (
          <span>
            <span class="zs-num text-muted">#{r.operator_admin_id}</span> {r.operator_username || '-'}
          </span>
        ),
      },
      {
        key: 'target',
        title: tx('table.target'),
        class: 'text-xs break-words',
        render: (r) =>
          r.target_admin_id ? (
            <span>
              <span class="zs-num text-muted">#{r.target_admin_id}</span> {r.target_username || '-'}
            </span>
          ) : (
            '-'
          ),
      },
      {
        key: 'action',
        title: tx('table.action'),
        render: (r) => (
          <Badge tone={actionTone(r.action)} class="font-mono">
            {r.action}
          </Badge>
        ),
      },
      { key: 'role', title: tx('table.role'), class: 'text-xs break-words', render: (r) => r.role || '-' },
      { key: 'object', title: tx('table.object'), class: 'font-mono text-xs text-muted break-all', render: (r) => r.object || '-' },
      { key: 'method', title: tx('table.method'), class: 'font-mono text-xs', render: (r) => r.method || '-' },
      { key: 'requestId', title: tx('table.requestId'), class: 'font-mono text-xs text-muted break-all', render: (r) => r.request_id || '-' },
      { key: 'createdAt', title: tx('table.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={tx('title')} subtitle={tx('subtitle')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input
                  type="number"
                  min={1}
                  icon={Search}
                  v-model={filters.operator_admin_id}
                  placeholder={tx('filters.operator')}
                  onEnter={list.handleSearch}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input
                  type="number"
                  min={1}
                  v-model={filters.target_admin_id}
                  placeholder={tx('filters.target')}
                  onEnter={list.handleSearch}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Select
                  v-model={filters.action}
                  onChange={list.handleSearch}
                  options={[{ label: tx('filters.allActions'), value: '__all__' }, ...AUDIT_ACTIONS.map((a) => ({ label: a, value: a }))]}
                />
                <Input v-model={filters.role} placeholder={tx('filters.role')} onEnter={list.handleSearch} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.object} placeholder={tx('filters.object')} onEnter={list.handleSearch} onUpdate:modelValue={list.debouncedSearch} />
                <Select
                  v-model={filters.method}
                  onChange={list.handleSearch}
                  options={[{ label: tx('filters.allMethods'), value: '__all__' }, ...POLICY_ACTIONS.map((m) => ({ label: m, value: m }))]}
                />
                <DateTimeInput v-model={filters.created_from} placeholder={tx('filters.createdFrom')} onUpdate:modelValue={list.handleSearch} />
                <DateTimeInput v-model={filters.created_to} placeholder={tx('filters.createdTo')} onUpdate:modelValue={list.handleSearch} />
              </>
            ),
            actions: () => (
              <>
                <Button size="sm" onClick={reset}>
                  <RotateCcw class="h-3.5 w-3.5" />
                  {tx('actions.reset')}
                </Button>
                <Button size="sm" loading={refreshing.value} onClick={refresh}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {tx('actions.refresh')}
                </Button>
                <Button size="sm" variant="primary" onClick={list.handleSearch}>
                  <Search class="h-3.5 w-3.5" />
                  {tx('actions.search')}
                </Button>
              </>
            ),
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={tx('table.empty')}
            minWidth="1000px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>
      </div>
    )
  },
})
