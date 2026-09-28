import { reactive, ref, type Ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { parseSecretLines } from './cardSecretUtils'

export interface BatchCreateTarget {
  productId: Ref<number>
  skuId: Ref<number>
  requireSkuSelection: Ref<boolean>
  onSuccess: () => void
}

const errorText = (err: unknown, fallback: string) => (err instanceof Error && err.message ? err.message : fallback)

/** Two import forms of 卡密导入: paste (`card-secrets/batch`) and CSV/TXT file (`card-secrets/import`). */
export function useCardSecretBatchCreate(target: BatchCreateTarget) {
  const t = i18n.global.t

  const batchForm = reactive({ secrets: '', batch_no: '', note: '', deduplicate: true })
  const batchSubmitting = ref(false)
  const batchError = ref('')
  const batchSuccess = ref('')

  const importForm = reactive({ file: null as File | null, batch_no: '', note: '', deduplicate: true })
  const importSubmitting = ref(false)
  const importError = ref('')
  const importSuccess = ref('')
  /** Bumped to re-mount the FileInput (clears its displayed file name). */
  const fileInputKey = ref(0)

  const checkTarget = (setError: (msg: string) => void) => {
    if (!target.productId.value) {
      setError(t('admin.cardSecrets.errors.productRequired'))
      return false
    }
    if (target.requireSkuSelection.value && !target.skuId.value) {
      setError(t('admin.cardSecrets.errors.skuRequired'))
      return false
    }
    return true
  }

  const resetBatchForm = () => {
    Object.assign(batchForm, { secrets: '', batch_no: '', note: '', deduplicate: true })
    batchError.value = ''
    batchSuccess.value = ''
  }

  const submitBatch = async () => {
    batchError.value = ''
    batchSuccess.value = ''
    if (!checkTarget((m) => (batchError.value = m))) return
    const secrets = parseSecretLines(batchForm.secrets)
    if (!secrets.length) {
      batchError.value = t('admin.cardSecrets.errors.secretsRequired')
      return
    }
    batchSubmitting.value = true
    try {
      await adminAPI.createCardSecretBatch({
        product_id: target.productId.value,
        sku_id: target.skuId.value || undefined,
        secrets,
        batch_no: batchForm.batch_no.trim(),
        note: batchForm.note.trim(),
        deduplicate: batchForm.deduplicate,
      })
      batchSuccess.value = t('admin.cardSecrets.success.batchCreated')
      batchForm.secrets = ''
      target.onSuccess()
    } catch (err) {
      batchError.value = errorText(err, t('admin.cardSecrets.errors.batchFailed'))
    } finally {
      batchSubmitting.value = false
    }
  }

  const clearImportFile = () => {
    importForm.file = null
    fileInputKey.value += 1
  }

  const resetImportForm = () => {
    clearImportFile()
    Object.assign(importForm, { batch_no: '', note: '', deduplicate: true })
    importError.value = ''
    importSuccess.value = ''
  }

  const submitImport = async () => {
    importError.value = ''
    importSuccess.value = ''
    if (!checkTarget((m) => (importError.value = m))) return
    if (!importForm.file) {
      importError.value = t('admin.cardSecrets.errors.fileRequired')
      return
    }
    importSubmitting.value = true
    try {
      const formData = new FormData()
      formData.append('product_id', String(target.productId.value))
      if (target.skuId.value > 0) formData.append('sku_id', String(target.skuId.value))
      formData.append('batch_no', importForm.batch_no.trim())
      formData.append('note', importForm.note.trim())
      formData.append('deduplicate', String(importForm.deduplicate))
      formData.append('file', importForm.file)
      await adminAPI.importCardSecretCSV(formData)
      resetImportForm()
      importSuccess.value = t('admin.cardSecrets.success.imported')
      target.onSuccess()
    } catch (err) {
      importError.value = errorText(err, t('admin.cardSecrets.errors.importFailed'))
    } finally {
      importSubmitting.value = false
    }
  }

  return {
    batchForm,
    batchSubmitting,
    batchError,
    batchSuccess,
    resetBatchForm,
    submitBatch,
    importForm,
    importSubmitting,
    importError,
    importSuccess,
    fileInputKey,
    clearImportFile,
    resetImportForm,
    submitImport,
  }
}
