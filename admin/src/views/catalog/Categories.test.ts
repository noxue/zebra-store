import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AdminCategory } from '@/api/types'
import i18n from '@/i18n'
import Categories from './Categories'

const api = vi.hoisted(() => ({
  getCategories: vi.fn(),
  updateCategory: vi.fn(),
  patchCategoryActive: vi.fn(),
  createCategory: vi.fn(),
}))
vi.mock('@/api/admin', () => ({ adminAPI: api }))
vi.mock('vue-router', () => ({ useRoute: () => ({ query: { category_id: '7' } }) }))
vi.mock('@/utils/notify', () => ({ notifySuccess: vi.fn() }))

let category: AdminCategory
let wrapper: VueWrapper | undefined

const DialogStub = defineComponent({
  props: { modelValue: Boolean },
  setup: (props, { slots }) => () => props.modelValue ? h('div', [slots.default?.(), slots.footer?.()]) : null,
})

async function openEditor(active: boolean) {
  category.is_active = active
  wrapper = mount(Categories, {
    global: { plugins: [i18n], stubs: { MediaPicker: true, ZsDialog: DialogStub } },
  })
  await flushPromises()
  return wrapper
}

describe('category editor status (ORD-09)', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    i18n.global.locale.value = 'zh-CN'
    category = { id: 7, parent_id: 0, name: { 'zh-CN': '游戏点卡' }, slug: 'game-cards', icon: '', sort_order: 3, is_active: true, created_at: '' }
    api.getCategories.mockImplementation(async () => ({ data: [{ ...category }] }))
    api.updateCategory.mockImplementation(async () => ({ data: { ...category } }))
    api.patchCategoryActive.mockImplementation(async (_id: number, active: boolean) => {
      category.is_active = active
      return { data: { ...category } }
    })
  })

  afterEach(() => { wrapper?.unmount() })

  it.each([true, false])('loads saved status %s and persists a switch change on save', async (active) => {
    const view = await openEditor(active)
    const toggle = view.get('[role="switch"]')
    expect(toggle.attributes('aria-checked')).toBe(String(active))
    expect(view.text()).toContain('启用分类')
    expect(view.text()).toContain('子分类商品也会隐藏')
    await toggle.trigger('click')
    expect(api.patchCategoryActive).not.toHaveBeenCalled()
    await view.get('form').trigger('submit')
    await flushPromises()
    expect(api.updateCategory).toHaveBeenCalledWith(7, expect.objectContaining({ slug: 'game-cards' }))
    expect(api.updateCategory.mock.calls[0]![1]).not.toHaveProperty('is_active')
    expect(api.patchCategoryActive).toHaveBeenCalledExactlyOnceWith(7, !active)
    expect(view.find('[role="switch"]').exists()).toBe(false)
    expect(view.findAll('tbody button').some((button) => button.text() === (!active ? '已启用' : '已停用'))).toBe(true)
  })

  it('preserves an inactive category when editing other fields', async () => {
    const view = await openEditor(false)
    await view.get('form').trigger('submit')
    await flushPromises()
    expect(api.updateCategory).toHaveBeenCalledOnce()
    expect(api.patchCategoryActive).not.toHaveBeenCalled()
    expect(category.is_active).toBe(false)
  })

  it('cancels a status change without saving', async () => {
    const view = await openEditor(true)
    await view.get('[role="switch"]').trigger('click')
    await view.findAll('button').find((button) => button.text() === '取消')!.trigger('click')
    expect(api.updateCategory).not.toHaveBeenCalled()
    expect(api.patchCategoryActive).not.toHaveBeenCalled()
    await view.findAll('button').find((button) => button.text() === '编辑')!.trigger('click')
    expect(view.get('[role="switch"]').attributes('aria-checked')).toBe('true')
  })

  it('keeps the editor open on status save failure and supports retry', async () => {
    api.patchCategoryActive.mockRejectedValueOnce(new Error('状态保存失败'))
    const view = await openEditor(true)
    await view.get('[role="switch"]').trigger('click')
    await view.get('form').trigger('submit')
    await flushPromises()
    expect(view.text()).toContain('状态保存失败')
    expect(view.get('[role="switch"]').attributes('aria-checked')).toBe('false')
    expect(category.is_active).toBe(true)
    await view.get('form').trigger('submit')
    await flushPromises()
    expect(category.is_active).toBe(false)
    expect(view.find('[role="switch"]').exists()).toBe(false)
  })
})
