import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { PlugZap, Plus } from 'lucide-vue-next'
import { Badge, Button, DataTable, Dialog, FormField, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminSiteConnection } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { canReapplyMarkup, formatTime, hasMarkup, protocolTone, siteConnectionStatusTone, syncModeTone, webhookStatusTone } from './integrationUtils'
import { useSiteConnections } from './useSiteConnections'
import { ConnectionCodeBox, HandshakePanel, ProtocolFields, ProtocolPicker, RegistryNotice } from './components/SiteConnectionParts'

export default defineComponent({
  name: 'SiteConnectionsView',
  setup() {
    const { t, te } = useI18n()
    const p = useSiteConnections()
    onMounted(() => {
      void p.list.fetchData(1)
      void p.registry.load()
    })

    const statusLabel = (status?: string) => {
      const key = `siteConnections.status.${status}`
      return status && te(key) ? t(key) : status || '-'
    }

    const labelOr = (prefix: string, value?: string | null) => {
      const key = `${prefix}.${value}`
      return value && te(key) ? t(key) : value || '-'
    }

    const protocolName = (id?: string) => {
      const def = p.registry.find(id)
      return def ? getLocalizedText(def.name) || def.id : id || '-'
    }

    const columns = (): DataTableColumn<AdminSiteConnection>[] => [
      { key: 'id', title: t('siteConnections.columns.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'name', title: t('siteConnections.columns.name'), class: 'min-w-[160px] font-medium text-fg break-words', render: (r) => r.name },
      { key: 'baseUrl', title: t('siteConnections.columns.baseUrl'), class: 'min-w-[200px] font-mono text-xs text-muted break-all', render: (r) => r.base_url },
      {
        key: 'protocol',
        title: t('siteConnections.columns.protocol'),
        render: (r) => (
          <div class="flex flex-col items-start gap-1">
            <Badge tone={protocolTone(!!p.registry.find(r.protocol))}>{protocolName(r.protocol)}</Badge>
            {r.sync_mode && <Badge tone={syncModeTone(r.sync_mode)}>{labelOr('siteConnections.syncMode', r.sync_mode)}</Badge>}
          </div>
        ),
      },
      {
        key: 'supplierCurrency',
        title: t('siteConnections.columns.supplierCurrency'),
        class: 'zs-num text-xs whitespace-nowrap',
        render: (r) => r.supplier_currency || '-',
      },
      {
        key: 'webhook',
        title: t('siteConnections.columns.webhook'),
        render: (r) =>
          r.webhook_status ? (
            <Badge tone={webhookStatusTone(r.webhook_status)} dot>
              {labelOr('siteConnections.webhookStatus', r.webhook_status)}
            </Badge>
          ) : (
            <span class="text-xs text-muted">-</span>
          ),
      },
      {
        key: 'markup',
        title: t('siteConnections.columns.markup'),
        render: (r) => (hasMarkup(r) ? <Badge tone="info">+{r.price_markup_percent}%</Badge> : <span class="text-xs text-muted">-</span>),
      },
      {
        key: 'status',
        title: t('siteConnections.columns.status'),
        render: (r) => (
          <Badge tone={siteConnectionStatusTone(r.status)} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      { key: 'lastPing', title: t('siteConnections.columns.lastPing'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatTime(r.last_ping_at) },
      {
        key: 'actions',
        title: t('siteConnections.columns.actions'),
        align: 'right',
        class: 'min-w-[360px]',
        render: (r) => (
          <div class="flex flex-wrap items-center justify-end gap-2">
            <Button size="sm" onClick={() => p.openEdit(r)}>
              {t('admin.common.edit')}
            </Button>
            <Button size="sm" loading={p.pingingId.value === r.id} disabled={p.pingingId.value === r.id} onClick={() => p.ping(r)}>
              {p.pingingId.value === r.id ? t('siteConnections.ping.loading') : 'Ping'}
            </Button>
            {canReapplyMarkup(r) && (
              <Button size="sm" loading={p.reapplyingId.value === r.id} onClick={() => p.reapplyMarkup(r)}>
                {t('siteConnections.actions.reapplyMarkup')}
              </Button>
            )}
            <Button size="sm" onClick={() => p.toggleStatus(r)}>
              {r.status === 'active' ? t('siteConnections.actions.disable') : t('siteConnections.actions.enable')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => p.remove(r)}>
              {t('admin.common.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const renderForm = () => {
      const f = p.form
      const def = p.registry.find(f.protocol)
      const fields = p.registry.resolve(f.protocol)
      return (
        <form
          class="space-y-6"
          onSubmit={(e: Event) => {
            e.preventDefault()
            p.submit()
          }}
        >
          <section class="space-y-3">
            <div>
              <h3 class="text-sm font-semibold text-fg">{t('siteConnections.form.protocol')}</h3>
              <p class="mt-0.5 text-xs text-muted">{t('siteConnections.protocolHint')}</p>
            </div>
            {p.registry.loading.value && !p.registry.loaded.value && <p class="text-xs text-muted">{t('siteConnections.protocolsLoading')}</p>}
            {p.registry.error.value && (
              <RegistryNotice message={t('siteConnections.protocolsLoadFailed')} loading={p.registry.loading.value} onRetry={() => void p.registry.load()} />
            )}
            {p.registry.protocols.value.length > 0 && (
              <ProtocolPicker protocols={p.registry.protocols.value} modelValue={f.protocol} onUpdate:modelValue={p.selectProtocol} />
            )}
            {p.registry.loaded.value && f.protocol && !def && (
              <p class="text-xs text-warning-text">{t('siteConnections.protocolUnknown', { id: f.protocol })}</p>
            )}
            {p.errors.protocol && <p class="text-xs text-danger-text">{p.errors.protocol}</p>}
          </section>

          {!p.modal.isEditing.value && def?.supports_connection_code && <ConnectionCodeBox wizard={p.wizard} />}

          <section class="space-y-4">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="text-sm font-semibold text-fg">{t('siteConnections.credentialsSection')}</h3>
              <Button size="sm" loading={p.wizard.testing.value} data-testid="test-connection" onClick={() => void p.wizard.handshake()}>
                <PlugZap class="h-3.5 w-3.5" />
                {t('siteConnections.wizard.test')}
              </Button>
            </div>
            <ProtocolFields fields={fields.fields} config={f.config} errors={p.configErrors.value} editing={p.modal.isEditing.value} onChange={p.setConfigValue} />
            {p.modal.isEditing.value && fields.fields.some((x) => x.kind === 'secret') && <p class="text-[11px] text-muted">{t('siteConnections.wizard.editSecretHint')}</p>}
            <HandshakePanel wizard={p.wizard} exchangeRate={f.exchange_rate} callbackUrl={f.callback_url} />
          </section>

          <div class="grid grid-cols-1 gap-4 border-t border-line pt-4 md:grid-cols-2">
            <div class="md:col-span-2">
              <FormField label={t('siteConnections.form.name')} required error={p.errors.name}>
                <Input v-model={f.name} placeholder={t('siteConnections.form.namePlaceholder')} />
              </FormField>
            </div>
            <div class="md:col-span-2">
              <FormField label={t('siteConnections.form.callbackUrl')}>
                <Input v-model={f.callback_url} mono placeholder={t('siteConnections.form.callbackUrlPlaceholder')} />
              </FormField>
            </div>
            <FormField label={t('siteConnections.form.retryMax')}>
              <Input v-model={f.retry_max} type="number" min="0" placeholder="3" />
            </FormField>
            <FormField label={t('siteConnections.form.retryIntervals')}>
              <Input v-model={f.retry_intervals} placeholder="30,60,120" />
            </FormField>
          </div>

          <div class="border-t border-line pt-4">
            <h3 class="mb-3 text-sm font-semibold text-fg">{t('siteConnections.form.markupSection')}</h3>
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
              <FormField label={t('siteConnections.form.exchangeRate')} hint={t('siteConnections.form.exchangeRateHint')}>
                <Input v-model={f.exchange_rate} type="number" step="0.000001" min="0" placeholder="1" />
              </FormField>
              <FormField label={t('siteConnections.form.priceMarkupPercent')} hint={t('siteConnections.form.priceMarkupPercentHint')}>
                <Input v-model={f.price_markup_percent} type="number" step="0.01" min="0" placeholder="0">
                  {{ suffix: () => <span class="text-xs text-muted">%</span> }}
                </Input>
              </FormField>
              <FormField label={t('siteConnections.form.priceRoundingMode')}>
                <Select
                  v-model={f.price_rounding_mode}
                  options={[
                    { label: t('siteConnections.form.roundingNone'), value: 'none' },
                    { label: t('siteConnections.form.roundingCeilInt'), value: 'ceil_int' },
                    { label: t('siteConnections.form.roundingCeilTenth'), value: 'ceil_tenth' },
                  ]}
                />
              </FormField>
              <FormField label={t('siteConnections.form.autoSyncPrice')} hint={t('siteConnections.form.autoSyncPriceHint')}>
                <Select
                  modelValue={f.auto_sync_price}
                  onUpdate:modelValue={(v) => (f.auto_sync_price = v === 'true' ? 'true' : 'false')}
                  options={[
                    { label: t('admin.common.no'), value: 'false' },
                    { label: t('admin.common.yes'), value: 'true' },
                  ]}
                />
              </FormField>
            </div>
          </div>
          {p.modal.error.value && <p class="text-xs text-danger-text">{p.modal.error.value}</p>}
          <button type="submit" class="hidden" />
        </form>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('siteConnections.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('siteConnections.createButton')}
              </Button>
            ),
          }}
        </PageHeader>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('siteConnections.empty')}
            minWidth="1320px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('siteConnections.editTitle') : t('siteConnections.createTitle')}
          size="2xl"
          closeOnOverlay={false}
        >
          {{
            default: renderForm,
            footer: () => (
              <>
                <Button onClick={p.modal.closeModal}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.modal.submitting.value} onClick={p.submit}>
                  {p.modal.isEditing.value ? t('admin.common.save') : t('admin.common.create')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
