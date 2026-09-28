import { computed, defineComponent, toRef, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Plus, Trash2, X } from 'lucide-vue-next'
import { Button, Dialog, FormField, Input, Loader, Select, Switch, Tabs, Textarea, cn } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { RichEditor } from '@/components/RichEditor'
import type { AdminCategory } from '@/api/types'
import { useProductEditor } from '../useProductEditor'
import { MANUAL_FIELD_TYPES, PRODUCT_LOCALES, STOCK_DISPLAY_MODES, isOptionField, isTextLikeField, type ManualFormField, type SKUFormItem } from '../productUtils'

const sectionClass = 'md:col-span-2 space-y-4 rounded-zs border border-line bg-surface-muted/50 p-4'
const itemClass = 'space-y-3 rounded-zs-sm border border-line bg-surface-solid p-4'

export default defineComponent({
  name: 'ProductEditModal',
  props: {
    modelValue: { type: Boolean, default: false },
    productId: { type: Number as PropType<number | null>, default: null },
    categories: { type: Array as PropType<AdminCategory[]>, default: () => [] },
  },
  emits: { 'update:modelValue': (_v: boolean) => true, success: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const close = () => emit('update:modelValue', false)
    const e = useProductEditor({
      open: toRef(props, 'modelValue'),
      productId: toRef(props, 'productId'),
      categories: toRef(props, 'categories'),
      onClose: close,
      onSuccess: () => emit('success'),
    })
    const f = e.form
    const mapped = computed(() => e.isMapped.value)

    const langName = () =>
      ({ 'zh-CN': t('admin.common.lang.zhCN'), 'zh-TW': t('admin.common.lang.zhTW'), 'en-US': t('admin.common.lang.enUS') })[e.currentLang.value]
    const langTabs = () => PRODUCT_LOCALES.map((code) => ({ key: code, label: ({ 'zh-CN': t('admin.common.lang.zhCN'), 'zh-TW': t('admin.common.lang.zhTW'), 'en-US': t('admin.common.lang.enUS') })[code] }))
    const L = () => e.currentLang.value

    const renderManualField = (field: ManualFormField, index: number) => (
      <div key={index} class={itemClass}>
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-muted">{t('admin.products.form.manualFormFieldTitle', { index: index + 1 })}</span>
          {!mapped.value && (
            <Button size="xs" variant="danger" onClick={() => e.removeManualField(index)}>
              <Trash2 class="h-3.5 w-3.5" />
              {t('admin.products.form.manualFormRemoveField')}
            </Button>
          )}
        </div>
        <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
          <FormField label={t('admin.products.form.manualFormFieldKey')}>
            <Input v-model={field.key} placeholder={t('admin.products.form.manualFormFieldKeyPlaceholder')} disabled={mapped.value} mono />
          </FormField>
          <FormField label={t('admin.products.form.manualFormFieldType')}>
            <Select
              v-model={field.type}
              disabled={mapped.value}
              options={MANUAL_FIELD_TYPES.map((v) => ({ value: v, label: t(`admin.products.form.manualFormFieldTypes.${v}`) }))}
            />
          </FormField>
          <div class="flex items-end pb-1.5">
            <Switch v-model={field.required} disabled={mapped.value} label={t('admin.products.form.manualFormFieldRequired')} />
          </div>
        </div>
        <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
          <FormField label={t('admin.products.form.manualFormFieldLabel', { lang: langName() })}>
            <Input v-model={field.label[L()]} placeholder={t('admin.products.form.manualFormFieldLabelPlaceholder')} disabled={mapped.value} />
          </FormField>
          <FormField label={t('admin.products.form.manualFormFieldPlaceholder', { lang: langName() })}>
            <Input v-model={field.placeholder[L()]} placeholder={t('admin.products.form.manualFormFieldPlaceholderPlaceholder')} disabled={mapped.value} />
          </FormField>
        </div>
        {isTextLikeField(field.type) && (
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FormField label={t('admin.products.form.manualFormFieldRegex')}>
              <Input v-model={field.regex} placeholder={t('admin.products.form.manualFormFieldRegexPlaceholder')} disabled={mapped.value} mono />
            </FormField>
            <FormField label={t('admin.products.form.manualFormFieldMaxLength')}>
              <Input
                modelValue={field.max_len}
                onUpdate:modelValue={(v) => (field.max_len = String(v))}
                type="number"
                min={1}
                placeholder={t('admin.products.form.manualFormFieldMaxLengthPlaceholder')}
                disabled={mapped.value}
              />
            </FormField>
          </div>
        )}
        {field.type === 'number' && (
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FormField label={t('admin.products.form.manualFormFieldMin')}>
              <Input modelValue={field.min} onUpdate:modelValue={(v) => (field.min = String(v))} type="number" placeholder={t('admin.products.form.manualFormFieldMinPlaceholder')} disabled={mapped.value} />
            </FormField>
            <FormField label={t('admin.products.form.manualFormFieldMax')}>
              <Input modelValue={field.max} onUpdate:modelValue={(v) => (field.max = String(v))} type="number" placeholder={t('admin.products.form.manualFormFieldMaxPlaceholder')} disabled={mapped.value} />
            </FormField>
          </div>
        )}
        {isOptionField(field.type) && (
          <FormField label={t('admin.products.form.manualFormFieldOptions')} hint={t('admin.products.form.manualFormFieldOptionsTip')}>
            <Textarea v-model={field.options_text} rows={4} placeholder={t('admin.products.form.manualFormFieldOptionsPlaceholder')} disabled={mapped.value} />
          </FormField>
        )}
      </div>
    )

    const renderSku = (sku: SKUFormItem, index: number) => (
      <div key={`sku-${index}-${sku.id || 0}`} class={itemClass}>
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-muted">{t('admin.products.form.skuItemTitle', { index: index + 1 })}</span>
          {!mapped.value && (
            <Button size="xs" variant="danger" onClick={() => e.removeSKU(index)}>
              <Trash2 class="h-3.5 w-3.5" />
              {t('admin.products.form.skuRemove')}
            </Button>
          )}
        </div>
        <div class="grid grid-cols-1 gap-3 md:grid-cols-6">
          <FormField label={t('admin.products.form.skuCode')}>
            <Input v-model={sku.sku_code} placeholder={t('admin.products.form.skuCodePlaceholder')} disabled={mapped.value} mono />
          </FormField>
          <div class={f.fulfillment_type === 'manual' ? 'md:col-span-1' : 'md:col-span-2'}>
            <FormField label={t('admin.products.form.skuSpec', { lang: langName() })}>
              <Input v-model={sku.spec_values[L()]} placeholder={t('admin.products.form.skuSpecPlaceholder')} disabled={mapped.value} />
            </FormField>
          </div>
          <FormField label={t('admin.products.form.skuPrice')}>
            <Input v-model={sku.price_amount} type="number" step="0.01" min={0} placeholder={t('admin.products.form.skuPricePlaceholder')} />
          </FormField>
          <FormField label={t('admin.products.form.skuCostPrice')} hint={mapped.value ? t('admin.products.form.costPriceAutoFromUpstream') : undefined}>
            <Input v-model={sku.cost_price_amount} type="number" step="0.01" min={0} placeholder={t('admin.products.form.skuCostPricePlaceholder')} disabled={mapped.value} />
          </FormField>
          {f.fulfillment_type === 'manual' && (
            <FormField label={t('admin.products.form.skuManualStock')}>
              <Input v-model={sku.manual_stock_total} type="number" min={-1} placeholder={t('admin.products.form.skuManualStockPlaceholder')} />
            </FormField>
          )}
          <FormField label={t('admin.products.form.skuSort')} hint={t('admin.products.form.skuSortTip')}>
            <Input v-model={sku.sort_order} type="number" placeholder={t('admin.products.form.skuSortPlaceholder')} />
          </FormField>
        </div>
        <Switch v-model={sku.is_active} label={t('admin.products.form.skuActive')} />
      </div>
    )

    const body = () => {
      if (e.loading.value) return <Loader />
      return (
        <form
          class="space-y-6"
          onSubmit={(ev: Event) => {
            ev.preventDefault()
            void e.submit()
          }}
        >
          <Tabs
            modelValue={e.currentLang.value}
            onUpdate:modelValue={(v) => (e.currentLang.value = v as typeof e.currentLang.value)}
            items={langTabs()}
            variant="underline"
          />
          <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
            <div class="md:col-span-2">
              <FormField label={t('admin.products.form.title', { lang: langName() })}>
                <Input v-model={f.title[L()]} placeholder={t('admin.products.form.titlePlaceholder')} />
              </FormField>
            </div>
            <FormField label={t('admin.products.form.slug')} hint={t('admin.products.form.slugTip')}>
              <Input v-model={f.slug} placeholder={t('admin.products.form.slugPlaceholder')} mono />
            </FormField>
            <FormField label={t('admin.products.form.seoMetaKeywords', { lang: langName() })} hint={t('admin.products.form.seoMetaKeywordsTip')}>
              <Input v-model={f.seo_meta.keywords[L()]} placeholder={t('admin.products.form.seoMetaKeywordsPlaceholder')} />
            </FormField>
            <div class="md:col-span-2">
              <FormField label={t('admin.products.form.seoMetaDescription', { lang: langName() })} hint={t('admin.products.form.seoMetaDescriptionTip')}>
                <Textarea v-model={f.seo_meta.description[L()]} rows={3} placeholder={t('admin.products.form.seoMetaDescriptionPlaceholder')} />
              </FormField>
            </div>
            <FormField label={t('admin.products.form.category')} hint={e.localeTip('categoryLeafTip')}>
              <Select
                modelValue={f.category_id ?? '__none__'}
                onUpdate:modelValue={(v) => (f.category_id = v && v !== '__none__' ? Number(v) : null)}
                options={e.categoryOptions.value}
              />
            </FormField>
            <FormField label={t('admin.products.form.purchaseType')}>
              <Select
                v-model={f.purchase_type}
                options={[
                  { value: 'member', label: t('admin.products.purchaseType.member') },
                  { value: 'guest', label: t('admin.products.purchaseType.guest') },
                ]}
              />
            </FormField>
            <FormField label={t('admin.products.form.minPurchaseQuantity')} hint={t('admin.products.form.minPurchaseQuantityTip')}>
              <Input v-model={f.min_purchase_quantity} type="number" min={1} placeholder={t('admin.products.form.minPurchaseQuantityPlaceholder')} />
            </FormField>
            <FormField label={t('admin.products.form.maxPurchaseQuantity')} hint={t('admin.products.form.maxPurchaseQuantityTip')}>
              <Input v-model={f.max_purchase_quantity} type="number" min={1} placeholder={t('admin.products.form.maxPurchaseQuantityPlaceholder')} />
            </FormField>
            <FormField label={t('admin.products.form.stockDisplayMode')} hint={t('admin.products.form.stockDisplayModeTip')}>
              <Select v-model={f.stock_display_mode} options={STOCK_DISPLAY_MODES.map((v) => ({ value: v, label: t(`admin.products.form.stockDisplayModes.${v}`) }))} />
            </FormField>
            <FormField label={t('admin.products.form.fulfillmentType')}>
              {{
                default: () => (
                  <Select
                    v-model={f.fulfillment_type}
                    disabled={mapped.value}
                    options={[
                      { value: 'manual', label: t('admin.products.fulfillmentType.manual') },
                      { value: 'auto', label: t('admin.products.fulfillmentType.auto') },
                    ]}
                  />
                ),
                hint: mapped.value ? () => <span class="text-secondary">{t('admin.products.mappedFulfillmentLocked')}</span> : undefined,
              }}
            </FormField>
            <FormField
              label={t('admin.products.form.manualStockTotal')}
              hint={f.skus.length > 0 ? t('admin.products.form.manualStockTotalSkuTip') : t('admin.products.form.manualStockTotalTip')}
            >
              <Input v-model={f.manual_stock_total} type="number" min={-1} placeholder={t('admin.products.form.manualStockTotalPlaceholder')} disabled={f.skus.length > 0} />
            </FormField>

            {(f.fulfillment_type === 'manual' || mapped.value) && (
              <div class={sectionClass}>
                <div class="flex items-start justify-between gap-3">
                  <div>
                    <h3 class="text-sm font-semibold text-fg">{t('admin.products.form.manualFormSchemaTitle')}</h3>
                    <p class="mt-1 text-xs text-muted">{t('admin.products.form.manualFormSchemaTip')}</p>
                    {mapped.value && <p class="mt-1 text-xs text-secondary">{t('admin.products.mappedFormSchemaLocked')}</p>}
                  </div>
                  {!mapped.value && (
                    <Button size="sm" onClick={e.addManualField}>
                      <Plus class="h-3.5 w-3.5" />
                      {t('admin.products.form.manualFormAddField')}
                    </Button>
                  )}
                </div>
                {!f.manual_form_schema.fields.length && (
                  <div class="rounded-zs-sm border border-dashed border-line-strong p-4 text-xs text-muted">{t('admin.products.form.manualFormEmpty')}</div>
                )}
                {f.manual_form_schema.fields.map(renderManualField)}
              </div>
            )}

            <div class={sectionClass}>
              <div class="flex items-start justify-between gap-3">
                <div>
                  <h3 class="text-sm font-semibold text-fg">{t('admin.products.form.skuTitle')}</h3>
                  <p class="mt-1 text-xs text-muted">{t('admin.products.form.skuTip')}</p>
                  {mapped.value && <p class="mt-1 text-xs text-secondary">{t('admin.products.mappedSkuLocked')}</p>}
                </div>
                {!mapped.value && (
                  <Button size="sm" onClick={e.addSKU}>
                    <Plus class="h-3.5 w-3.5" />
                    {t('admin.products.form.skuAdd')}
                  </Button>
                )}
              </div>
              {!f.skus.length && <div class="rounded-zs-sm border border-dashed border-line-strong p-4 text-xs text-muted">{t('admin.products.form.skuEmpty')}</div>}
              {f.skus.map(renderSku)}
            </div>

            <FormField
              label={t('admin.products.form.priceAmount')}
              hint={f.skus.length > 0 ? t('admin.products.form.priceAmountSkuTip') : t('admin.products.form.priceAmountTip')}
            >
              <Input v-model={f.price_amount} type="number" step="0.01" min={0} placeholder={t('admin.products.form.priceAmountPlaceholder')} disabled={f.skus.length > 0} />
            </FormField>
            <FormField
              label={t('admin.products.form.costPriceAmount')}
              hint={f.skus.length > 0 ? t('admin.products.form.costPriceAmountSkuTip') : mapped.value ? t('admin.products.form.costPriceAutoFromUpstream') : undefined}
            >
              <Input
                v-model={f.cost_price_amount}
                type="number"
                step="0.01"
                min={0}
                placeholder={t('admin.products.form.costPriceAmountPlaceholder')}
                disabled={f.skus.length > 0 || mapped.value}
              />
            </FormField>
            <FormField label={t('admin.products.form.sortOrder')} hint={t('admin.products.form.sortTip')}>
              <Input v-model={f.sort_order} type="number" placeholder="0" />
            </FormField>

            <div class="md:col-span-2">
              <FormField label={t('admin.products.form.images')}>
                <MediaPicker modelValue={f.images} onUpdate:modelValue={(v) => (f.images = Array.isArray(v) ? v : v ? [v] : [])} multiple scene="product" />
              </FormField>
            </div>
            <div class="md:col-span-2">
              <FormField label={t('admin.products.form.description', { lang: langName() })}>
                <Textarea v-model={f.description[L()]} rows={3} placeholder={t('admin.products.form.descriptionPlaceholder')} />
              </FormField>
            </div>
            <div class="md:col-span-2">
              <FormField label={t('admin.products.form.content', { lang: langName() })} hint={t('admin.products.form.contentTip')}>
                <RichEditor
                  key={`content-${L()}`}
                  modelValue={f.content[L()] || ''}
                  onUpdate:modelValue={(v: string) => (f.content[L()] = v)}
                  placeholder={t('admin.products.form.contentPlaceholder')}
                />
              </FormField>
            </div>
            <div class="md:col-span-2">
              <FormField label={t('admin.products.form.instructions', { lang: langName() })} hint={t('admin.products.form.instructionsTip')}>
                <RichEditor
                  key={`instructions-${L()}`}
                  modelValue={f.instructions[L()] || ''}
                  onUpdate:modelValue={(v: string) => (f.instructions[L()] = v)}
                  placeholder={t('admin.products.form.instructionsPlaceholder')}
                />
              </FormField>
            </div>

            <div class="md:col-span-2">
              <FormField label={t('admin.products.form.tags')}>
                <div class="space-y-2">
                  {f.tags.length > 0 && (
                    <div class="flex flex-wrap gap-2">
                      {f.tags.map((tag, index) => (
                        <span key={`${tag}-${index}`} class="inline-flex items-center gap-1 rounded-full border border-line bg-primary-soft px-3 py-1 text-xs text-primary">
                          # {tag}
                          <button type="button" class="rounded-full p-0.5 hover:bg-surface-solid" aria-label="remove" onClick={() => e.removeTag(index)}>
                            <X class="h-3 w-3" />
                          </button>
                        </span>
                      ))}
                    </div>
                  )}
                  <div class="flex flex-col gap-2 sm:flex-row">
                    <Input
                      v-model={e.newTag.value}
                      placeholder={t('admin.products.form.tagsPlaceholder')}
                      onKeydown={(ev: KeyboardEvent) => {
                        if (ev.key === 'Enter' && !ev.isComposing) {
                          ev.preventDefault()
                          e.addTag()
                        }
                      }}
                    />
                    <Button class="shrink-0" onClick={e.addTag}>
                      {t('admin.products.actions.addTag')}
                    </Button>
                  </div>
                </div>
              </FormField>
            </div>

            {e.paymentChannels.value.length > 0 && (
              <div class="md:col-span-2">
                <FormField label={t('admin.products.form.paymentChannels')} hint={t('admin.products.form.paymentChannelsTip')}>
                  <div class="flex flex-wrap gap-2">
                    {e.paymentChannels.value.map((ch) => {
                      const on = f.payment_channel_ids.includes(ch.id)
                      return (
                        <button
                          key={ch.id}
                          type="button"
                          aria-pressed={on}
                          class={cn(
                            'inline-flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs transition-colors',
                            on ? 'border-primary bg-primary-soft text-primary' : 'border-line text-muted hover:border-primary hover:text-primary',
                          )}
                          onClick={() => e.togglePaymentChannel(ch.id)}
                        >
                          <span class={cn('flex h-3.5 w-3.5 items-center justify-center rounded-[4px] border', on ? 'zs-gradient-bg border-transparent text-on-primary' : 'border-line-strong')}>
                            {on && <Check class="h-2.5 w-2.5" />}
                          </span>
                          {ch.name}
                        </button>
                      )
                    })}
                  </div>
                </FormField>
              </div>
            )}

            <div class="flex flex-col items-start gap-4 border-t border-line pt-4 sm:flex-row sm:flex-wrap sm:items-center sm:gap-6 md:col-span-2">
              <Switch v-model={f.is_affiliate_enabled} label={t('admin.products.form.affiliateEnabled')} />
              <Switch v-model={f.is_active} label={t('admin.products.form.activeNow')} />
            </div>
          </div>
        </form>
      )
    }

    return () => (
      <Dialog
        modelValue={props.modelValue}
        onUpdate:modelValue={(v) => !v && close()}
        title={e.isEditing.value ? t('admin.products.modal.editTitle') : t('admin.products.modal.createTitle')}
        size="2xl"
        closeOnOverlay={false}
      >
        {{
          default: body,
          footer: () => (
            <>
              <Button onClick={close}>{t('admin.common.cancel')}</Button>
              <Button variant="primary" loading={e.submitting.value} disabled={e.loading.value} onClick={() => void e.submit()}>
                {e.submitting.value
                  ? t('admin.products.actions.submitting')
                  : e.isEditing.value
                    ? t('admin.products.actions.saveChanges')
                    : t('admin.products.actions.createNow')}
              </Button>
            </>
          ),
        }}
      </Dialog>
    )
  },
})
