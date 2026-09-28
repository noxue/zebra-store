import { defineComponent, toRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Card, FileInput, FormField, Input, Switch, Textarea } from '@/components/ui'
import { useCardSecretBatchCreate } from '../useCardSecretBatchCreate'

/** 批量录入 + CSV 导入 forms (original CardSecretBatchCreateModal.vue, rendered inline). */
export default defineComponent({
  name: 'CardSecretBatchCreateForm',
  props: {
    productId: { type: Number, required: true },
    skuId: { type: Number, default: 0 },
    requireSkuSelection: { type: Boolean, default: false },
  },
  emits: { success: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const f = useCardSecretBatchCreate({
      productId: toRef(props, 'productId'),
      skuId: toRef(props, 'skuId'),
      requireSkuSelection: toRef(props, 'requireSkuSelection'),
      onSuccess: () => emit('success'),
    })
    const blocked = () => props.requireSkuSelection && !props.skuId

    const dedupRow = (value: boolean, onUpdate: (v: boolean) => void) => (
      <div class="flex items-start justify-between gap-4 border-y border-line py-3">
        <div>
          <p class="text-sm font-medium text-fg">{t('admin.cardSecrets.deduplicateLabel')}</p>
          <p class="mt-1 text-xs text-muted">{t('admin.cardSecrets.deduplicateHint')}</p>
        </div>
        <Switch modelValue={value} onUpdate:modelValue={onUpdate} />
      </div>
    )
    const messages = (error: string, success: string) => (
      <>
        {error && <div class="rounded-zs-sm border border-line bg-danger-soft p-3 text-sm text-danger-text">{error}</div>}
        {success && <div class="rounded-zs-sm border border-line bg-success-soft p-3 text-sm text-success-text">{success}</div>}
      </>
    )

    return () => (
      <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <Card title={t('admin.cardSecrets.batchTitle')}>
          <form
            class="space-y-4"
            onSubmit={(e: Event) => {
              e.preventDefault()
              void f.submitBatch()
            }}
          >
            <FormField label={t('admin.cardSecrets.secretsLabel')} required>
              <Textarea v-model={f.batchForm.secrets} rows={6} mono placeholder={t('admin.cardSecrets.secretsPlaceholder')} />
            </FormField>
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
              <FormField label={t('admin.cardSecrets.batchNoLabel')}>
                <Input v-model={f.batchForm.batch_no} placeholder="BATCH-20260203-001" mono />
              </FormField>
              <FormField label={t('admin.cardSecrets.noteLabel')}>
                <Input v-model={f.batchForm.note} placeholder={t('admin.cardSecrets.notePlaceholder')} />
              </FormField>
            </div>
            {dedupRow(f.batchForm.deduplicate, (v) => (f.batchForm.deduplicate = v))}
            {messages(f.batchError.value, f.batchSuccess.value)}
            <div class="flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
              <Button onClick={f.resetBatchForm}>{t('admin.common.reset')}</Button>
              <Button type="submit" variant="primary" loading={f.batchSubmitting.value} disabled={blocked()}>
                {f.batchSubmitting.value ? t('admin.cardSecrets.submitting') : t('admin.cardSecrets.submitBatch')}
              </Button>
            </div>
          </form>
        </Card>

        <Card title={t('admin.cardSecrets.importTitle')}>
          <form
            class="space-y-4"
            onSubmit={(e: Event) => {
              e.preventDefault()
              void f.submitImport()
            }}
          >
            <FormField label={t('admin.cardSecrets.csvLabel')} required hint={t('admin.cardSecrets.csvHint')}>
              <div class="flex items-center gap-2">
                <div class="min-w-0 flex-1">
                  <FileInput
                    key={f.fileInputKey.value}
                    accept=".csv"
                    placeholder={t('admin.cardSecrets.csvPlaceholder')}
                    onChange={(file) => (f.importForm.file = file)}
                  />
                </div>
                {f.importForm.file && (
                  <Button size="sm" variant="ghost" onClick={f.clearImportFile}>
                    {t('admin.cardSecrets.csvClear')}
                  </Button>
                )}
              </div>
            </FormField>
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
              <FormField label={t('admin.cardSecrets.batchNoLabel')}>
                <Input v-model={f.importForm.batch_no} placeholder="BATCH-20260203-002" mono />
              </FormField>
              <FormField label={t('admin.cardSecrets.noteLabel')}>
                <Input v-model={f.importForm.note} placeholder={t('admin.cardSecrets.importNotePlaceholder')} />
              </FormField>
            </div>
            {dedupRow(f.importForm.deduplicate, (v) => (f.importForm.deduplicate = v))}
            {messages(f.importError.value, f.importSuccess.value)}
            <div class="flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
              <Button onClick={f.resetImportForm}>{t('admin.common.reset')}</Button>
              <Button type="submit" variant="primary" loading={f.importSubmitting.value} disabled={blocked()}>
                {f.importSubmitting.value ? t('admin.cardSecrets.importing') : t('admin.cardSecrets.startImport')}
              </Button>
            </div>
          </form>
        </Card>
      </div>
    )
  },
})
