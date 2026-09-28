import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, Dialog, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminResellerProductSetting } from '@/api/types'
import { formatDate, getLocalizedText } from '@/utils/format'
import { getAdminResellerProductSettingOwnerLabel } from '@/utils/resellerProductSettings'
import { RefreshButton } from './components/RefreshButton'
import { SettingRuleRow } from './components/SettingRuleRow'
import { RESELLER_PRICING_MODES, pricingModeKey, pricingValue, skuLabel } from './resellerUtils'
import { useResellerProductSettings } from './useResellerProductSettings'

export default defineComponent({
  name: 'ResellerProductSettingsView',
  setup() {
    const { t } = useI18n()
    const s = useResellerProductSettings()
    const { filters, list } = s
    onMounted(() => void list.fetchData(1))

    const modeLabel = (m?: string) => t(`admin.resellerProductSettings.modes.${pricingModeKey(m)}`)
    const productTitle = (r: AdminResellerProductSetting) => getLocalizedText(r.product?.title) || `#${r.product_id}`
    const detailTitle = () => {
      const d = s.detail.value
      return d ? getLocalizedText(d.product.title) || `#${d.product.id}` : '-'
    }
    const previewHint = (key: number) => {
      const code = s.preview[key]?.errorCode
      if (!code) return ''
      return code === 'markup_exceeded' ? t('admin.resellerProductSettings.preview.markupExceeded') : t('admin.resellerProductSettings.preview.priceInvalid')
    }

    const columns = (): DataTableColumn<AdminResellerProductSetting>[] => [
      {
        key: 'id',
        title: t('admin.resellerProductSettings.columns.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'owner',
        title: t('admin.resellerProductSettings.columns.owner'),
        render: (r) => (
          <div class="min-w-[170px]">
            <div class="font-mono text-sm font-medium">R#{r.reseller_id}</div>
            <div class="break-all text-xs text-muted">{getAdminResellerProductSettingOwnerLabel(r)}</div>
          </div>
        ),
      },
      {
        key: 'product',
        title: t('admin.resellerProductSettings.columns.product'),
        render: (r) => (
          <div class="min-w-[200px]">
            <div class="text-sm font-medium">{productTitle(r)}</div>
            <div class="mt-0.5 font-mono text-xs text-muted">
              #{r.product_id} / {r.product?.slug || '-'}
            </div>
          </div>
        ),
      },
      {
        key: 'sku',
        title: t('admin.resellerProductSettings.columns.sku'),
        class: 'font-mono text-sm text-muted',
        render: (r) => String(r.sku_id || 0),
      },
      {
        key: 'listed',
        title: t('admin.resellerProductSettings.columns.listed'),
        render: (r) => (
          <Badge tone={r.is_listed === false ? 'danger' : 'success'} dot>
            {r.is_listed ? t('admin.resellerProductSettings.status.listed') : t('admin.resellerProductSettings.status.hidden')}
          </Badge>
        ),
      },
      {
        key: 'pricingMode',
        title: t('admin.resellerProductSettings.columns.pricingMode'),
        class: 'text-sm whitespace-nowrap',
        render: (r) => modeLabel(r.pricing_mode),
      },
      {
        key: 'pricingValue',
        title: t('admin.resellerProductSettings.columns.pricingValue'),
        class: 'zs-num text-sm',
        render: (r) => pricingValue(r),
      },
      {
        key: 'updatedAt',
        title: t('admin.resellerProductSettings.columns.updatedAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.updated_at),
      },
      {
        key: 'actions',
        title: t('admin.resellerProductSettings.columns.actions'),
        align: 'right',
        render: (r) => {
          const busy = s.operatingId.value === r.id
          return (
            <div class="flex justify-end gap-2">
              <Button size="xs" disabled={busy} onClick={() => s.openEditor(r)}>
                {t('admin.resellerProductSettings.actions.edit')}
              </Button>
              <Button size="xs" variant="ghost" class="text-danger-text" disabled={busy} onClick={() => s.reset(r)}>
                {t('admin.resellerProductSettings.actions.reset')}
              </Button>
            </div>
          )
        },
      },
    ]

    const renderEditor = () => {
      const d = s.detail.value
      if (!d) return null
      return (
        <div class="space-y-4">
          <Card padded>
            {{
              title: () => t('admin.resellerProductSettings.columns.product'),
              extra: () => (
                <span class="font-mono text-xs text-muted">
                  #{d.product.id} / {d.product.slug}
                </span>
              ),
              default: () => (
                <SettingRuleRow
                  label={t('admin.resellerProductSettings.columns.product')}
                  name={detailTitle()}
                  form={s.productForm.value}
                  preview={s.preview[0]}
                  hint={previewHint(0)}
                />
              ),
            }}
          </Card>
          <Card title={t('admin.resellerProductSettings.columns.sku')}>
            {d.skus.length ? (
              <div class="space-y-3">
                {d.skus.map((sku) => (
                  <div key={sku.id} class="rounded-zs border border-line bg-surface-solid/40 p-3">
                    <SettingRuleRow
                      label={t('admin.resellerProductSettings.columns.sku')}
                      name={skuLabel(sku)}
                      form={s.skuForm(sku.id)}
                      preview={s.preview[sku.id]}
                      hint={previewHint(sku.id)}
                    />
                  </div>
                ))}
              </div>
            ) : (
              <p class="rounded-zs border border-dashed border-line px-4 py-6 text-center text-sm text-muted">{t('admin.resellerProductSettings.empty')}</p>
            )}
          </Card>
        </div>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.resellerProductSettings.title')} subtitle={t('admin.resellerProductSettings.subtitle')} />
        <FilterBar cols={6}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={filters.keyword}
                  placeholder={t('admin.resellerProductSettings.filters.keyword')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input
                  v-model={filters.resellerId}
                  placeholder={t('admin.resellerProductSettings.filters.resellerId')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input v-model={filters.userId} placeholder={t('admin.resellerProductSettings.filters.userId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input
                  v-model={filters.productId}
                  placeholder={t('admin.resellerProductSettings.filters.productId')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Select
                  v-model={filters.pricingMode}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: `${t('admin.resellerProductSettings.filters.pricingMode')} · ${t('admin.resellerProductSettings.filters.all')}`,
                      value: '__all__',
                    },
                    ...RESELLER_PRICING_MODES.map((m) => ({
                      label: modeLabel(m),
                      value: m,
                    })),
                  ]}
                />
                <Select
                  v-model={filters.listed}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: `${t('admin.resellerProductSettings.filters.listed')} · ${t('admin.resellerProductSettings.filters.all')}`,
                      value: '__all__',
                    },
                    {
                      label: t('admin.resellerProductSettings.status.listed'),
                      value: 'listed',
                    },
                    {
                      label: t('admin.resellerProductSettings.status.hidden'),
                      value: 'hidden',
                    },
                  ]}
                />
              </>
            ),
            actions: () => <RefreshButton loading={s.refreshing.value} onClick={s.refresh} />,
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.resellerProductSettings.empty')}
            minWidth="1200px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>

        <Dialog
          modelValue={s.showEditor.value}
          onUpdate:modelValue={(v: boolean) => !v && s.closeEditor()}
          title={`${t('admin.resellerProductSettings.actions.edit')} · R#${s.scope.value?.resellerId ?? '-'} / ${detailTitle()}`}
          size="full"
          closeOnOverlay={false}
        >
          {{
            default: renderEditor,
            footer: () => (
              <>
                <Button disabled={s.saving.value} onClick={s.closeEditor}>
                  {t('admin.common.cancel')}
                </Button>
                <Button variant="primary" loading={s.saving.value} onClick={s.save}>
                  {s.saving.value ? t('admin.resellerProductSettings.actions.saving') : t('admin.resellerProductSettings.actions.save')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
