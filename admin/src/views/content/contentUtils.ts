import type { LocalizedText } from '@/api/types'
import type { PostCategory } from '@/utils/postCategory'
import { toRFC3339 } from '@/utils/format'

export type PostType = 'blog' | 'notice'

export const normalizePostType = (raw: unknown): PostType => (raw === 'notice' ? 'notice' : 'blog')

export interface PostCategoryOption {
  id: number
  name: LocalizedText
  depth: number
  /** Only active leaf categories can be assigned to a post. */
  selectable: boolean
}

/** Port of Posts.vue `buildCategoryOptions`: parent-first ordering, depth clamped to 1, leaf+active selectable. */
export function buildPostCategoryOptions(flat: PostCategory[]): PostCategoryOption[] {
  const ids = new Set(flat.map((c) => c.id))
  const order = new Map(flat.map((c, i) => [c.id, i]))
  const children = new Map<number | null, PostCategory[]>()
  for (const c of flat) {
    const pid = c.parent_id && ids.has(c.parent_id) ? c.parent_id : null
    const list = children.get(pid) ?? []
    list.push(c)
    children.set(pid, list)
  }
  for (const list of children.values()) list.sort((a, b) => (order.get(a.id) ?? 0) - (order.get(b.id) ?? 0))

  const result: PostCategoryOption[] = []
  const visited = new Set<number>()
  const walk = (cat: PostCategory, depth: number) => {
    if (visited.has(cat.id)) return
    visited.add(cat.id)
    const childCount = (children.get(cat.id) ?? []).length
    result.push({ id: cat.id, name: cat.name, depth: Math.min(depth, 1), selectable: childCount === 0 && Boolean(cat.is_active) })
    for (const child of children.get(cat.id) ?? []) walk(child, depth + 1)
  }
  for (const root of children.get(null) ?? []) walk(root, 0)
  for (const c of flat) if (!visited.has(c.id)) walk(c, 0)
  return result
}

/** Media card size text, same thresholds as the original. */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export interface BannerForm {
  id: number
  name: string
  position: string
  title: Record<'zh-CN' | 'zh-TW' | 'en-US', string>
  subtitle: Record<'zh-CN' | 'zh-TW' | 'en-US', string>
  image: string
  mobile_image: string
  link_type: string
  link_value: string
  open_in_new_tab: boolean
  is_active: boolean
  start_at: string
  end_at: string
  sort_order: number | ''
}

/** Same payload the original Banners.vue sends (link_value cleared for `none`, datetimes as ISO or ''). */
export function buildBannerPayload(form: BannerForm) {
  return {
    name: form.name,
    position: form.position,
    title: { ...form.title },
    subtitle: { ...form.subtitle },
    image: form.image,
    mobile_image: form.mobile_image,
    link_type: form.link_type,
    link_value: form.link_type === 'none' ? '' : form.link_value,
    open_in_new_tab: form.open_in_new_tab,
    is_active: form.is_active,
    start_at: toRFC3339(form.start_at) ?? '',
    end_at: toRFC3339(form.end_at) ?? '',
    sort_order: Number(form.sort_order || 0),
  }
}

/** Labels like '标题 ({lang}) *' rendered with lang='' → '标题' (LocalizedInput shows the locale tabs itself). */
export const stripLangLabel = (label: string) =>
  label
    .replace(/\s*[（(]\s*[）)]/g, '')
    .replace(/\s*\*\s*$/, '')
    .trim()

/** i18n key of the post dialog's submit button; a new draft is not "published now" (QA-A23). */
export function postSubmitLabelKey(isEditing: boolean, isPublished: boolean): string {
  if (isEditing) return 'admin.posts.actions.saveChanges'
  return isPublished ? 'admin.posts.actions.publishNow' : 'admin.posts.actions.saveDraft'
}
