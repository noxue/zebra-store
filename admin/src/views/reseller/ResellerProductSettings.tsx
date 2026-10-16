import { defineComponent, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, Search } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, Dialog, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, Switch, type DataTableColumn } from '@/components/ui'
import { RichEditor } from '@/components/RichEditor'
import type { AdminResellerProductSetting, AdminResellerProfile } from '@/api/types'
import { adminAPI } from '@/api/admin'
import { formatDate, getLocalizedText } from '@/utils/format'
import { getAdminResellerProductSettingOwnerLabel } from '@/utils/resellerProductSettings'
import { RefreshButton } from './components/RefreshButton'
import { SettingRuleRow } from './components/SettingRuleRow'
import { SearchPicker, type SearchOption } from './components/SearchPicker'
import { RESELLER_PRICING_MODES, pricingModeKey, pricingValue, skuLabel } from './resellerUtils'
import { useResellerProductSettings } from './useResellerProductSettings'

export default defineComponent({
  name: 'ResellerProductSettingsView',
  setup() {
    const { t } = useI18n()
    const s = useResellerProductSettings()
    const editLocale = ref<'zh-CN' | 'zh-TW' | 'en-US'>('zh-CN')
    const selectedReseller = ref<SearchOption | null>(null)
    const selectedProduct = ref<SearchOption | null>(null)
    const filterReseller = ref<SearchOption | null>(null)
    const filterProduct = ref<SearchOption | null>(null)
    const pickerLoading = ref(false)
    const { filters, list } = s
    onMounted(() => {
      void list.fetchData(1)
      if (filters.resellerId) {
        void adminAPI.getResellerProfile(filters.resellerId).then((res) => {
          if (res.data?.profile) filterReseller.value = profileOption(res.data.profile)
        }).catch(() => { /* the list still uses the route's reseller filter */ })
      }
    })

    const modeLabel = (m?: string) => t(`admin.resellerProductSettings.modes.${pricingModeKey(m)}`)
    const pickerText = (key: string) => t(`admin.resellerProductSettings.picker.${key}`)
    const profileOption = (profile: AdminResellerProfile): SearchOption => ({
      id: profile.id,
      label: `${profile.user?.display_name || profile.user?.username || profile.user?.email || `R#${profile.id}`} · ${profile.user?.email || `R#${profile.id}`} (R#${profile.id})`,
    })
    const searchResellers = async (query: string): Promise<SearchOption[]> => {
      const res = await adminAPI.getResellerProfiles({ page: 1, page_size: 20, keyword: query })
      return (res.data ?? []).map(profileOption)
    }
    const searchProducts = async (query: string): Promise<SearchOption[]> => {
      const res = await adminAPI.getProducts({ page: 1, page_size: 20, search: query })
      return (res.data ?? []).map((product) => ({
        id: product.id,
        label: `${getLocalizedText(product.title) || product.slug} · #${product.id}`,
      }))
    }
    const picker = (value: SearchOption | null, search: (query: string) => Promise<SearchOption[]>, placeholder: string, update: (option: SearchOption | null) => void) => (
      <SearchPicker modelValue={value} search={search} placeholder={placeholder} emptyText={pickerText('noResults')} loadingText={pickerText('loading')} onUpdate:modelValue={update} />
    )
    const openSelected = async () => {
      if (!selectedReseller.value || !selectedProduct.value) return
      pickerLoading.value = true
      try { await s.openBySelection(selectedReseller.value.id, selectedProduct.value.id) }
      finally { pickerLoading.value = false }
    }
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
          <Card title={t('admin.resellerProductSettings.contentOverrides')}>
            <div class="space-y-4">
              <div class="flex gap-2">
                {(['zh-CN', 'zh-TW', 'en-US'] as const).map((lang) => (
                  <Button key={lang} size="xs" variant={editLocale.value === lang ? 'primary' : 'soft'} onClick={() => (editLocale.value = lang)}>{lang}</Button>
                ))}
              </div>
              {(['content', 'instructions'] as const).map((field) => (
                <div key={field} class="space-y-2">
                  <Switch
                    modelValue={s.productForm.value[field] !== null}
                    onUpdate:modelValue={(enabled: boolean) => (s.productForm.value[field] = enabled ? { 'zh-CN': '', 'zh-TW': '', 'en-US': '' } : null)}
                    label={t(field === 'content' ? 'admin.resellerProductSettings.overrideContent' : 'admin.resellerProductSettings.overrideInstructions')}
                  />
                  {s.productForm.value[field] !== null && (
                    <RichEditor
                      key={`${field}-${editLocale.value}`}
                      modelValue={s.productForm.value[field]?.[editLocale.value] || ''}
                      onUpdate:modelValue={(value: string) => (s.productForm.value[field] = { ...s.productForm.value[field], [editLocale.value]: value })}
                    />
                  )}
                </div>
              ))}
              <p class="text-xs text-muted">{t('admin.resellerProductSettings.overrideHint')}</p>
            </div>
          </Card>
          <Card title={t('admin.resellerProductSettings.columns.sku')}>
            {d.skus.length ? (
              <div class="space-y-3">
                {d.skus.map((sku) => (
                  <div key={sku.id} class="rounded-zs border border-line bg-surface-solid/40 p-3">
                    <SettingRuleRow
                      label={t('admin.resellerProductSettings.columns.sku')}
                      name={skuLabel(sku)}
                      sharedBase={String(sku.shared_agent_price_amount ?? sku.base_price_amount)}
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
                {picker(filterReseller.value, searchResellers, pickerText('resellerSearch'), (option) => {
                  filterReseller.value = option
                  filters.resellerId = option ? String(option.id) : ''
                  filters.userId = ''
                  list.handleSearch()
                })}
                {picker(filterProduct.value, searchProducts, pickerText('productSearch'), (option) => {
                  filterProduct.value = option
                  filters.productId = option ? String(option.id) : ''
                  list.handleSearch()
                })}
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
            actions: () => (
              <div class="flex gap-2">
                <Button variant="primary" onClick={() => (s.showPicker.value = true)}><Plus class="size-4" />{pickerText('configure')}</Button>
                <RefreshButton loading={s.refreshing.value} onClick={s.refresh} />
              </div>
            ),
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

        <Dialog modelValue={s.showPicker.value} onUpdate:modelValue={(value: boolean) => (s.showPicker.value = value)} title={pickerText('title')}>
          {{
            default: () => <div class="space-y-4">
              <div class="space-y-1.5"><label class="text-sm font-medium text-fg">{pickerText('reseller')}</label>
                {picker(selectedReseller.value, searchResellers, pickerText('resellerSearch'), (option) => (selectedReseller.value = option))}
              </div>
              <div class="space-y-1.5"><label class="text-sm font-medium text-fg">{pickerText('product')}</label>
                {picker(selectedProduct.value, searchProducts, pickerText('productSearch'), (option) => (selectedProduct.value = option))}
              </div>
            </div>,
            footer: () => <>
              <Button onClick={() => (s.showPicker.value = false)}>{t('admin.common.cancel')}</Button>
              <Button variant="primary" disabled={!selectedReseller.value || !selectedProduct.value} loading={pickerLoading.value} onClick={openSelected}>{pickerText('continue')}</Button>
            </>,
          }}
        </Dialog>
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
