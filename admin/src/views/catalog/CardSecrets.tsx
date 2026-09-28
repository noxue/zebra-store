import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { PackagePlus, RefreshCw, Upload } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminCardSecret, AdminCardSecretBatch } from '@/api/types'
import { formatDate } from '@/utils/format'
import { adminUrl } from '@/utils/adminBase'
import ProductSkuPicker from './components/ProductSkuPicker'
import CardSecretEditModal from './components/CardSecretEditModal'
import { useCardSecrets } from './useCardSecrets'
import { CARD_SECRET_PAGE_SIZE_OPTIONS, CARD_SECRET_STATUSES, resolveSecretBatchLabel, statusTone } from './cardSecretUtils'

export default defineComponent({
  name: 'CardSecretsView',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const c = useCardSecrets()
    const pk = c.picker
    onMounted(() => void c.init())

    const statusOptions = () => CARD_SECRET_STATUSES.map((s) => ({ value: s, label: t(`admin.cardSecrets.status.${s}`) }))

    const renderGuide = () => (
      <div class="rounded-zs-lg border-2 border-dashed border-line-strong bg-primary-soft/60 p-8">
        <div class="mx-auto max-w-lg space-y-4 text-center">
          <div class="zs-gradient-bg mx-auto flex h-16 w-16 items-center justify-center rounded-full text-on-primary shadow-zs">
            <PackagePlus class="h-8 w-8" />
          </div>
          <h2 class="zs-display text-xl text-fg">{t('admin.cardSecrets.guide.title')}</h2>
          <p class="text-sm text-muted">{t('admin.cardSecrets.guide.description')}</p>
          <div class="flex flex-col gap-4 pt-2 text-left sm:flex-row sm:items-start sm:justify-center sm:gap-6">
            {[1, 2].map((n) => (
              <div key={n} class="flex items-start gap-3">
                <div class="zs-gradient-bg flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-bold text-on-primary">{n}</div>
                <div>
                  <p class="text-sm font-medium text-fg">{t(`admin.cardSecrets.guide.step${n}Title`)}</p>
                  <p class="text-xs text-muted">{t(`admin.cardSecrets.guide.step${n}Desc`)}</p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    )

    const renderBatchCard = (batch: AdminCardSecretBatch) => {
      const active = c.currentBatchId.value === Number(batch.id || 0)
      return (
        <button
          key={batch.id}
          type="button"
          class={[
            'w-full rounded-zs border px-4 py-3 text-left transition hover:border-primary hover:bg-primary-soft/60',
            active ? 'border-primary bg-primary-soft shadow-zs-sm' : 'border-line bg-surface-solid',
          ]}
          onClick={() => void c.filterByBatch(batch)}
        >
          <div class="flex items-start justify-between gap-3">
            <div class="space-y-1">
              <p class="text-sm font-medium text-fg">{batch.batch_no || `#${batch.id}`}</p>
              <p class="text-xs text-muted">
                #{batch.id} · {pk.skuLabelById(Number(batch.sku_id || 0))}
              </p>
            </div>
            <Badge tone="neutral">{batch.total_count}</Badge>
          </div>
          <div class="mt-3 flex flex-wrap gap-2">
            <Badge tone="success">
              {t('admin.cardSecrets.stats.available')} {batch.available_count}
            </Badge>
            <Badge tone="warning">
              {t('admin.cardSecrets.stats.reserved')} {batch.reserved_count}
            </Badge>
            <Badge tone="neutral">
              {t('admin.cardSecrets.stats.used')} {batch.used_count}
            </Badge>
          </div>
          {batch.note && <p class="mt-3 line-clamp-2 text-xs text-muted">{batch.note}</p>}
          <p class="mt-3 text-[11px] text-muted">{formatDate(batch.created_at)}</p>
        </button>
      )
    }

    const columns = (): DataTableColumn<AdminCardSecret>[] => [
      { key: 'id', title: t('admin.cardSecrets.listTable.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'secret', title: t('admin.cardSecrets.listTable.secret'), class: 'min-w-[120px] break-all font-mono text-xs text-muted', render: (r) => r.secret },
      {
        key: 'product',
        title: t('admin.cardSecrets.listTable.product'),
        class: 'min-w-[120px] text-xs',
        render: (r) =>
          r.product_id ? (
            <a href={pk.productLink(r.product_id)} target="_blank" rel="noopener" class="break-words text-accent underline-offset-4 hover:underline">
              #{r.product_id} {pk.productNameById(r.product_id)}
            </a>
          ) : (
            <span class="text-muted">-</span>
          ),
      },
      { key: 'sku', title: t('admin.cardSecrets.listTable.sku'), class: 'min-w-[100px] break-words text-xs text-muted', render: (r) => pk.skuLabelById(Number(r.sku_id || 0)) },
      {
        key: 'status',
        title: t('admin.cardSecrets.listTable.status'),
        render: (r) => (
          <Badge tone={statusTone(r.status)} dot>
            {c.statusLabel(r.status)}
          </Badge>
        ),
      },
      {
        key: 'orderId',
        title: t('admin.cardSecrets.listTable.orderId'),
        class: 'text-xs',
        render: (r) => (
          <div class="flex flex-col gap-1">
            {r.order_id ? (
              <a href={adminUrl(`/orders?order_id=${r.order_id}`)} target="_blank" rel="noopener" class="text-accent underline-offset-4 hover:underline">
                #{r.order_id}
              </a>
            ) : (
              <span class="text-muted">-</span>
            )}
            {r.status === 'used' && <span class="text-[11px] text-muted">{t('admin.cardSecrets.listTable.usedOrderHint')}</span>}
          </div>
        ),
      },
      {
        key: 'batchId',
        title: t('admin.cardSecrets.listTable.batchId'),
        class: 'text-xs',
        render: (r) =>
          r.batch_id ? (
            <button
              type="button"
              class="text-left text-accent underline-offset-4 hover:underline"
              onClick={() => void (r.batch ? c.filterByBatch(r.batch) : c.filterByBatchId(Number(r.batch_id || 0)))}
            >
              {resolveSecretBatchLabel(r)}
            </button>
          ) : (
            <span class="text-muted">-</span>
          ),
      },
      { key: 'createdAt', title: t('admin.cardSecrets.listTable.createdAt'), class: 'whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.created_at) },
      {
        key: 'action',
        title: t('admin.cardSecrets.listTable.action'),
        align: 'right',
        render: (r) => (
          <Button size="sm" onClick={() => c.openEdit(r)}>
            {t('admin.cardSecrets.actions.edit')}
          </Button>
        ),
      },
    ]

    const renderOperationBar = () => {
      const disabled = c.actionLoading.value || !c.hasActionableScope.value
      return (
        <div class="sticky top-3 z-10 rounded-zs border border-line bg-surface-solid/95 p-3 shadow-zs-sm backdrop-blur">
          <div class="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <div class="space-y-1">
              <p class="text-sm font-medium text-fg">{t('admin.cardSecrets.batch.operationTitle')}</p>
              <p class="text-xs text-muted">{c.scopeHint.value}</p>
            </div>
            <div class="flex flex-col gap-2 xl:flex-row xl:items-center">
              <div class="w-full xl:w-40">
                <Select
                  size="sm"
                  v-model={c.operationScope.value}
                  options={[
                    { value: 'selected', label: t('admin.cardSecrets.batch.scopeSelected') },
                    { value: 'filtered', label: t('admin.cardSecrets.batch.scopeFiltered') },
                  ]}
                />
              </div>
              <div class="w-full xl:w-36">
                <Select size="sm" v-model={c.batchStatusTarget.value} options={statusOptions()} />
              </div>
              <div class="flex flex-wrap gap-2">
                <Button size="sm" disabled={disabled} onClick={() => void c.applyBatchStatus()}>
                  {t('admin.cardSecrets.batch.applyStatus')}
                </Button>
                <Button size="sm" disabled={disabled} onClick={() => void c.exportInScope('txt')}>
                  {t('admin.cardSecrets.batch.exportTxt')}
                </Button>
                <Button size="sm" disabled={disabled} onClick={() => void c.exportInScope('csv')}>
                  {t('admin.cardSecrets.batch.exportCsv')}
                </Button>
                <Button size="sm" variant="danger" disabled={disabled} onClick={() => void c.deleteInScope()}>
                  {t('admin.cardSecrets.batch.deleteSelected')}
                </Button>
              </div>
            </div>
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-2 text-xs">
            <Badge tone="neutral">{t('admin.cardSecrets.batch.scopeCount', { scope: c.scopeLabel.value, count: c.scopeCount.value })}</Badge>
            {c.operationScope.value === 'selected' && c.selectedCount.value > 0 && (
              <Badge tone="primary">{t('admin.cardSecrets.batch.selectedCount', { count: c.selectedCount.value })}</Badge>
            )}
          </div>
          {c.actionError.value && <div class="mt-3 rounded-zs-sm border border-line bg-danger-soft p-3 text-sm text-danger-text">{c.actionError.value}</div>}
          {c.actionSuccess.value && <div class="mt-3 rounded-zs-sm border border-line bg-success-soft p-3 text-sm text-success-text">{c.actionSuccess.value}</div>}
        </div>
      )
    }

    return () => {
      const stats = c.stats.value
      return (
        <div class="space-y-6">
          <PageHeader title={t('admin.cardSecrets.title')}>
            {{
              actions: () => (
                <Button variant="primary" onClick={() => void router.push('/card-secret-imports')}>
                  <Upload class="h-4 w-4" />
                  {t('admin.cardSecrets.importAction')}
                </Button>
              ),
            }}
          </PageHeader>

          {!pk.productId.value && renderGuide()}

          <ProductSkuPicker picker={pk} hint={c.productHint.value} onProductChange={() => void c.onProductChange()} onSkuChange={() => void c.onSkuChange()}>
            {{
              extra: () => (
                <Button block disabled={c.refreshing.value} onClick={() => void c.refreshCurrent()}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {t('admin.common.refresh')}
                </Button>
              ),
              default: () => (
                <>
                  {pk.productId.value && (c.statsLoading.value || stats) && (
                    <p class="text-sm">
                      {c.statsLoading.value || !stats ? (
                        t('admin.common.loading')
                      ) : (
                        <>
                          <strong class="font-semibold text-fg">{t('admin.cardSecrets.statsTitle')}：</strong>
                          {t('admin.cardSecrets.stats.available')} (<span class="zs-num text-success-text">{stats.available}</span>) /{' '}
                          {t('admin.cardSecrets.stats.reserved')} (<span class="zs-num text-warning-text">{stats.reserved}</span>) /{' '}
                          {t('admin.cardSecrets.stats.used')} (<span class="zs-num text-fg">{stats.used}</span>) / {t('admin.cardSecrets.stats.total')} (
                          <span class="zs-num text-fg">{stats.total}</span>)
                        </>
                      )}
                    </p>
                  )}
                  {pk.productId.value && (
                    <p>
                      {c.batchFilterText.value}
                      {c.currentBatchId.value > 0 && (
                        <button type="button" class="ml-2 text-accent underline-offset-4 hover:underline" onClick={() => void c.clearBatchFilter()}>
                          {t('admin.cardSecrets.batchFilterClear')}
                        </button>
                      )}
                    </p>
                  )}
                </>
              ),
            }}
          </ProductSkuPicker>

          <Card title={t('admin.cardSecrets.batchesTitle')} description={t('admin.cardSecrets.batch.navigatorHint')}>
            {{
              extra: () => (
                <Button size="sm" disabled={c.refreshing.value} onClick={() => void c.refreshBatches()}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {t('admin.common.refresh')}
                </Button>
              ),
              default: () => (
                <div class="space-y-3">
                  <button
                    type="button"
                    class={[
                      'w-full rounded-zs border px-4 py-3 text-left transition hover:border-primary hover:bg-primary-soft/60',
                      !c.currentBatchId.value ? 'border-primary bg-primary-soft shadow-zs-sm' : 'border-line bg-surface-solid',
                    ]}
                    onClick={() => void c.filterByBatch(null)}
                  >
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-sm font-medium text-fg">{t('admin.cardSecrets.batch.navigatorAll')}</span>
                      <span class="zs-num text-xs text-muted">{c.batchPagination.value.total}</span>
                    </div>
                    <p class="mt-2 text-xs text-muted">{t('admin.cardSecrets.batch.navigatorAllDesc')}</p>
                  </button>
                  <div class="max-h-[20rem] space-y-3 overflow-y-auto pr-1">
                    {c.batchesLoading.value ? (
                      [1, 2, 3].map((i) => <div key={i} class="zs-skeleton h-24 rounded-zs" />)
                    ) : c.batches.value.length === 0 ? (
                      <div class="rounded-zs border border-dashed border-line-strong px-4 py-10 text-center text-sm text-muted">{t('admin.cardSecrets.emptyBatches')}</div>
                    ) : (
                      c.batches.value.map(renderBatchCard)
                    )}
                  </div>
                  <ListPagination
                    pagination={c.batchPagination.value}
                    pageSizeOptions={CARD_SECRET_PAGE_SIZE_OPTIONS}
                    onChangePage={c.changeBatchPage}
                    onChangePageSize={c.changeBatchPageSize}
                  />
                </div>
              ),
            }}
          </Card>

          <Card>
            <div class="space-y-4">
              <div class="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                <div>
                  <h2 class="zs-display text-lg text-fg">{t('admin.cardSecrets.listTitle')}</h2>
                  <p class="text-xs text-muted">{c.batchFilterText.value}</p>
                </div>
                <div class="grid w-full grid-cols-1 gap-3 md:grid-cols-4 lg:max-w-4xl">
                  <Input v-model={c.filters.secret} placeholder={t('admin.cardSecrets.filters.secretPlaceholder')} onEnter={() => void c.applyFilters()} />
                  <Input v-model={c.filters.batchNo} placeholder={t('admin.cardSecrets.filters.batchNoPlaceholder')} onEnter={() => void c.applyFilters()} />
                  <Select v-model={c.filters.status} options={[{ value: '__all__', label: t('admin.cardSecrets.statusAll') }, ...statusOptions()]} />
                  <div class="flex gap-2">
                    <Button class="flex-1" variant="primary" onClick={() => void c.applyFilters()}>
                      {t('admin.common.search')}
                    </Button>
                    <Button class="flex-1" onClick={() => void c.resetFilters()}>
                      {t('admin.common.reset')}
                    </Button>
                  </div>
                </div>
              </div>

              {c.currentBatchId.value > 0 && (
                <div class="rounded-zs-sm border border-line bg-primary-soft px-3 py-2 text-xs text-muted">
                  <span>{c.batchFilterText.value}</span>
                  <button type="button" class="ml-2 text-accent underline-offset-4 hover:underline" onClick={() => void c.clearBatchFilter()}>
                    {t('admin.cardSecrets.batchFilterClear')}
                  </button>
                </div>
              )}

              {renderOperationBar()}

              <div>
                <DataTable
                  bare
                  columns={columns()}
                  rows={c.secrets.value}
                  rowKey={(r) => r.id}
                  loading={c.secretsLoading.value}
                  emptyText={t('admin.cardSecrets.emptyList')}
                  minWidth="1020px"
                  selection={{
                    allSelected: c.allPageSelected.value,
                    someSelected: c.somePageSelected.value,
                    isSelected: (r) => c.selectedIds.value.includes(r.id),
                    toggle: c.toggleSelected,
                    toggleAll: c.toggleSelectAll,
                  }}
                />
                <ListPagination
                  pagination={c.secretPagination.value}
                  pageSizeOptions={CARD_SECRET_PAGE_SIZE_OPTIONS}
                  onChangePage={c.changeSecretPage}
                  onChangePageSize={c.changeSecretPageSize}
                />
              </div>
            </div>
          </Card>

          <CardSecretEditModal v-model={c.showEditModal.value} cardSecret={c.editingSecret.value} onSuccess={() => void c.refreshAfterMutations()} />
        </div>
      )
    }
  },
})
