import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminCategory } from '@/api/types'
import { getLocalizedText, toLocalizedForm } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { buildAdminCategoryPath, createAdminCategoryMap, flattenAdminCategories } from '@/utils/category'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { useCrudModal } from '@/composables/useCrudModal'

export interface CategoryForm {
  id: number
  parent_id: number
  name: Record<'zh-CN' | 'zh-TW' | 'en-US', string>
  slug: string
  icon: string
  sort_order: number
}

const emptyForm = (): CategoryForm => ({ id: 0, parent_id: 0, name: toLocalizedForm(null), slug: '', icon: '', sort_order: 0 })

/** Page logic for 商品分类 (tree list + create/edit dialog). */
export function useCategories() {
  const t = i18n.global.t
  const loading = ref(false)
  const categories = ref<AdminCategory[]>([])
  const form = reactive<CategoryForm>(emptyForm())
  const modal = useCrudModal({ onSuccess: () => void fetchCategories() })
  const { errors, validate, clearErrors } = useFormValidation<{ slug: string; name: string }>({
    slug: [rules.required(t('admin.common.required'))],
    name: [rules.required(t('admin.common.required'))],
  })

  const categoryMap = computed(() => createAdminCategoryMap(categories.value))
  const hierarchy = computed(() => flattenAdminCategories(categories.value))
  const hasChildren = (id: number) => categories.value.some((c) => c.parent_id === id)

  /** A root category that already has children cannot be moved under another parent. */
  const canChooseParent = computed(() => {
    if (!modal.isEditing.value || form.id <= 0) return true
    const current = categoryMap.value.get(form.id)
    return !current || current.parent_id > 0 || !hasChildren(current.id)
  })
  const parentOptions = computed(() => {
    const root = [{ label: t('admin.categories.form.parentRoot'), value: 0 }]
    if (!canChooseParent.value) return root
    return root.concat(
      hierarchy.value
        .filter((h) => h.depth === 0 && h.category.id !== form.id)
        .map((h) => ({ label: buildAdminCategoryPath(h.category, categoryMap.value, (c) => getLocalizedText(c.name)), value: h.category.id })),
    )
  })

  async function fetchCategories() {
    loading.value = true
    try {
      categories.value = (await adminAPI.getCategories()).data ?? []
    } catch {
      categories.value = []
    } finally {
      loading.value = false
    }
  }

  const openCreate = () => {
    clearErrors()
    Object.assign(form, emptyForm())
    modal.openCreate()
  }

  const openEdit = (c: AdminCategory) => {
    clearErrors()
    Object.assign(form, {
      id: c.id,
      parent_id: c.parent_id || 0,
      name: toLocalizedForm(c.name),
      slug: c.slug,
      icon: c.icon || '',
      sort_order: c.sort_order,
    })
    modal.openEdit(c.id)
  }

  const openEditById = async (raw: unknown) => {
    const id = Number(raw)
    if (!Number.isFinite(id) || id <= 0) return
    if (!categories.value.length) await fetchCategories()
    const target = categories.value.find((c) => c.id === id)
    if (target) openEdit(target)
  }

  const submit = () => {
    if (!validate({ slug: form.slug, name: form.name['zh-CN'] })) return
    void modal.handleSubmit(async () => {
      const payload = { ...form, name: { ...form.name }, sort_order: Number(form.sort_order) || 0 }
      if (modal.isEditing.value) await adminAPI.updateCategory(form.id, payload)
      else await adminAPI.createCategory(payload)
      notifySuccess(t('admin.common.operationSuccess'))
    })
  }

  const toggleActive = async (c: AdminCategory) => {
    const next = !c.is_active
    c.is_active = next
    categories.value = [...categories.value]
    try {
      await adminAPI.patchCategoryActive(c.id, next)
      notifySuccess(next ? t('admin.categories.status.activatedTip') : t('admin.categories.status.deactivatedTip'))
    } catch {
      c.is_active = !next
      categories.value = [...categories.value]
    }
  }

  const remove = async (c: AdminCategory) => {
    const ok = await confirmAction({
      description: t('admin.categories.confirmDelete', { name: getLocalizedText(c.name) }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteCategory(c.id)
      notifySuccess(t('admin.common.operationSuccess'))
      await fetchCategories()
    } catch {
      /* toast shown by client */
    }
  }

  return { loading, categories, hierarchy, form, modal, errors, canChooseParent, parentOptions, fetchCategories, openCreate, openEdit, openEditById, submit, toggleActive, remove }
}
