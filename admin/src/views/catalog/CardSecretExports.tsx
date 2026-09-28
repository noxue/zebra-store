import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { AlertTriangle, Copy, Download, PackageCheck, RefreshCw, Upload } from 'lucide-vue-next'
import { Badge, Button, Card, Checkbox, Dialog, FormField, Input, PageHeader, Select } from '@/components/ui'
import ProductSkuPicker from './components/ProductSkuPicker'
import { useCardSecretExports } from './useCardSecretExports'

export default defineComponent({
  name: 'CardSecretExportsView',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const x = useCardSecretExports()
    const pk = x.picker
    onMounted(() => void x.init())

    const renderForm = () => (
      <div class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
        <Card title={t('admin.cardSecretExports.formTitle')} description={t('admin.cardSecretExports.formDescription')}>
          <div class="space-y-4">
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
              <FormField label={t('admin.cardSecretExports.batchLabel')}>
                <Select
                  v-model={x.batchValue.value}
                  placeholder={t('admin.cardSecretExports.batchPlaceholder')}
                  options={x.batchOptions.value}
                  disabled={x.batchesLoading.value || x.batches.value.length === 0}
                />
              </FormField>
              <FormField label={t('admin.cardSecretExports.countLabel')} hint={t('admin.cardSecretExports.availableHint', { count: x.currentAvailable.value })}>
                <Input v-model={x.exportCount.value} type="number" min={1} max={x.currentAvailable.value || undefined} step={1} />
              </FormField>
              <FormField label={t('admin.cardSecretExports.formatLabel')}>
                <Select
                  v-model={x.exportFormat.value}
                  options={[
                    { value: 'txt', label: 'TXT' },
                    { value: 'csv', label: 'CSV' },
                  ]}
                />
              </FormField>
            </div>

            <div class="flex items-start gap-3 rounded-zs-sm border border-line bg-danger-soft/60 p-3">
              <Checkbox class="mt-0.5" v-model={x.deleteAfterExport.value} />
              <div>
                <p class="text-sm font-medium text-fg">{t('admin.cardSecretExports.deleteAfterExport')}</p>
                <p class="mt-1 text-xs text-muted">{t('admin.cardSecretExports.deleteAfterExportHint')}</p>
              </div>
            </div>

            {x.successMessage.value && <div class="rounded-zs-sm border border-line bg-success-soft px-3 py-2 text-sm text-success-text">{x.successMessage.value}</div>}
            {x.errorMessage.value && <div class="rounded-zs-sm border border-line bg-danger-soft px-3 py-2 text-sm text-danger-text">{x.errorMessage.value}</div>}

            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-end">
              <Button disabled={x.exporting.value} onClick={() => void x.loadInventoryMeta()}>
                <RefreshCw class="h-3.5 w-3.5" />
                {t('admin.common.refresh')}
              </Button>
              <Button variant="primary" disabled={x.exportDisabled.value} loading={x.exporting.value} onClick={x.submitExport}>
                <Download class="h-4 w-4" />
                {x.exporting.value ? t('admin.cardSecretExports.exporting') : t('admin.cardSecretExports.submit')}
              </Button>
            </div>
          </div>
        </Card>

        <div class="rounded-zs-lg border border-line bg-primary-soft/70 p-4">
          <p class="text-sm font-medium text-fg">{t('admin.cardSecretExports.targetTitle')}</p>
          <p class="mt-2 text-sm text-muted">{pk.productLabel.value}</p>
          <p class="mt-1 text-xs text-muted">
            {t('admin.cardSecrets.skuLabel')}：{pk.currentSkuLabel.value}
          </p>
          <div class="mt-4 grid grid-cols-2 gap-3">
            <div class="rounded-zs-sm border border-line bg-surface-solid p-3">
              <p class="text-xs text-muted">{t('admin.cardSecrets.stats.available')}</p>
              <p class="zs-num mt-1 text-2xl font-semibold text-success-text">{x.currentAvailable.value}</p>
            </div>
            <div class="rounded-zs-sm border border-line bg-surface-solid p-3">
              <p class="text-xs text-muted">{t('admin.cardSecretExports.batchCount')}</p>
              <p class="zs-num mt-1 text-2xl font-semibold text-fg">{x.batches.value.length}</p>
            </div>
          </div>
          <p class="mt-4 text-xs text-muted">{t('admin.cardSecretExports.targetHint')}</p>
        </div>
      </div>
    )

    const renderConfirm = () => (
      <Dialog v-model={x.confirmOpen.value} title={t('admin.cardSecretExports.confirmTitle')} size="md">
        {{
          default: () => (
            <div class="space-y-4">
              <div class="flex items-start gap-3">
                <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-warning-soft text-warning-text">
                  <AlertTriangle class="h-5 w-5" />
                </div>
                <p class="text-sm leading-6 text-muted">{x.confirmMessage.value}</p>
              </div>
              <div class="space-y-2 rounded-zs-sm border border-line bg-surface-muted/60 p-3 text-sm">
                <div class="flex justify-between gap-4">
                  <span class="text-muted">{t('admin.cardSecretExports.targetTitle')}</span>
                  <span class="text-right font-medium text-fg">{pk.productLabel.value}</span>
                </div>
                <div class="flex justify-between gap-4">
                  <span class="text-muted">{t('admin.cardSecrets.skuLabel')}</span>
                  <span class="text-right text-fg">{pk.currentSkuLabel.value}</span>
                </div>
                <div class="flex justify-between gap-4">
                  <span class="text-muted">{t('admin.cardSecretExports.countLabel')}</span>
                  <span class="zs-num text-fg">{x.countValue.value}</span>
                </div>
              </div>
            </div>
          ),
          footer: () => (
            <>
              <Button disabled={x.exporting.value} onClick={() => (x.confirmOpen.value = false)}>
                {t('admin.common.cancel')}
              </Button>
              <Button variant={x.deleteAfterExport.value ? 'danger' : 'primary'} loading={x.exporting.value} onClick={() => void x.runConfirmedExport()}>
                {x.exporting.value ? t('admin.cardSecretExports.exporting') : t('admin.common.confirm')}
              </Button>
            </>
          ),
        }}
      </Dialog>
    )

    const renderResult = () => {
      const r = x.exportResult.value
      return (
        <Dialog
          modelValue={!!r}
          onUpdate:modelValue={(v) => !v && x.closeResult()}
          title={t('admin.cardSecretExports.result.title')}
          description={r ? (r.deleted ? t('admin.cardSecretExports.success.deleted', { count: r.count }) : t('admin.cardSecretExports.success.used', { count: r.count })) : ''}
          size="2xl"
          closeOnOverlay={false}
        >
          {{
            default: () =>
              r && (
                <div class="space-y-4">
                  <div class="grid gap-3 rounded-zs-sm border border-line bg-surface-muted/60 p-3 text-sm md:grid-cols-3">
                    <div class="min-w-0">
                      <div class="text-xs text-muted">{t('admin.cardSecretExports.targetTitle')}</div>
                      <div class="mt-1 truncate text-fg">{r.productLabel}</div>
                    </div>
                    <div class="min-w-0">
                      <div class="text-xs text-muted">{t('admin.cardSecrets.skuLabel')}</div>
                      <div class="mt-1 truncate text-fg">{r.skuLabel}</div>
                    </div>
                    <div class="min-w-0">
                      <div class="text-xs text-muted">{t('admin.cardSecretExports.result.filename')}</div>
                      <div class="mt-1 truncate font-mono text-fg">{r.filename}</div>
                    </div>
                  </div>
                  <div>
                    <div class="mb-2 flex items-center justify-between gap-3">
                      <div class="flex items-center gap-2 text-sm font-medium text-fg">
                        {t('admin.cardSecretExports.result.content')}
                        <Badge tone="success">{r.format.toUpperCase()}</Badge>
                      </div>
                      {x.resultMessage.value && <div class="text-xs text-muted">{x.resultMessage.value}</div>}
                    </div>
                    <textarea
                      value={r.content}
                      readonly
                      spellcheck={false}
                      class="h-[46vh] min-h-[280px] w-full resize-none rounded-zs border border-line-strong bg-surface-strong p-4 font-mono text-xs leading-5 text-fg outline-none focus:border-primary"
                    />
                    <p class="mt-2 text-xs leading-5 text-muted">{t('admin.cardSecretExports.result.keepOpenHint')}</p>
                  </div>
                </div>
              ),
            footer: () => (
              <div class="flex w-full flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <Button onClick={() => void x.copyResult()}>
                  <Copy class="h-4 w-4" />
                  {t('admin.cardSecretExports.result.copy')}
                </Button>
                <div class="flex flex-col gap-2 sm:flex-row">
                  <Button variant="primary" onClick={x.downloadResult}>
                    <Download class="h-4 w-4" />
                    {t('admin.cardSecretExports.result.download')}
                  </Button>
                  <Button onClick={x.closeResult}>{t('admin.common.close')}</Button>
                </div>
              </div>
            ),
          }}
        </Dialog>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.cardSecretExports.title')} subtitle={t('admin.cardSecretExports.subtitle')}>
          {{
            actions: () => (
              <Button onClick={() => void router.push('/card-secret-imports')}>
                <Upload class="h-4 w-4" />
                {t('admin.cardSecretExports.importAction')}
              </Button>
            ),
          }}
        </PageHeader>

        <ProductSkuPicker
          picker={pk}
          title={t('admin.cardSecretExports.selectionTitle')}
          description={t('admin.cardSecretExports.selectionDescription')}
          hint={pk.productId.value ? t('admin.cardSecrets.productHintCurrent', { id: pk.productId.value }) : t('admin.cardSecretExports.selectionTip')}
          onProductChange={() => void x.onProductChange()}
          onSkuChange={() => void x.onSkuChange()}
        />

        {!pk.productId.value ? (
          <div class="rounded-zs-lg border-2 border-dashed border-line-strong bg-primary-soft/60 p-8">
            <div class="mx-auto max-w-xl space-y-4 text-center">
              <div class="zs-gradient-bg mx-auto flex h-16 w-16 items-center justify-center rounded-full text-on-primary shadow-zs">
                <PackageCheck class="h-8 w-8" />
              </div>
              <h2 class="zs-display text-xl text-fg">{t('admin.cardSecretExports.emptyTitle')}</h2>
              <p class="text-sm text-muted">{t('admin.cardSecretExports.emptyDescription')}</p>
            </div>
          </div>
        ) : (
          renderForm()
        )}

        {renderConfirm()}
        {renderResult()}
      </div>
    )
  },
})
