import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { getLocalizedText, toLocalizedForm } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import {
  buildPostCategoryPath,
  createPostCategoryChildCountMap,
  createPostCategoryMap,
  flattenPostCategories,
  type PostCategory,
} from '@/utils/postCategory'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { useCrudModal } from '@/composables/useCrudModal'

export interface PostCategoryForm {
  id: number
  parent_id: number
  name: Record<'zh-CN' | 'zh-TW' | 'en-US', string>
  slug: string
  icon: string
  sort_order: number | ''
}

const emptyForm = (): PostCategoryForm => ({ id: 0, parent_id: 0, name: toLocalizedForm(null), slug: '', icon: '', sort_order: 0 })

/** Page logic for 文章分类 (two-level tree + create/edit dialog). */
export function usePostCategories() {
  const t = i18n.global.t
  const loading = ref(false)
  const categories = ref<PostCategory[]>([])
  const form = reactive<PostCategoryForm>(emptyForm())
  const modal = useCrudModal({ onSuccess: () => void fetchCategories() })
  const { errors, validate, clearErrors } = useFormValidation<{ slug: string; name: string }>({
    slug: [rules.required(t('admin.common.required'))],
    name: [rules.required(t('admin.common.required'))],
  })

  const hierarchy = computed(() => flattenPostCategories(categories.value))
  const categoryMap = computed(() => createPostCategoryMap(categories.value))
  const childCountMap = computed(() => createPostCategoryChildCountMap(categories.value))
  const hasChildren = (id: number) => (childCountMap.value.get(id) || 0) > 0

  /** A root category that already has children cannot become a child. */
  const canChooseParent = computed(() => {
    if (!modal.isEditing.value || form.id <= 0) return true
    const current = categoryMap.value.get(form.id)
    if (!current) return true
    if (current.parent_id) return true
    return !hasChildren(current.id)
  })
  const parentOptions = computed(() => {
    const root = [{ label: t('admin.categories.form.parentRoot'), value: 0 }]
    if (!canChooseParent.value) return root
    return root.concat(
      hierarchy.value
        .filter((h) => h.depth === 0 && h.category.id !== form.id)
        .map((h) => ({ label: buildPostCategoryPath(h.category, categoryMap.value, (c) => getLocalizedText(c.name)), value: h.category.id })),
    )
  })

  async function fetchCategories() {
    loading.value = true
    try {
      categories.value = ((await adminAPI.getPostCategories()).data ?? []) as unknown as PostCategory[]
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

  const openEdit = (c: PostCategory) => {
    clearErrors()
    Object.assign(form, {
      id: c.id,
      parent_id: c.parent_id || 0,
      name: toLocalizedForm(c.name),
      slug: c.slug ?? '',
      icon: c.icon || '',
      sort_order: c.sort_order || 0,
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
      if (modal.isEditing.value) await adminAPI.updatePostCategory(form.id, payload)
      else await adminAPI.createPostCategory(payload)
      notifySuccess(t('admin.common.operationSuccess'))
    })
  }

  const toggleActive = async (c: PostCategory) => {
    const next = !c.is_active
    c.is_active = next
    categories.value = [...categories.value]
    try {
      await adminAPI.patchPostCategoryStatus(c.id, next)
      notifySuccess(next ? t('admin.categories.status.activatedTip') : t('admin.categories.status.deactivatedTip'))
    } catch {
      c.is_active = !next
      categories.value = [...categories.value]
    }
  }

  const remove = async (c: PostCategory) => {
    const ok = await confirmAction({
      description: t('admin.postCategories.confirmDelete', { name: getLocalizedText(c.name) }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deletePostCategory(c.id)
      notifySuccess(t('admin.common.operationSuccess'))
      await fetchCategories()
    } catch {
      /* toast shown by client */
    }
  }

  return { loading, categories, hierarchy, form, modal, errors, canChooseParent, parentOptions, fetchCategories, openCreate, openEdit, openEditById, submit, toggleActive, remove }
}
