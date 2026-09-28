import { computed, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { adminAPI } from '@/api/admin'
import type { AdminTelegramBroadcastUser } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useSelection } from '@/composables/useSelection'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import { fileNameFromPath, toTelegramHtml } from './telegramUtils'

export type RecipientType = 'all' | 'specific'

const emptyUserFilters = () => ({
  keyword: '',
  display_name: '',
  telegram_username: '',
  telegram_user_id: '',
  created_from: '',
  created_to: '',
})

export function useTelegramBroadcastCreate() {
  const { t } = useI18n()
  const router = useRouter()

  const submitting = ref(false)
  const uploading = ref(false)
  const pickerOpen = ref(false)

  const form = reactive({
    title: '',
    recipient_type: 'all' as RecipientType,
    attachment_url: '',
    attachment_name: '',
    /** Rich editor HTML; converted to Telegram HTML on submit. */
    message_html: '',
  })

  const filters = reactive(emptyUserFilters())

  const users = useListPage<AdminTelegramBroadcastUser>({
    pageSize: 10,
    fetchFn: (page, pageSize) =>
      adminAPI.getTelegramBroadcastUsers(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          display_name: filters.display_name,
          telegram_username: filters.telegram_username,
          telegram_user_id: filters.telegram_user_id,
          created_from: toRFC3339(filters.created_from),
          created_to: toRFC3339(filters.created_to),
        }),
      ),
  })
  const selection = useSelection(users.items, (u) => u.user_id)
  const selectedCount = computed(() => selection.selectedIds.value.length)

  const fetchUsers = (page = 1) => {
    if (form.recipient_type !== 'specific') return
    void users.fetchData(page)
  }

  const resetFilters = () => {
    Object.assign(filters, emptyUserFilters())
    fetchUsers(1)
  }

  watch(
    () => form.recipient_type,
    (value) => {
      if (value === 'specific' && users.items.value.length === 0) fetchUsers(1)
    },
  )

  const handleMediaSelected = (value: string | string[]) => {
    const path = Array.isArray(value) ? value[0] : value
    if (!path) return
    form.attachment_url = path
    form.attachment_name = fileNameFromPath(path)
  }

  const clearAttachment = () => {
    form.attachment_url = ''
    form.attachment_name = ''
  }

  const handleAttachmentChange = async (file: File | null) => {
    if (!file) return
    uploading.value = true
    try {
      const res = await adminAPI.upload(file, 'telegram')
      form.attachment_url = String(res.data?.url || '')
      form.attachment_name = String(res.data?.filename || file.name || '')
      notifySuccess(t('telegramBot.broadcasts.uploadSuccess'))
    } catch {
      /* already notified */
    } finally {
      uploading.value = false
    }
  }

  const handleSubmit = async () => {
    const messageHtml = toTelegramHtml(form.message_html)
    if (!form.title.trim() || !messageHtml) {
      notifyError(t('telegramBot.broadcasts.formInvalid'))
      return
    }
    const specific = form.recipient_type === 'specific'
    if (specific && selectedCount.value === 0) {
      notifyError(t('telegramBot.broadcasts.usersRequired'))
      return
    }
    submitting.value = true
    try {
      await adminAPI.createTelegramBroadcast({
        title: form.title.trim(),
        recipient_type: form.recipient_type,
        user_ids: specific ? selection.selectedIds.value : [],
        filters: specific
          ? {
              keyword: filters.keyword,
              display_name: filters.display_name,
              telegram_username: filters.telegram_username,
              telegram_user_id: filters.telegram_user_id,
              created_from: toRFC3339(filters.created_from),
              created_to: toRFC3339(filters.created_to),
            }
          : {},
        attachment_url: form.attachment_url || undefined,
        attachment_name: form.attachment_name || undefined,
        message_html: messageHtml,
      })
      notifySuccess(t('telegramBot.broadcasts.createSuccess'))
      void router.push('/telegram-bot/broadcasts')
    } catch {
      /* already notified */
    } finally {
      submitting.value = false
    }
  }

  return {
    form,
    filters,
    users,
    selection,
    selectedCount,
    fetchUsers,
    resetFilters,
    submitting,
    uploading,
    pickerOpen,
    handleMediaSelected,
    clearAttachment,
    handleAttachmentChange,
    handleSubmit,
  }
}
