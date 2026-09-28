import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminMemberLevel } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { useCrudModal } from '@/composables/useCrudModal'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { getLocalizedText, toLocalizedForm } from '@/utils/format'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'
import { isImagePath } from './usersUtils'

export type IconMode = 'emoji' | 'image'

export interface MemberLevelForm {
  name: Record<'zh-CN' | 'zh-TW' | 'en-US', string>
  slug: string
  icon: string
  discount_rate: number | ''
  recharge_threshold: number | ''
  spend_threshold: number | ''
  is_default: boolean
  sort_order: number | ''
  is_active: boolean
}

const emptyForm = (): MemberLevelForm => ({
  name: toLocalizedForm(null),
  slug: '',
  icon: '',
  discount_rate: 100,
  recharge_threshold: 0,
  spend_threshold: 0,
  is_default: false,
  sort_order: 0,
  is_active: true,
})

/** Page logic for 会员等级管理 (list, create/edit dialog with emoji/image icon, delete, backfill). */
export function useMemberLevels() {
  const t = i18n.global.t
  const list = useListPage<AdminMemberLevel>({
    pageSize: 50,
    fetchFn: (page, pageSize) => adminAPI.getMemberLevels({ page, page_size: pageSize }),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const form = reactive<MemberLevelForm>(emptyForm())
  const modal = useCrudModal({ onSuccess: () => void list.refresh() })
  const { errors, validate, clearErrors } = useFormValidation<{ slug: string }>({
    slug: [rules.required(t('admin.common.required'))],
  })

  // Emoji / image icon modes keep their own draft value so switching back and forth is lossless.
  const iconMode = ref<IconMode>('emoji')
  const iconCache = reactive<Record<IconMode, string>>({ emoji: '', image: '' })
  const switchIconMode = (mode: IconMode) => {
    if (mode === iconMode.value) return
    iconCache[iconMode.value] = form.icon
    iconMode.value = mode
    form.icon = iconCache[mode]
  }

  const openCreate = () => {
    clearErrors()
    Object.assign(form, emptyForm())
    iconMode.value = 'emoji'
    iconCache.emoji = ''
    iconCache.image = ''
    modal.openCreate()
  }

  const openEdit = (level: AdminMemberLevel) => {
    clearErrors()
    Object.assign(form, {
      name: toLocalizedForm(level.name),
      slug: level.slug || '',
      icon: level.icon || '',
      discount_rate: Number(level.discount_rate) || 100,
      recharge_threshold: Number(level.recharge_threshold) || 0,
      spend_threshold: Number(level.spend_threshold) || 0,
      is_default: Boolean(level.is_default),
      sort_order: level.sort_order || 0,
      is_active: Boolean(level.is_active),
    })
    iconMode.value = isImagePath(form.icon) ? 'image' : 'emoji'
    iconCache.emoji = iconMode.value === 'emoji' ? form.icon : ''
    iconCache.image = iconMode.value === 'image' ? form.icon : ''
    modal.openEdit(level.id)
  }

  const submit = () => {
    if (!validate({ slug: form.slug })) return
    void modal.handleSubmit(async () => {
      const payload = {
        name: { ...form.name },
        slug: form.slug.trim(),
        icon: form.icon.trim(),
        discount_rate: Number(form.discount_rate),
        recharge_threshold: Number(form.recharge_threshold),
        spend_threshold: Number(form.spend_threshold),
        is_default: form.is_default,
        sort_order: Number(form.sort_order),
        is_active: form.is_active,
      }
      const id = modal.editingId.value
      if (modal.isEditing.value && id) await adminAPI.updateMemberLevel(id, payload)
      else await adminAPI.createMemberLevel(payload)
      notifySuccess(t('admin.memberLevels.saveSuccess'))
    })
  }

  const remove = async (level: AdminMemberLevel) => {
    const ok = await confirmAction({
      description: t('admin.memberLevels.deleteConfirm', { name: getLocalizedText(level.name) }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteMemberLevel(level.id)
      notifySuccess(t('admin.memberLevels.deleteSuccess'))
      await list.refresh()
    } catch {
      /* already notified */
    }
  }

  const backfilling = ref(false)
  const backfill = async () => {
    if (!(await confirmAction({ description: t('admin.memberLevels.backfillConfirm'), confirmText: t('admin.common.confirm') }))) return
    backfilling.value = true
    try {
      const res = await adminAPI.backfillMemberLevels()
      const affected = Number((res.data as { affected?: unknown } | undefined)?.affected ?? 0) || 0
      notifySuccess(t('admin.memberLevels.backfillSuccess', { count: affected }))
    } catch {
      /* already notified */
    } finally {
      backfilling.value = false
    }
  }

  return { list, refreshing, refresh, form, modal, errors, iconMode, switchIconMode, openCreate, openEdit, submit, remove, backfilling, backfill }
}
