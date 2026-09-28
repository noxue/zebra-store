import { computed, reactive, ref, watch } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { errorMessage } from '@/api/client'
import type { AdminMedia } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { cleanParams } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { DEFAULT_UPLOAD_MAX_SIZE_BYTES, formatFileSizeLimit, splitFilesBySize } from '@/utils/upload'

export const MEDIA_SCENES = ['product', 'banner', 'category', 'post', 'editor', 'common', 'telegram', 'upstream'] as const
export const MEDIA_PAGE_SIZE_OPTIONS = [24, 48, 96]
const UPLOAD_CONCURRENCY = 3

export interface UploadFailure {
  name: string
  message: string
}

/**
 * One toast for a batch upload: every failed file with the backend's reason
 * (QA-A15: it used to show the failure count as the message, "Upload failed: 1").
 */
export function uploadFailureMessage(t: (key: string, params?: Record<string, unknown>) => string, failures: UploadFailure[], total: number): string {
  const details = failures.map((f) => (f.message ? `${f.name}: ${f.message}` : f.name)).join('; ')
  if (failures.length >= total) return t('admin.media.errors.uploadFailed', { message: details })
  return `${t('admin.media.errors.uploadPartialFailed', { fail: failures.length, total })} ${details}`
}

/** Page logic for 素材管理: grid list (`data.{items,total}`), upload, inline rename, delete, batch delete. */
export function useMedia() {
  const t = i18n.global.t
  const filters = reactive({ search: '', scene: '__all__' })
  const uploading = ref(false)
  const uploadProgress = reactive({ current: 0, total: 0 })
  const batchMode = ref(false)
  const batchOperating = ref(false)
  const selected = ref<Set<number>>(new Set())
  const editingId = ref<number | null>(null)
  const editingName = ref('')

  const list = useListPage<AdminMedia>({
    pageSize: 24,
    fetchFn: async (page, pageSize) => {
      const res = await adminAPI.getMedia(cleanParams({ page, page_size: pageSize, search: filters.search, scene: filters.scene }))
      const items = res.data?.items ?? []
      const total = res.data?.total ?? 0
      return { items, pagination: { page, page_size: pageSize, total, total_page: Math.ceil(total / pageSize) } }
    },
  })

  // keep only selections visible on the current page (same as the original)
  watch(list.items, (items) => {
    const visible = new Set(items.map((i) => i.id))
    selected.value = new Set(Array.from(selected.value).filter((id) => visible.has(id)))
  })
  watch(() => filters.search, list.debouncedSearch)
  watch(() => filters.scene, () => void list.fetchData(1))

  const allSelected = computed(() => list.items.value.length > 0 && list.items.value.every((i) => selected.value.has(i.id)))
  const isSelected = (id: number) => selected.value.has(id)
  const clearSelection = () => {
    selected.value = new Set()
  }
  const toggleSelect = (id: number) => {
    const next = new Set(selected.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selected.value = next
  }
  const toggleSelectAll = () => {
    selected.value = allSelected.value ? new Set() : new Set(list.items.value.map((i) => i.id))
  }
  const toggleBatchMode = () => {
    batchMode.value = !batchMode.value
    editingId.value = null
    if (!batchMode.value) clearSelection()
  }

  const upload = async (files: File[]) => {
    if (!files.length) return
    const { accepted, rejected } = splitFilesBySize(files)
    if (rejected.length) notifyError(t('admin.media.errors.fileTooLarge', { count: rejected.length, max: formatFileSizeLimit(DEFAULT_UPLOAD_MAX_SIZE_BYTES) }))
    if (!accepted.length) return
    uploading.value = true
    uploadProgress.current = 0
    uploadProgress.total = accepted.length
    const failures: UploadFailure[] = []
    try {
      for (let i = 0; i < accepted.length; i += UPLOAD_CONCURRENCY) {
        await Promise.allSettled(
          accepted.slice(i, i + UPLOAD_CONCURRENCY).map(async (file) => {
            try {
              await adminAPI.upload(file, 'common', undefined, { silent: true })
            } catch (err) {
              failures.push({ name: file.name, message: errorMessage(err) })
            } finally {
              uploadProgress.current++
            }
          }),
        )
      }
      if (failures.length > 0) notifyError(uploadFailureMessage(t, failures, accepted.length))
      await list.fetchData(1)
    } finally {
      uploading.value = false
    }
  }

  const startRename = (item: AdminMedia) => {
    editingId.value = item.id
    editingName.value = item.name
  }
  const cancelRename = () => {
    editingId.value = null
  }
  const saveRename = async (item: AdminMedia) => {
    if (editingId.value !== item.id) return
    const name = editingName.value.trim()
    editingId.value = null
    if (!name || name === item.name) return
    try {
      await adminAPI.updateMedia(item.id, { name })
      list.items.value = list.items.value.map((m) => (m.id === item.id ? { ...m, name } : m))
      notifySuccess(t('admin.media.card.renameSaved'))
    } catch {
      /* toast shown by client */
    }
  }

  const remove = async (item: AdminMedia) => {
    const ok = await confirmAction({
      title: t('admin.media.confirmDelete', { name: item.name }),
      description: t('admin.media.deleteWarning'),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteMedia(item.id)
      await list.refresh()
    } catch {
      /* toast shown by client */
    }
  }

  const batchDelete = async () => {
    const ids = Array.from(selected.value)
    if (!ids.length) return
    const ok = await confirmAction({
      description: t('admin.media.batch.deleteConfirm', { count: ids.length }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    batchOperating.value = true
    try {
      const res = await adminAPI.batchDeleteMedia(ids)
      const success = Number(res.data?.success_count ?? 0)
      notifySuccess(t('admin.media.batch.deleteResult', { success, total: ids.length }))
      clearSelection()
      const page = list.pagination.value.page
      await list.fetchData(list.items.value.length <= success && page > 1 ? page - 1 : page)
    } catch {
      /* toast shown by client */
    } finally {
      batchOperating.value = false
    }
  }

  return {
    filters,
    list,
    uploading,
    uploadProgress,
    batchMode,
    batchOperating,
    selected,
    allSelected,
    isSelected,
    clearSelection,
    toggleSelect,
    toggleSelectAll,
    toggleBatchMode,
    upload,
    editingId,
    editingName,
    startRename,
    cancelRename,
    saveRename,
    remove,
    batchDelete,
  }
}
