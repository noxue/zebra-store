import { afterEach, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import WholesalePrices from './WholesalePrices'
import { ListPagination } from '@/components/ui/ListPagination'
import { adminAPI } from '@/api/admin'
import i18n from '@/i18n'
afterEach(() => vi.restoreAllMocks())
it('FE-15 wires pagination and page-size events to wholesale product queries', async () => {
  setActivePinia(createPinia())
  vi.spyOn(adminAPI, 'getSettings').mockResolvedValue({ status_code: 0, msg: '', data: {} })
  const get = vi.spyOn(adminAPI, 'getProducts').mockResolvedValue({ status_code: 0, msg: '', data: [], pagination: { page: 1, page_size: 20, total: 80, total_page: 4 } })
  const wrapper = mount(WholesalePrices, { global: { plugins: [i18n], stubs: { Teleport: true } } })
  await flushPromises()
  const paging = wrapper.getComponent(ListPagination)
  paging.vm.$emit('changePage', 2)
  await flushPromises()
  expect(get).toHaveBeenLastCalledWith(expect.objectContaining({ page: 2, page_size: 20 }))
  paging.vm.$emit('changePageSize', 50)
  await flushPromises()
  expect(get).toHaveBeenLastCalledWith(expect.objectContaining({ page: 1, page_size: 50 }))
  wrapper.unmount()
})
