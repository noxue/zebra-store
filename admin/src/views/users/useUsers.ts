import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminMemberLevel, AdminUser } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { useSelection } from '@/composables/useSelection'
import { useCrudModal } from '@/composables/useCrudModal'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'
import { fetchAllMemberLevels, fetchSiteCurrency, nextSortState, type UserSortColumn, type UserSortState } from './usersUtils'

export interface UserEditForm {
  email: string
  nickname: string
  password: string
  locale: string
  email_verified: 'verified' | 'unverified'
  status: string
  admin_note: string
}

const emptyFilters = () => ({
  userId: '',
  keyword: '',
  status: '__all__',
  createdFrom: '',
  createdTo: '',
  lastLoginFrom: '',
  lastLoginTo: '',
})

/** Page logic for 用户管理 (filters, sortable list, batch status, edit dialog). */
export function useUsers() {
  const t = i18n.global.t
  const filters = reactive(emptyFilters())
  const sort = reactive<UserSortState>({ by: '', order: '' })
  const siteCurrency = ref('CNY')
  const memberLevels = ref<Map<number, AdminMemberLevel>>(new Map())

  const list = useListPage<AdminUser>({
    fetchFn: async (page, pageSize) => {
      const res = await adminAPI.getUsers(
        cleanParams({
          page,
          page_size: pageSize,
          user_id: filters.userId,
          keyword: filters.keyword,
          status: filters.status,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
          last_login_from: toRFC3339(filters.lastLoginFrom),
          last_login_to: toRFC3339(filters.lastLoginTo),
          sort_by: sort.by,
          sort_order: sort.by ? sort.order : undefined,
        }),
      )
      selection.clear()
      return res
    },
  })
  const selection = useSelection(list.items, (u) => u.id)

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const resetFilters = () => {
    Object.assign(filters, emptyFilters())
    void list.fetchData(1)
  }

  const toggleSort = (column: UserSortColumn) => {
    Object.assign(sort, nextSortState(sort, column))
    void list.fetchData(1)
  }

  const init = async () => {
    void list.fetchData(1)
    fetchSiteCurrency().then((c) => (siteCurrency.value = c))
    const levels = await fetchAllMemberLevels()
    memberLevels.value = new Map(levels.map((l) => [l.id, l]))
  }

  const batchUpdateStatus = async (status: 'active' | 'disabled') => {
    const ids = selection.selectedIds.value
    if (ids.length === 0) return
    if (!(await confirmAction({ description: t('admin.users.batch.confirm', { count: ids.length }), variant: status === 'disabled' ? 'destructive' : 'default' }))) return
    try {
      await adminAPI.batchUpdateUserStatus({ user_ids: ids, status })
      notifySuccess(t('admin.common.operationSuccess'))
      await list.refresh()
    } catch {
      /* already notified */
    }
  }

  // ---- edit dialog ----
  const form = reactive<UserEditForm>({
    email: '',
    nickname: '',
    password: '',
    locale: 'zh-CN',
    email_verified: 'unverified',
    status: 'active',
    admin_note: '',
  })
  const modal = useCrudModal({ onSuccess: () => void list.refresh() })
  const { errors, validate, clearErrors } = useFormValidation<{ email: string; nickname: string }>({
    email: [rules.required(t('admin.common.required')), rules.email(t('admin.users.form.emailInvalid'))],
    nickname: [rules.required(t('admin.common.required'))],
  })

  const openEdit = (user: AdminUser) => {
    clearErrors()
    Object.assign(form, {
      email: user.email || '',
      nickname: user.display_name || '',
      password: '',
      locale: user.locale || 'zh-CN',
      email_verified: user.email_verified_at ? 'verified' : 'unverified',
      status: user.status || 'active',
      admin_note: user.admin_note || '',
    })
    modal.openEdit(user.id)
  }

  const submit = () => {
    const id = modal.editingId.value
    if (!id) return
    if (!validate({ email: form.email, nickname: form.nickname })) return
    void modal.handleSubmit(async () => {
      await adminAPI.updateUser(id, {
        email: form.email,
        nickname: form.nickname,
        password: form.password || undefined,
        locale: form.locale,
        email_verified: form.email_verified === 'verified',
        status: form.status,
        admin_note: form.admin_note,
      })
      notifySuccess(t('admin.common.operationSuccess'))
    })
  }

  const hasSelection = computed(() => selection.selectedIds.value.length > 0)

  return {
    filters,
    sort,
    siteCurrency,
    memberLevels,
    list,
    selection,
    hasSelection,
    refreshing,
    refresh,
    resetFilters,
    toggleSort,
    init,
    batchUpdateStatus,
    form,
    modal,
    errors,
    openEdit,
    submit,
  }
}
