import { computed, defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import type { LucideIcon } from 'lucide-vue-next'
import { ExternalLink, Pencil, Search, ShoppingBag, SlidersHorizontal, Tag, ToggleLeft } from 'lucide-vue-next'
import type { ResellerProductSettingDetailData } from '@/api/types'
import { useResellerProductSettings } from '@/composables/reseller/useResellerProductSettings'
import { useLocalized } from '@/composables/useLocalized'
import { ResellerAlert, ResellerPageHeader } from '@/components/reseller/ConsoleParts'
import { ResellerRuleEditor } from '@/components/reseller/ResellerFormParts'
import { Badge, Button, Card, DataTable, Input, Modal, Pagination, PetalLoader, columns } from '@/components/ui'
import { countActiveSkus, countListedSkus, summarizeProductEffectivePrice } from '@/utils/reseller/productSettings'
import { formatSkuSpecValues } from '@/utils/sku'

export default defineComponent({
  name: 'ResellerProducts',
  setup() {
    const { t, locale } = useI18n()
    const { getLocalizedText } = useLocalized()
    const p = useResellerProductSettings()
    const k = (key: string) => t(`personalCenter.reseller.productSettings.${key}`)
    onMounted(() => {
      if (!p.profile.snapshot.value) void p.profile.load()
      void p.loadRows(1)
    })

    const hints: Array<{ title: string; description: string; icon: LucideIcon }> = [
      { title: 'resellerConsole.products.hints.listing', description: 'resellerConsole.products.hints.listingDescription', icon: ToggleLeft },
      { title: 'resellerConsole.products.hints.pricing', description: 'resellerConsole.products.hints.pricingDescription', icon: SlidersHorizontal },
      { title: 'resellerConsole.products.hints.sku', description: 'resellerConsole.products.hints.skuDescription', icon: Tag },
    ]
    const profileData = computed(() => p.profile.snapshot.value?.profile)

    const productColumns = columns<ResellerProductSettingDetailData>([
      {
        key: 'title',
        title: t('resellerConsole.orderDetail.product'),
        render: (row) => (
          <div class="min-w-0">
            <div class="truncate font-bold text-fg">{getLocalizedText(row.product.title) || row.product.slug}</div>
            <div class="font-mono text-xs text-muted">
              #{row.product.id} · {row.product.slug}
            </div>
          </div>
        ),
      },
      { key: 'base', title: k('basePrice'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num">{row.product.price_amount}</span> },
      { key: 'effective', title: k('effectivePrice'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num font-bold text-primary-text">{summarizeProductEffectivePrice(row)}</span> },
      {
        key: 'listed',
        title: k('listedSkuCount'),
        render: (row) => {
          const listed = countListedSkus(row)
          return <Badge tone={listed > 0 ? 'success' : 'neutral'}>{listed > 0 ? `${k('displayed')} ${listed}/${countActiveSkus(row)}` : k('hidden')}</Badge>
        },
      },
      {
        key: 'actions',
        title: '',
        align: 'right',
        render: (row) => (
          <Button size="xs" variant="soft" onClick={() => void p.openEditor(row.product.id)}>
            <Pencil class="size-3.5" />
            {k('edit')}
          </Button>
        ),
      },
    ])

    const skuLabel = (sku: ResellerProductSettingDetailData['skus'][number]) => {
      const spec = formatSkuSpecValues(sku.spec_values, String(locale.value))
      return `${k('skuLevelRule')} · ${spec || sku.sku_code || `#${sku.id}`}`
    }

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.products.title')} description={t('resellerConsole.products.description')}>
          {{
            actions: () => (
              <>
                {p.profile.primaryDomainUrl.value && (
                  <Button variant="secondary" size="sm" href={p.profile.primaryDomainUrl.value} target="_blank">
                    <ExternalLink class="size-4" />
                    {t('resellerConsole.products.previewStore')}
                  </Button>
                )}
                <Button variant="secondary" size="sm" to="/reseller/orders">
                  <ShoppingBag class="size-4" />
                  {t('resellerConsole.products.viewOrders')}
                </Button>
              </>
            ),
          }}
        </ResellerPageHeader>

        <div class="grid gap-4 md:grid-cols-3">
          <Card padding="sm">
            <div class="text-xs font-bold text-muted">{t('resellerConsole.products.cards.defaultMarkup')}</div>
            <div class="zs-num mt-1 text-2xl font-bold text-fg">{profileData.value?.default_markup_percent ?? '-'}%</div>
            <p class="mt-1 text-xs text-muted">{t('resellerConsole.products.cards.defaultMarkupDescription')}</p>
          </Card>
          <Card padding="sm">
            <div class="text-xs font-bold text-muted">{t('resellerConsole.products.cards.maxMarkup')}</div>
            <div class="zs-num mt-1 text-2xl font-bold text-fg">{profileData.value?.max_markup_percent ?? '-'}%</div>
            <p class="mt-1 text-xs text-muted">{t('resellerConsole.products.cards.maxMarkupDescription')}</p>
          </Card>
          <Card padding="sm">
            <div class="text-xs font-bold text-muted">{t('resellerConsole.products.cards.storefront')}</div>
            <div class="mt-1 truncate font-mono text-sm font-bold text-fg">{p.profile.primaryDomain.value || t('resellerConsole.products.cards.noPrimaryDomain')}</div>
            <p class="mt-1 text-xs text-muted">{t('resellerConsole.products.cards.storefrontDescription')}</p>
          </Card>
        </div>

        <div class="grid gap-3 md:grid-cols-3">
          {hints.map((h) => {
            const Icon = h.icon
            return (
              <div key={h.title} class="flex items-start gap-3 rounded-zs border border-line bg-surface p-4">
                <span class="flex size-9 shrink-0 items-center justify-center rounded-zs-sm bg-secondary-soft text-secondary-text">
                  <Icon class="size-4" />
                </span>
                <div>
                  <div class="text-sm font-bold text-fg">{t(h.title)}</div>
                  <p class="mt-0.5 text-xs text-muted">{t(h.description)}</p>
                </div>
              </div>
            )
          })}
        </div>

        <ResellerAlert alert={p.editorOpen.value ? null : p.alert.value} />

        <Card>
          <div class="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h3 class="zs-title text-lg text-fg">{k('title')}</h3>
              <p class="text-xs text-muted">{k('subtitle')}</p>
            </div>
            <form
              class="flex gap-2"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void p.loadRows(1)
              }}
            >
              <Input v-model={p.keyword.value} size="sm" placeholder={k('searchPlaceholder')} class="w-full sm:w-72">
                {{ prefix: () => <Search class="size-4" /> }}
              </Input>
              <Button type="submit" size="sm">
                {t('zsReseller.search')}
              </Button>
            </form>
          </div>
          <DataTable columns={productColumns} rows={p.rows.value} rowKey={(row: ResellerProductSettingDetailData) => row.product.id} loading={p.loading.value} emptyText={k('empty')} />
          <div class="mt-4">
            <Pagination page={p.pagination.page} totalPages={p.pagination.total_page} onChange={(pg: number) => void p.loadRows(pg)} />
          </div>
        </Card>

        <Modal open={p.editorOpen.value} size="lg" title={p.editing.value ? `${k('editorTitle')} · ${getLocalizedText(p.editing.value.product.title)}` : ''} onClose={p.closeEditor}>
          {{
            default: () =>
              p.editing.value ? (
                <div class="space-y-4">
                  <ResellerAlert alert={p.alert.value} />
                  {p.detailLoading.value && <PetalLoader size="sm" />}
                  <ResellerRuleEditor
                    label={k('productLevelRule')}
                    basePrice={p.editing.value.product.price_amount}
                    effectivePrice={p.preview[0]?.effective || summarizeProductEffectivePrice(p.editing.value, '')}
                    invalid={p.preview[0]?.valid === false}
                    errorCode={p.preview[0]?.errorCode || ''}
                    modelValue={p.productForm.value}
                    onUpdate:modelValue={(v: typeof p.productForm.value) => {
                      p.productForm.value = v
                    }}
                  />
                  {p.editing.value.skus.map((sku) => (
                    <ResellerRuleEditor
                      key={sku.id}
                      label={skuLabel(sku)}
                      basePrice={sku.base_price_amount}
                      effectivePrice={p.preview[sku.id]?.effective || sku.effective_price_amount || sku.setting?.effective_price_amount || ''}
                      invalid={p.preview[sku.id]?.valid === false}
                      errorCode={p.preview[sku.id]?.errorCode || ''}
                      modelValue={p.skuFormFor(sku.id)}
                      onUpdate:modelValue={(v: typeof p.productForm.value) => {
                        p.skuForms[sku.id] = v
                      }}
                    />
                  ))}
                </div>
              ) : null,
            footer: () => (
              <>
                <Button variant="ghost" disabled={p.saving.value} onClick={() => void p.resetProductRule()}>
                  {k('resetProductRule')}
                </Button>
                <Button variant="secondary" onClick={p.closeEditor}>
                  {t('common.cancel')}
                </Button>
                <Button loading={p.saving.value} onClick={() => void p.save()}>
                  {p.saving.value ? k('saving') : k('save')}
                </Button>
              </>
            ),
          }}
        </Modal>
      </div>
    )
  },
})
