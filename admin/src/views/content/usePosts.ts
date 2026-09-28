import { computed, reactive, ref, watch, type Ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminPost, AdminProduct, LocalizedText } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useCrudModal } from '@/composables/useCrudModal'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { getLocalizedText, toLocalizedForm } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import type { PostCategory } from '@/utils/postCategory'
import { buildPostCategoryOptions, type PostCategoryOption, type PostType } from './contentUtils'

/** Full-width space: survives inside native <option> text (regular spaces are collapsed). */
const INDENT = String.fromCharCode(0x3000)

export interface RelatedProductRef {
  id: number
  slug: string
  title: LocalizedText
  image?: string
}

type Localized = Record<'zh-CN' | 'zh-TW' | 'en-US', string>

export interface PostForm {
  id: number
  title: Localized
  slug: string
  summary: Localized
  content: Localized
  type: PostType
  thumbnail: string
  is_published: boolean
  category_id: number | null
  product_ids: number[]
}

const emptyForm = (type: PostType): PostForm => ({
  id: 0,
  title: toLocalizedForm(null),
  slug: '',
  summary: toLocalizedForm(null),
  content: toLocalizedForm(null),
  type,
  thumbnail: '',
  is_published: true,
  category_id: null,
  product_ids: [],
})

const toProductRef = (p: Partial<AdminProduct> & { id: number; image?: string }): RelatedProductRef => ({
  id: p.id,
  slug: p.slug ?? '',
  title: p.title ?? {},
  image: p.image ?? p.images?.[0],
})

