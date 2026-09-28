import { describe, expect, it } from 'vitest'
import { buildBannerPayload, buildPostCategoryOptions, formatFileSize, normalizePostType, stripLangLabel, type BannerForm } from './contentUtils'

describe('buildPostCategoryOptions', () => {
  it('orders children after their parent and marks only active leaves selectable', () => {
    const opts = buildPostCategoryOptions([
      { id: 3, parent_id: 1, name: { 'zh-CN': 'child' }, is_active: true },
      { id: 1, parent_id: null, name: { 'zh-CN': 'root' }, is_active: true },
      { id: 2, parent_id: null, name: { 'zh-CN': 'leaf-off' }, is_active: false },
      { id: 4, parent_id: 99, name: { 'zh-CN': 'orphan' }, is_active: true },
    ])
    expect(opts.map((o) => o.id)).toEqual([1, 3, 2, 4])
    expect(opts.map((o) => o.depth)).toEqual([0, 1, 0, 0])
    expect(opts.map((o) => o.selectable)).toEqual([false, true, false, true])
  })
})

describe('formatFileSize', () => {
  it('uses B / KB / MB', () => {
    expect(formatFileSize(512)).toBe('512 B')
    expect(formatFileSize(2048)).toBe('2.0 KB')
    expect(formatFileSize(3 * 1024 * 1024)).toBe('3.0 MB')
  })
})

describe('normalizePostType', () => {
  it('falls back to blog', () => {
    expect(normalizePostType('notice')).toBe('notice')
    expect(normalizePostType('x')).toBe('blog')
    expect(normalizePostType(undefined)).toBe('blog')
  })
})

describe('buildBannerPayload', () => {
  const base: BannerForm = {
    id: 0,
    name: 'n',
    position: 'home_hero',
    title: { 'zh-CN': 't', 'zh-TW': '', 'en-US': '' },
    subtitle: { 'zh-CN': '', 'zh-TW': '', 'en-US': '' },
    image: '/a.png',
    mobile_image: '',
    link_type: 'none',
    link_value: '/should-drop',
    open_in_new_tab: false,
    is_active: true,
    start_at: '',
    end_at: '',
    sort_order: '',
  }
  it('drops link value for none, empty dates to "", sort to number', () => {
    const p = buildBannerPayload(base)
    expect(p.link_value).toBe('')
    expect(p.start_at).toBe('')
    expect(p.sort_order).toBe(0)
    expect('id' in p).toBe(false)
  })
  it('keeps link and converts dates to ISO', () => {
    const p = buildBannerPayload({ ...base, link_type: 'internal', link_value: '/x', start_at: '2026-01-02T03:04', sort_order: 5 })
    expect(p.link_value).toBe('/x')
    expect(p.start_at).toMatch(/^2026-01-0\dT\d\d:04:00\.000Z$/)
    expect(p.sort_order).toBe(5)
  })
})

describe('stripLangLabel', () => {
  it('removes empty lang parens and the trailing required star', () => {
    expect(stripLangLabel('标题 () *')).toBe('标题')
    expect(stripLangLabel('分类名称（）')).toBe('分类名称')
    expect(stripLangLabel('Content ()')).toBe('Content')
  })
})

describe('postSubmitLabelKey', () => {
  it('QA-A23 a new draft is saved as a draft, not "published now"', async () => {
    const { postSubmitLabelKey } = await import('./contentUtils')
    expect(postSubmitLabelKey(false, false)).toBe('admin.posts.actions.saveDraft')
    expect(postSubmitLabelKey(false, true)).toBe('admin.posts.actions.publishNow')
    expect(postSubmitLabelKey(true, false)).toBe('admin.posts.actions.saveChanges')
  })
})
