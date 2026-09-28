import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, Dialog, FilterBar, FormField, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminApiCredential } from '@/api/types'
import { adminUrl } from '@/utils/adminBase'
import { apiCredentialStatusTone, formatTime } from './integrationUtils'
import { API_CREDENTIAL_STATUSES, useApiCredentials } from './useApiCredentials'

export default defineComponent({
  name: 'ApiCredentialsView',
  setup() {
    const { t, te } = useI18n()
    const p = useApiCredentials()
    onMounted(() => void p.list.fetchData(1))

    const statusLabel = (s: string) => (te(`apiCredentials.status.${s}`) ? t(`apiCredentials.status.${s}`) : s)
    const statusBadge = (c: AdminApiCredential) => (
      <Badge tone={apiCredentialStatusTone(c.status)} dot>
        {statusLabel(c.status)}
      </Badge>
    )
    const activeBadge = (c: AdminApiCredential) => <Badge tone={c.is_active ? 'success' : 'neutral'}>{c.is_active ? 'ON' : 'OFF'}</Badge>

    const columns = (): DataTableColumn<AdminApiCredential>[] => [
      { key: 'id', title: t('apiCredentials.columns.id'), render: (c) => <IdCell value={c.id} /> },
      {
        key: 'user',
        title: t('apiCredentials.columns.user'),
        class: 'min-w-[180px]',
        render: (c) => (
          <div class="space-y-0.5 text-sm">
            <div class="break-words font-medium text-fg">{c.user?.display_name || '-'}</div>
            <div class="break-all text-xs text-muted">{c.user?.email || '-'}</div>
            <div class="text-xs text-muted">
              ID:{' '}
              <a href={adminUrl(`/users/${c.user_id}`)} target="_blank" rel="noopener" class="font-mono text-accent hover:underline">
                {c.user_id}
              </a>
            </div>
          </div>
        ),
      },
      { key: 'apiKey', title: t('apiCredentials.columns.apiKey'), class: 'min-w-[140px] max-w-[260px] font-mono text-xs break-all', render: (c) => c.api_key || '-' },
      { key: 'status', title: t('apiCredentials.columns.status'), render: statusBadge },
      { key: 'isActive', title: t('apiCredentials.columns.isActive'), render: activeBadge },
      { key: 'lastUsedAt', title: t('apiCredentials.columns.lastUsedAt'), class: 'text-xs text-muted whitespace-nowrap', render: (c) => formatTime(c.last_used_at) },
      { key: 'createdAt', title: t('apiCredentials.columns.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (c) => formatTime(c.created_at) },
      {
        key: 'actions',
        title: t('apiCredentials.columns.actions'),
        class: 'min-w-[250px]',
        render: (c) => (
          <div class="flex flex-wrap gap-1">
            <Button size="xs" onClick={() => p.openDetail(c)}>
              {t('apiCredentials.actions.detail')}
            </Button>
            {c.status === 'pending_review' && (
              <>
                <Button size="xs" variant="primary" onClick={() => p.approve(c)}>
                  {t('apiCredentials.actions.approve')}
                </Button>
                <Button size="xs" variant="danger" onClick={() => p.openReject(c)}>
                  {t('apiCredentials.actions.reject')}
                </Button>
              </>
            )}
            {c.status === 'approved' && (
              <Button size="xs" variant={c.is_active ? 'outline' : 'primary'} onClick={() => p.toggle(c)}>
                {c.is_active ? t('apiCredentials.actions.disable') : t('apiCredentials.actions.enable')}
              </Button>
            )}
            <Button size="xs" variant="danger" onClick={() => p.remove(c)}>
              {t('apiCredentials.actions.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const detailRow = (label: string, value: unknown) => [
      <span class="text-muted">{label}</span>,
      <div class="min-w-0 text-fg">{value}</div>,
    ]

    const detailBody = () => {
      const d = p.detail.value
      if (!d) return null
      return (
        <div class="grid grid-cols-1 gap-x-4 gap-y-2.5 text-sm sm:grid-cols-[120px_1fr]">
          {detailRow(t('apiCredentials.columns.id'), d.id)}
          {detailRow(
            t('apiCredentials.columns.user'),
            <>
              <div>{d.user?.display_name || '-'}</div>
              <div class="text-xs text-muted">{d.user?.email || '-'}</div>
              <div class="text-xs text-muted">ID: {d.user_id}</div>
            </>,
          )}
          {detailRow(t('apiCredentials.columns.apiKey'), <span class="break-all font-mono text-xs">{d.api_key || '-'}</span>)}
          {detailRow(t('apiCredentials.columns.status'), statusBadge(d))}
          {detailRow(t('apiCredentials.columns.isActive'), d.is_active ? 'ON' : 'OFF')}
          {d.reject_reason && detailRow(t('apiCredentials.detail.rejectReason'), <span class="text-danger-text">{d.reject_reason}</span>)}
          {detailRow(t('apiCredentials.columns.approvedAt'), formatTime(d.approved_at))}
          {detailRow(t('apiCredentials.columns.lastUsedAt'), formatTime(d.last_used_at))}
          {detailRow(t('apiCredentials.columns.createdAt'), formatTime(d.created_at))}
        </div>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('apiCredentials.title')} />

        <FilterBar cols={3}>
          {{
            default: () => (
              <>
                <Select
                  v-model={p.filters.status}
                  onChange={p.list.handleSearch}
                  placeholder={t('apiCredentials.filters.statusPlaceholder')}
                  options={[
                    { label: t('apiCredentials.filters.allStatus'), value: '__all__' },
                    ...API_CREDENTIAL_STATUSES.map((s) => ({ label: t(`apiCredentials.status.${s}`), value: s })),
                  ]}
                />
                <Input
                  icon={Search}
                  v-model={p.filters.search}
                  placeholder={t('apiCredentials.filters.searchPlaceholder')}
                  onUpdate:modelValue={p.list.debouncedSearch}
                  onEnter={p.list.handleSearch}
                />
                <div>
                  <Button onClick={p.list.handleSearch}>
                    <Search class="h-4 w-4" />
                    {t('apiCredentials.filters.search')}
                  </Button>
                </div>
              </>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(c) => c.id}
            loading={p.list.loading.value}
            emptyText={t('apiCredentials.empty')}
            minWidth="1040px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog v-model={p.showDetail.value} title={t('apiCredentials.detail.title')} size="lg">
          {{ default: detailBody }}
        </Dialog>

        <Dialog v-model={p.showReject.value} title={t('apiCredentials.reject.title')} size="md" closeOnOverlay={false}>
          {{
            default: () => (
              <FormField label={t('apiCredentials.reject.reasonLabel')} required>
                <Input v-model={p.rejectReason.value} placeholder={t('apiCredentials.reject.reasonPlaceholder')} onEnter={p.submitReject} />
              </FormField>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showReject.value = false)}>{t('admin.common.cancel')}</Button>
                <Button variant="danger" loading={p.rejecting.value} disabled={!p.rejectReason.value.trim()} onClick={p.submitReject}>
                  {t('apiCredentials.actions.reject')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