/** Page logic for 文章/公告 (list per type tab, editor dialog with related products for blog). */
export function usePosts(type: Ref<PostType>) {
  const t = i18n.global.t
  const categories = ref<PostCategoryOption[]>([])
  const form = reactive<PostForm>(emptyForm(type.value))

  const list = useListPage<AdminPost>({
    pageSize: 10,
    fetchFn: (page, pageSize) => adminAPI.getPosts({ page, page_size: pageSize, type: type.value }),
  })
  const modal = useCrudModal({ onSuccess: () => void list.refresh() })
  const { errors, validate, clearErrors } = useFormValidation<{ slug: string; type: string; title: string }>({
    slug: [rules.required(t('admin.common.required'))],
    type: [rules.required(t('admin.common.required'))],
    title: [rules.required(t('admin.common.required'))],
  })

  // ---- categories ----
  const fetchCategories = async () => {
    try {
      const res = await adminAPI.getPostCategories()
      categories.value = buildPostCategoryOptions((res.data ?? []) as unknown as PostCategory[])
    } catch {
      categories.value = []
    }
  }
  const categoryName = (id?: number | null) => {
    if (!id) return '-'
    const cat = categories.value.find((c) => c.id === id)
    return cat ? getLocalizedText(cat.name) : '-'
  }
  const categoryOptions = computed(() => [
    { label: t('admin.posts.form.noCategory'), value: 0 },
    ...categories.value.map((c) => ({
      label: c.depth > 0 ? `${INDENT}└ ${getLocalizedText(c.name)}` : getLocalizedText(c.name),
      value: c.id,
      disabled: !c.selectable,
    })),
  ])

  // ---- related products ----
  const relatedProducts = ref<RelatedProductRef[]>([])
  const productSearch = ref('')
  const productSearchResults = ref<RelatedProductRef[]>([])
  const productSearchLoading = ref(false)
  let searchTimer: ReturnType<typeof setTimeout> | undefined
  let searchSeq = 0

  const resetRelated = () => {
    relatedProducts.value = []
    productSearch.value = ''
    productSearchResults.value = []
    productSearchLoading.value = false
  }

  const fetchRelated = async (postId: number) => {
    try {
      const res = await adminAPI.getPostRelatedProducts(postId)
      relatedProducts.value = (res.data ?? []).map((p) => toProductRef(p as AdminProduct & { image?: string }))
      form.product_ids = relatedProducts.value.map((p) => p.id)
    } catch {
      relatedProducts.value = []
      form.product_ids = []
    }
  }

  const runProductSearch = async () => {
    const keyword = productSearch.value.trim()
    const seq = ++searchSeq
    if (!keyword) {
      productSearchResults.value = []
      productSearchLoading.value = false
      return
    }
    productSearchLoading.value = true
    try {
      const res = await adminAPI.getProducts({ search: keyword, page: 1, page_size: 20 })
      if (seq === searchSeq) productSearchResults.value = (res.data ?? []).map(toProductRef)
    } catch {
      if (seq === searchSeq) productSearchResults.value = []
    } finally {
      if (seq === searchSeq) productSearchLoading.value = false
    }
  }
  watch(productSearch, () => {
    if (searchTimer) clearTimeout(searchTimer)
    searchTimer = setTimeout(() => void runProductSearch(), 300)
  })

  const addRelated = (p: RelatedProductRef) => {
    if (form.product_ids.includes(p.id)) return
    relatedProducts.value = [...relatedProducts.value, p]
    form.product_ids = [...form.product_ids, p.id]
  }
  const removeRelated = (id: number) => {
    relatedProducts.value = relatedProducts.value.filter((p) => p.id !== id)
    form.product_ids = form.product_ids.filter((pid) => pid !== id)
  }

  // ---- modal ----
  const openCreate = () => {
    clearErrors()
    resetRelated()
    Object.assign(form, emptyForm(type.value))
    modal.openCreate()
  }

  const openEdit = (post: AdminPost) => {
    clearErrors()
    resetRelated()
    Object.assign(form, {
      id: post.id,
      title: toLocalizedForm(post.title),
      slug: post.slug,
      summary: toLocalizedForm(post.summary),
      content: toLocalizedForm(post.content),
      type: post.type === 'notice' ? 'notice' : 'blog',
      thumbnail: post.thumbnail || '',
      is_published: post.is_published,
      category_id: post.category_id ?? null,
      product_ids: [],
    })
    modal.openEdit(post.id)
    if (post.type === 'blog') void fetchRelated(post.id)
  }

  const openEditById = async (raw: unknown) => {
    const id = Number(raw)
    if (!Number.isFinite(id) || id <= 0) return
    // The Go backend has no `GET /admin/posts/:id` (the original deep link 404s) — use the loaded list first.
    if (list.loading.value) await new Promise<void>((resolve) => watch(list.loading, (v) => !v && resolve(), { once: true }))
    const loaded = list.items.value.find((p) => p.id === id)
    if (loaded) return openEdit(loaded)
    try {
      const res = await adminAPI.getPost(id)
      if (res.data) openEdit(res.data)
    } catch {
      /* toast shown by client */
    }
  }

  const submit = () => {
    if (!validate({ slug: form.slug, type: form.type, title: form.title['zh-CN'] })) return
    void modal.handleSubmit(async () => {
      const isBlog = form.type === 'blog'
      const payload = {
        title: { ...form.title },
        slug: form.slug,
        summary: { ...form.summary },
        content: { ...form.content },
        type: form.type,
        thumbnail: form.thumbnail,
        is_published: form.is_published,
        category_id: isBlog ? form.category_id : null,
        product_ids: isBlog ? [...form.product_ids] : [],
      }
      if (modal.isEditing.value) await adminAPI.updatePost(form.id, { id: form.id, ...payload })
      else await adminAPI.createPost({ id: 0, ...payload })
      notifySuccess(t('admin.common.operationSuccess'))
    })
  }

  const remove = async (post: AdminPost) => {
    const ok = await confirmAction({
      description: t('admin.posts.confirmDelete', { name: getLocalizedText(post.title) }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deletePost(post.id)
      notifySuccess(t('admin.common.operationSuccess'))
      await list.refresh()
    } catch {
      /* toast shown by client */
    }
  }

  return {
    list,
    form,
    modal,
    errors,
    categories,
    categoryOptions,
    categoryName,
    fetchCategories,
    relatedProducts,
    productSearch,
    productSearchResults,
    productSearchLoading,
    addRelated,
    removeRelated,
    openCreate,
    openEdit,
    openEditById,
    submit,
    remove,
  }
}
