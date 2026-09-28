import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, RefreshCw } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  DateTimeInput,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  ListPagination,
  PageHeader,
  Select,
  Textarea,
  cn,
  type DataTableColumn,
} from '@/components/ui'
import type { AdminReconciliationItem, AdminReconciliationJob } from '@/api/types'
import { formatTime, mismatchTone, reconciliationStatusTone } from './integrationUtils'
import { RECON_ITEMS_PAGE_SIZE, RECON_STATUSES, RECON_TYPES, useReconciliation } from './useReconciliation'

export default defineComponent({
  name: 'ReconciliationView',
  setup() {
    const { t, te } = useI18n()
    const p = useReconciliation()
    onMounted(p.init)

    const tr = (prefix: string, v?: string) => (v && te(`${prefix}.${v}`) ? t(`${prefix}.${v}`) : v || '-')
    const connectionName = (job: AdminReconciliationJob) => job.connection?.name || job.connection_id || '-'
    const mismatched = (job: AdminReconciliationJob) => Number(job.mismatched_count ?? 0)
    const connectionOptions = () => p.connections.value.map((c) => ({ label: c.name || `#${c.id}`, value: c.id }))

    const columns = (): DataTableColumn<AdminReconciliationJob>[] => [
      { key: 'id', title: t('reconciliation.columns.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'connection', title: t('reconciliation.columns.connection'), class: 'min-w-[160px] font-medium text-fg break-words', render: connectionName },
      { key: 'type', title: t('reconciliation.columns.type'), class: 'text-xs', render: (r) => tr('reconciliation.type', r.type) },
      {
        key: 'status',
        title: t('reconciliation.columns.status'),
        render: (r) => (
          <Badge tone={reconciliationStatusTone(r.status)} dot>
            {tr('reconciliation.status', r.status)}
          </Badge>
        ),
      },
      {
        key: 'timeRange',
        title: t('reconciliation.columns.timeRange'),
        class: 'min-w-[160px] text-xs text-muted',
        render: (r) => `${formatTime(r.time_range_start)} ~ ${formatTime(r.time_range_end)}`,
      },
      { key: 'total', title: t('reconciliation.columns.total'), class: 'zs-num text-xs text-muted', render: (r) => r.total_count ?? '-' },
      { key: 'matched', title: t('reconciliation.columns.matched'), class: 'zs-num text-xs text-success-text', render: (r) => r.matched_count ?? '-' },
      {
        key: 'mismatched',
        title: t('reconciliation.columns.mismatched'),
        render: (r) => <span class={cn('zs-num text-xs', mismatched(r) > 0 ? 'font-semibold text-danger-text' : 'text-muted')}>{r.mismatched_count ?? '-'}</span>,
      },
      { key: 'createdAt', title: t('reconciliation.columns.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatTime(r.created_at) },
      {
        key: 'actions',
        title: t('reconciliation.columns.actions'),
        align: 'right',
        render: (r) => (
          <div onClick={(e: Event) => e.stopPropagation()}>
            <Button size="sm" onClick={() => p.openDetail(r)}>
              {t('reconciliation.detail.title')}
            </Button>
          </div>
        ),
      },
    ]

    const itemColumns = (): DataTableColumn<AdminReconciliationItem>[] => [
      { key: 'local', title: t('reconciliation.items.localOrderNo'), class: 'font-mono text-xs break-all', render: (i) => i.local_order_no || '-' },
      { key: 'upstream', title: t('reconciliation.items.upstreamOrderNo'), class: 'font-mono text-xs break-all', render: (i) => i.upstream_order_no || '-' },
      { key: 'localStatus', title: t('reconciliation.items.localStatus'), class: 'text-xs', render: (i) => i.local_status || '-' },
      { key: 'upstreamStatus', title: t('reconciliation.items.upstreamStatus'), class: 'text-xs', render: (i) => i.upstream_status || '-' },
      {
        key: 'mismatch',
        title: t('reconciliation.items.mismatchType'),
        render: (i) => <Badge tone={mismatchTone(i.mismatch_type)}>{tr('reconciliation.mismatchType', i.mismatch_type)}</Badge>,
      },
      {
        key: 'status',
        title: t('reconciliation.columns.status'),
        render: (i) => <Badge tone={i.resolved ? 'success' : 'warning'}>{i.resolved ? t('reconciliation.items.resolved') : t('reconciliation.items.unresolved')}</Badge>,
      },
      {
        key: 'actions',
        title: t('reconciliation.columns.actions'),
        align: 'right',
        render: (i) =>
          i.resolved ? (
            <span class="text-xs text-muted">{i.remark || '-'}</span>
          ) : (
            <Button size="sm" onClick={() => p.openResolve(i)}>
              {t('reconciliation.items.resolve')}
            </Button>
          ),
      },
    ]

    const field = (label: string, value: unknown) => (
      <div>
        <div class="mb-1 text-xs font-medium text-muted">{label}</div>
        <div class="text-sm text-fg">{value}</div>
      </div>
    )

    const detailBody = () => {
      const j = p.detailJob.value
      if (!j) return null
      return (
        <div class="space-y-5">
          <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
            {field(t('reconciliation.columns.id'), j.id)}
            {field(t('reconciliation.columns.connection'), connectionName(j))}
            {field(t('reconciliation.columns.type'), tr('reconciliation.type', j.type))}
            {field(
              t('reconciliation.columns.status'),
              <Badge tone={reconciliationStatusTone(j.status)} dot>
                {tr('reconciliation.status', j.status)}
              </Badge>,
            )}
            {field(t('reconciliation.columns.total'), j.total_count ?? '-')}
            {field(
              `${t('reconciliation.columns.matched')} / ${t('reconciliation.columns.mismatched')}`,
              <span class="zs-num">
                <span class="text-success-text">{j.matched_count ?? '-'}</span>
                {' / '}
                <span class={mismatched(j) > 0 ? 'font-semibold text-danger-text' : ''}>{j.mismatched_count ?? '-'}</span>
              </span>,
            )}
            {field(t('reconciliation.columns.timeRange'), <span class="text-xs">{`${formatTime(j.time_range_start)} ~ ${formatTime(j.time_range_end)}`}</span>)}
            {field(t('reconciliation.columns.startedAt'), formatTime(j.started_at))}
            {field(t('reconciliation.columns.finishedAt'), formatTime(j.finished_at))}
          </div>

          {j.result_json && (
            <div>
              <div class="mb-1 text-xs font-medium text-muted">{t('reconciliation.detail.resultJson')}</div>
              <div class="whitespace-pre-wrap break-all rounded-zs border border-line bg-surface-muted p-3 font-mono text-sm">{j.result_json}</div>
            </div>
          )}

          {p.detailItems.value.length > 0 && (
            <div>
              <h3 class="mb-3 text-sm font-semibold text-fg">
                {t('reconciliation.items.title')} ({p.itemsTotal.value})
              </h3>
              <DataTable columns={itemColumns()} rows={p.detailItems.value} rowKey={(i) => i.id} minWidth="840px" />
              {p.itemsTotal.value > RECON_ITEMS_PAGE_SIZE && (
                <div class="flex items-center justify-end gap-2 pt-3">
                  <Button size="xs" disabled={p.itemsPage.value <= 1} onClick={() => p.changeItemsPage(p.itemsPage.value - 1)}>
                    {t('admin.common.prevPage')}
                  </Button>
                  <span class="zs-num text-xs text-muted">{p.itemsPage.value}</span>
                  <Button
                    size="xs"
                    disabled={p.itemsPage.value * RECON_ITEMS_PAGE_SIZE >= p.itemsTotal.value}
                    onClick={() => p.changeItemsPage(p.itemsPage.value + 1)}
                  >
                    {t('admin.common.nextPage')}
                  </Button>
                </div>
              )}
            </div>
          )}
        </div>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('reconciliation.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openNewJob}>
                <Plus class="h-4 w-4" />
                {t('reconciliation.newJob')}
              </Button>
            ),
          }}
        </PageHeader>

        <FilterBar cols={3}>
          {{
            default: () => (
              <>
                <FormField label={t('reconciliation.columns.status')}>
                  <Select
                    v-model={p.filters.status}
                    onChange={p.list.handleSearch}
                    options={[{ label: t('reconciliation.filters.allStatus'), value: '__all__' }, ...RECON_STATUSES.map((s) => ({ label: t(`reconciliation.status.${s}`), value: s }))]}
                  />
                </FormField>
                <FormField label={t('reconciliation.columns.type')}>
                  <Select
                    v-model={p.filters.type}
                    onChange={p.list.handleSearch}
                    options={[{ label: t('reconciliation.filters.allTypes'), value: '__all__' }, ...RECON_TYPES.map((s) => ({ label: t(`reconciliation.type.${s}`), value: s }))]}
                  />
                </FormField>
                <FormField label={t('reconciliation.filters.connectionId')}>
                  <Select
                    v-model={p.filters.connection_id}
                    onChange={p.list.handleSearch}
                    options={[{ label: t('reconciliation.filters.allConnections'), value: '__all__' }, ...connectionOptions()]}
                  />
                </FormField>
              </>
            ),
            actions: () => (
              <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                <RefreshCw class="h-3.5 w-3.5" />
                {t('admin.common.refresh')}
              </Button>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('reconciliation.empty')}
            minWidth="960px"
            onRowClick={p.openDetail}
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog v-model={p.showNewJob.value} title={t('reconciliation.newJob')} size="md" closeOnOverlay={false}>
          {{
            default: () => (
              <div class="space-y-4">
                <FormField label={t('reconciliation.form.connectionId')} required>
                  <Select v-model={p.newJob.connection_id} placeholder={t('reconciliation.form.selectConnection')} options={connectionOptions()} />
                </FormField>
                <FormField label={t('reconciliation.form.type')}>
                  <Select v-model={p.newJob.type} options={RECON_TYPES.map((s) => ({ label: t(`reconciliation.type.${s}`), value: s }))} />
                </FormField>
                <FormField label={t('reconciliation.form.timeRangeStart')} required>
                  <DateTimeInput v-model={p.newJob.time_range_start} />
                </FormField>
                <FormField label={t('reconciliation.form.timeRangeEnd')} required>
                  <DateTimeInput v-model={p.newJob.time_range_end} />
                </FormField>
              </div>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showNewJob.value = false)}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.submitting.value} disabled={!p.canSubmitNewJob()} onClick={p.submitNewJob}>
                  {t('reconciliation.form.submit')}
                </Button>
              </>
            ),
          }}
        </Dialog>

        <Dialog v-model={p.showDetail.value} title={t('reconciliation.detail.title')} size="3xl">
          {{
            default: detailBody,
            footer: () => <Button onClick={() => (p.showDetail.value = false)}>{t('admin.common.cancel')}</Button>,
          }}
        </Dialog>

        <Dialog v-model={p.showResolve.value} title={t('reconciliation.items.resolve')} size="md" closeOnOverlay={false}>
          {{
            default: () => (
              <FormField label={t('reconciliation.items.remark')}>
                <Textarea v-model={p.resolveRemark.value} rows={3} placeholder={t('reconciliation.items.remarkPlaceholder')} />
              </FormField>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showResolve.value = false)}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.resolving.value} onClick={p.submitResolve}>
                  {t('reconciliation.items.resolve')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
