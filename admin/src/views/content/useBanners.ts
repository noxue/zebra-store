import { computed, reactive, watch } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminBanner } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useCrudModal } from '@/composables/useCrudModal'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { cleanParams, toDateTimeLocal, toLocalizedForm } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { buildBannerPayload, type BannerForm } from './contentUtils'

const emptyForm = (): BannerForm => ({
  id: 0,
  name: '',
  position: 'home_hero',
  title: toLocalizedForm(null),
  subtitle: toLocalizedForm(null),
  image: '',
  mobile_image: '',
  link_type: 'none',
  link_value: '',
  open_in_new_tab: false,
  is_active: true,
  start_at: '',
  end_at: '',
  sort_order: 0,
})

/** Page logic for Banner 管理 (filters, list, create/edit dialog). */
export function useBanners() {
  const t = i18n.global.t
  const filters = reactive({ search: '', position: 'home_hero', isActive: '__all__' })
  const form = reactive<BannerForm>(emptyForm())

  const positionOptions = computed(() => [{ label: t('admin.banners.positions.homeHero'), value: 'home_hero' }])
  const linkTypeOptions = computed(() => [
    { label: t('admin.banners.linkTypes.none'), value: 'none' },
    { label: t('admin.banners.linkTypes.internal'), value: 'internal' },
    { label: t('admin.banners.linkTypes.external'), value: 'external' },
  ])
  const activeFilterOptions = computed(() => [
    { label: t('admin.banners.filters.statusAll'), value: '__all__' },
    { label: t('admin.common.enabled'), value: 'true' },
    { label: t('admin.common.disabled'), value: 'false' },
  ])
  const positionLabel = (v: string) => positionOptions.value.find((o) => o.value === v)?.label || v
  const linkTypeLabel = (v: string) => linkTypeOptions.value.find((o) => o.value === v)?.label || v

  const list = useListPage<AdminBanner>({
    pageSize: 10,
    fetchFn: (page, pageSize) =>
      adminAPI.getBanners(
        cleanParams({
          page,
          page_size: pageSize,
          search: filters.search,
          position: filters.position,
          is_active: filters.isActive === '__all__' ? undefined : filters.isActive === 'true',
        }),
      ),
  })
  const modal = useCrudModal({ onSuccess: () => void list.fetchData(1) })
  const { errors, validate, clearErrors } = useFormValidation<{ name: string; position: string; image: string }>({
    name: [rules.required(t('admin.common.required'))],
    position: [rules.required(t('admin.common.required'))],
    image: [rules.required(t('admin.common.required'))],
  })

  // clear a field's error as soon as it gets a value (the image comes from a picker, not typing)
  watch(
    () => [form.name, form.position, form.image] as const,
    ([name, position, image]) => {
      if (name && errors.name) errors.name = ''
      if (position && errors.position) errors.position = ''
      if (image && errors.image) errors.image = ''
    },
  )

  const openCreate = () => {
    clearErrors()
    Object.assign(form, emptyForm())
    modal.openCreate()
  }

  const openEdit = (b: AdminBanner) => {
    clearErrors()
    Object.assign(form, {
      id: b.id,
      name: b.name || '',
      position: b.position || 'home_hero',
      title: toLocalizedForm(b.title),
      subtitle: toLocalizedForm(b.subtitle),
      image: b.image || '',
      mobile_image: b.mobile_image || '',
      link_type: b.link_type || 'none',
      link_value: b.link_value || '',
      open_in_new_tab: Boolean(b.open_in_new_tab),
      is_active: Boolean(b.is_active),
      start_at: toDateTimeLocal(b.start_at),
      end_at: toDateTimeLocal(b.end_at),
      sort_order: b.sort_order || 0,
    })
    modal.openEdit(b.id)
  }

  const openEditById = async (raw: unknown) => {
    const id = Number(raw)
    if (!Number.isFinite(id) || id <= 0) return
    try {
      const res = await adminAPI.getBanner(id)
      if (res.data) openEdit(res.data)
    } catch {
      /* toast shown by client */
    }
  }

  const submit = () => {
    if (!validate({ name: form.name, position: form.position, image: form.image })) return
    void modal.handleSubmit(async () => {
      const payload = buildBannerPayload(form)
      if (modal.isEditing.value && form.id) await adminAPI.updateBanner(form.id, payload)
      else await adminAPI.createBanner(payload)
      notifySuccess(t('admin.common.operationSuccess'))
    })
  }

  const remove = async (b: AdminBanner) => {
    const ok = await confirmAction({
      description: t('admin.banners.confirmDelete', { name: b.name || `#${b.id}` }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteBanner(b.id)
      notifySuccess(t('admin.common.operationSuccess'))
      await list.refresh()
    } catch {
      /* toast shown by client */
    }
  }

  return { filters, form, list, modal, errors, positionOptions, linkTypeOptions, activeFilterOptions, positionLabel, linkTypeLabel, openCreate, openEdit, openEditById, submit, remove }
}
