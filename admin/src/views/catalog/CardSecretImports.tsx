import { computed, defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { PackagePlus, Upload } from 'lucide-vue-next'
import { Button, PageHeader } from '@/components/ui'
import ProductSkuPicker from './components/ProductSkuPicker'
import CardSecretBatchCreateForm from './components/CardSecretBatchCreateForm'
import { useProductSkuPicker } from './useProductSkuPicker'

export default defineComponent({
  name: 'CardSecretImportsView',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const pk = useProductSkuPicker({ autoSelectSingleSku: true })
    onMounted(() => void pk.loadOptions())

    const hint = computed(() =>
      pk.productId.value ? t('admin.cardSecrets.productHintCurrent', { id: pk.productId.value }) : t('admin.cardSecretImports.selectionTip'),
    )
    const goInventory = () => void router.push('/card-secrets')
    const onProductChange = () => {
      pk.skuValue.value = '__all__'
      void pk.loadProductInfo()
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.cardSecretImports.title')} subtitle={t('admin.cardSecretImports.subtitle')}>
          {{
            actions: () => (
              <Button onClick={goInventory}>
                <Upload class="h-4 w-4" />
                {t('admin.cardSecretImports.inventoryAction')}
              </Button>
            ),
          }}
        </PageHeader>

        <ProductSkuPicker
          picker={pk}
          title={t('admin.cardSecretImports.selectionTitle')}
          description={t('admin.cardSecretImports.selectionDescription')}
          hint={hint.value}
          onProductChange={onProductChange}
        />

        {!pk.productId.value ? (
          <div class="rounded-zs-lg border-2 border-dashed border-line-strong bg-primary-soft/60 p-8">
            <div class="mx-auto max-w-xl space-y-4 text-center">
              <div class="zs-gradient-bg mx-auto flex h-16 w-16 items-center justify-center rounded-full text-on-primary shadow-zs">
                <PackagePlus class="h-8 w-8" />
              </div>
              <h2 class="zs-display text-xl text-fg">{t('admin.cardSecretImports.emptyTitle')}</h2>
              <p class="text-sm text-muted">{t('admin.cardSecretImports.emptyDescription')}</p>
            </div>
          </div>
        ) : (
          <div class="space-y-4">
            <div class="rounded-zs-lg border border-line bg-primary-soft/70 p-4">
              <div class="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
                <div>
                  <p class="text-sm font-medium text-fg">{t('admin.cardSecretImports.targetTitle')}</p>
                  <p class="mt-1 text-sm text-muted">{pk.productLabel.value}</p>
                  <p class="mt-1 text-xs text-muted">
                    {t('admin.cardSecrets.skuLabel')}：{pk.currentSkuLabel.value}
                  </p>
                </div>
                <Button size="sm" onClick={goInventory}>
                  {t('admin.cardSecretImports.inventoryAction')}
                </Button>
              </div>
              <p class="mt-3 text-xs text-muted">{t('admin.cardSecretImports.targetHint')}</p>
              {pk.requireExplicitSku.value && !pk.skuId.value && <p class="mt-2 text-xs text-danger-text">{t('admin.cardSecrets.errors.skuRequired')}</p>}
            </div>

            <div>
              <div class="mb-4">
                <h2 class="zs-display text-lg text-fg">{t('admin.cardSecretImports.readyTitle')}</h2>
                <p class="mt-1 text-sm text-muted">{t('admin.cardSecretImports.readyDescription')}</p>
              </div>
              <CardSecretBatchCreateForm
                productId={pk.productId.value || 0}
                skuId={pk.skuId.value}
                requireSkuSelection={pk.requireExplicitSku.value}
                onSuccess={() => void pk.loadProductInfo()}
              />
            </div>
          </div>
        )}
      </div>
    )
  },
})
