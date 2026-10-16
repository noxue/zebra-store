import { afterEach, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { SearchPicker, type SearchOption } from './SearchPicker'

afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
})

it('shows partial search matches and emits the chosen record, never a typed ID', async () => {
  vi.useFakeTimers()
  const search = vi.fn(async (query: string): Promise<SearchOption[]> =>
    query === 'llr' ? [{ id: 5, label: 'llry · llry@live.cn (R#5)' }] : [],
  )
  const wrapper = mount(SearchPicker, { props: { modelValue: null, search, placeholder: 'Search reseller', emptyText: 'None', loadingText: 'Loading' } })
  await wrapper.get('input').setValue('llr')
  await vi.advanceTimersByTimeAsync(250)
  await flushPromises()
  expect(search).toHaveBeenCalledWith('llr')
  await wrapper.get('[role="option"]').trigger('click')
  expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toEqual({ id: 5, label: 'llry · llry@live.cn (R#5)' })
  wrapper.unmount()
})

it('ignores an older search response after a newer query completes', async () => {
  vi.useFakeTimers()
  let completeOld: ((value: SearchOption[]) => void) | undefined
  const search = vi.fn((query: string): Promise<SearchOption[]> => query === 'old'
    ? new Promise((resolve) => { completeOld = resolve })
    : Promise.resolve([{ id: 9, label: 'New product' }]))
  const wrapper = mount(SearchPicker, { props: { modelValue: null, search, placeholder: 'Search product', emptyText: 'None', loadingText: 'Loading' } })
  await wrapper.get('input').setValue('old')
  await vi.advanceTimersByTimeAsync(250)
  await wrapper.get('input').setValue('new')
  await vi.advanceTimersByTimeAsync(250)
  await flushPromises()
  completeOld?.([{ id: 8, label: 'Old product' }])
  await flushPromises()
  expect(wrapper.get('[role="option"]').text()).toBe('New product')
  wrapper.unmount()
})
