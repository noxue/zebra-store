import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminGiftCard, AdminGiftCardBatch, AdminUpdateGiftCardPayload } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { useSelection } from '@/composables/useSelection'
import { cleanParams, toDateTimeLocal, toRFC3339 } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { downloadBlob, filenameFromDisposition } from '@/utils/download'
import { validateGiftCardGenerate, type GiftCardRedeemedUser } from './marketingUtils'

export type GiftCardRow = AdminGiftCard & {
  is_expired?: boolean
  redeemed_user?: GiftCardRedeemedUser
  batch?: AdminGiftCardBatch & { batch_no?: string }
}

/** Page logic for 礼品卡: list + filters, generate/edit dialogs, delete, batch status and export. */
export function useGiftCards() {
  const t = i18n.global.t

  const filters = reactive({ code: '', status: '__all__', redeemedUserID: '', createdFrom: '', createdTo: '' })

  const list = useListPage<GiftCardRow>({
    fetchFn: (page, pageSize) => {
      const uid = Number(filters.redeemedUserID)
      return adminAPI.getGiftCards(
        cleanParams({
          page,
          page_size: pageSize,
          code: filters.code,
          status: filters.status,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
          redeemed_user_id: Number.isFinite(uid) && uid > 0 ? Math.floor(uid) : undefined,
        }),
      )
    },
  })
  const selection = useSelection(list.items, (r) => r.id)

  const fetch = async (page = 1) => {
    await list.fetchData(page)
    selection.clear()
  }
  const handleSearch = () => void fetch(1)
  const changePage = (page: number) => {
    if (page < 1 || page > list.pagination.value.total_page) return
    void fetch(page)
  }
  const changePageSize = (size: number) => {
    list.pagination.value = { ...list.pagination.value, page_size: size }
    void fetch(1)
  }
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(() => fetch(list.pagination.value.page))

  // --- batch ---
  const batchStatusTarget = ref<'active' | 'disabled'>('disabled')
  const batchLoading = ref(false)
  const hasSelection = computed(() => selection.selectedIds.value.length > 0)

  const applyBatchStatus = async () => {
    const ids = selection.selectedIds.value
    if (!ids.length) return notifyError(t('admin.giftCards.errors.selectRequired'))
    const ok = await confirmAction({
      description: t('admin.giftCards.batch.confirmStatus', { count: ids.length, status: t(`admin.giftCards.status.${batchStatusTarget.value}`) }),
    })
    if (!ok) return
    batchLoading.value = true
    try {
      const res = await adminAPI.batchUpdateGiftCardStatus({ ids, status: batchStatusTarget.value })
      notifySuccess(t('admin.giftCards.alerts.statusUpdated', { count: Number(res.data?.affected || 0) }))
      await fetch(list.pagination.value.page)
    } catch {
      /* already notified */
    } finally {
      batchLoading.value = false
    }
  }

  const exportSelected = async (format: 'txt' | 'csv') => {
    const ids = selection.selectedIds.value
    if (!ids.length) return notifyError(t('admin.giftCards.errors.selectRequired'))
    batchLoading.value = true
    try {
      const res = await adminAPI.exportGiftCards({ ids, format })
      const fallback = `gift-cards-${new Date().toISOString().replace(/[:.]/g, '-')}.${format}`
      downloadBlob(res.data, filenameFromDisposition(res.headers['content-disposition'], fallback))
      notifySuccess(t('admin.giftCards.alerts.exported', { count: ids.length, format: format.toUpperCase() }))
    } catch {
      /* already notified */
    } finally {
      batchLoading.value = false
    }
  }

  // --- generate ---
  const showGenerate = ref(false)
  const generateSubmitting = ref(false)
  const generateError = ref('')
  const generateForm = reactive({ name: '', quantity: 10 as number | string, amount: '', expiresAt: '' })

  const openGenerate = () => {
    Object.assign(generateForm, { name: '', quantity: 10, amount: '', expiresAt: '' })
    generateError.value = ''
    showGenerate.value = true
  }

  const submitGenerate = async () => {
    generateError.value = ''
    const v = validateGiftCardGenerate(generateForm)
    if (!v.ok) {
      generateError.value = t(`admin.giftCards.errors.${v.error}`)
      return
    }
    generateSubmitting.value = true
    try {
      const res = await adminAPI.generateGiftCards({
        name: generateForm.name.trim(),
        quantity: v.quantity,
        amount: v.amount,
        expires_at: toRFC3339(generateForm.expiresAt) ?? '',
      })
      const data = (res.data ?? {}) as { created?: number; batch?: { batch_no?: string } }
      const created = Number(data.created || 0)
      const batchNo = String(data.batch?.batch_no || '').trim()
      notifySuccess(batchNo ? t('admin.giftCards.alerts.generatedDetail', { count: created, batchNo }) : t('admin.giftCards.alerts.generated', { count: created }))
      showGenerate.value = false
      await fetch(1)
    } catch (err) {
      generateError.value = err instanceof Error && err.message ? err.message : t('admin.giftCards.errors.generateFailed')
    } finally {
      generateSubmitting.value = false
    }
  }

  // --- edit ---
  const showEdit = ref(false)
  const editSubmitting = ref(false)
  const editError = ref('')
  const editingCard = ref<GiftCardRow | null>(null)
  const editForm = reactive({ name: '', status: 'active' as 'active' | 'disabled', expiresAt: '' })
  const editReadonly = computed(() => String(editingCard.value?.status || '') === 'redeemed')

  const openEdit = (card: GiftCardRow) => {
    editingCard.value = card
    editForm.name = String(card.name || '')
    editForm.status = String(card.status || '').toLowerCase() === 'disabled' ? 'disabled' : 'active'
    editForm.expiresAt = toDateTimeLocal(card.expires_at)
    editError.value = ''
    showEdit.value = true
  }

  const submitEdit = async () => {
    const card = editingCard.value
    if (!card?.id) return
    editError.value = ''
    const name = editForm.name.trim()
    if (!name) {
      editError.value = t('admin.giftCards.errors.nameRequired')
      return
    }
    const payload: AdminUpdateGiftCardPayload = { name }
    if (!editReadonly.value && editForm.status !== String(card.status || '').toLowerCase()) payload.status = editForm.status
    if (editForm.expiresAt !== toDateTimeLocal(card.expires_at)) payload.expires_at = toRFC3339(editForm.expiresAt) ?? ''
    editSubmitting.value = true
    try {
      await adminAPI.updateGiftCard(card.id, payload)
      notifySuccess(t('admin.giftCards.alerts.updated'))
      showEdit.value = false
      editingCard.value = null
      await fetch(list.pagination.value.page)
    } catch (err) {
      editError.value = err instanceof Error && err.message ? err.message : t('admin.giftCards.errors.updateFailed')
    } finally {
      editSubmitting.value = false
    }
  }

  const remove = async (card: GiftCardRow) => {
    const ok = await confirmAction({ description: t('admin.giftCards.confirmDelete', { code: card.code }), confirmText: t('admin.common.delete'), variant: 'destructive' })
    if (!ok) return
    try {
      await adminAPI.deleteGiftCard(card.id)
      notifySuccess(t('admin.giftCards.alerts.deleted'))
      const p = list.pagination.value.page
      await fetch(list.items.value.length === 1 && p > 1 ? p - 1 : p)
    } catch {
      /* already notified */
    }
  }

  return {
    filters,
    list,
    selection,
    fetch,
    handleSearch,
    changePage,
    changePageSize,
    refreshing,
    refresh,
    batchStatusTarget,
    batchLoading,
    hasSelection,
    applyBatchStatus,
    exportSelected,
    showGenerate,
    generateSubmitting,
    generateError,
    generateForm,
    openGenerate,
    submitGenerate,
    showEdit,
    editSubmitting,
    editError,
    editForm,
    editReadonly,
    openEdit,
    submitEdit,
    remove,
  }
}
