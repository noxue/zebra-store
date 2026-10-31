import { mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { computed, ref } from 'vue'
import { describe, expect, it, vi } from 'vitest'
import i18n from '@/i18n'
import Dashboard from './Dashboard'

vi.mock('./dashboard/useDashboard', () => ({
  useDashboard: () => ({
    load: vi.fn(), loading: ref(false), error: ref(''),
    overview: ref({ currency: 'CNY', kpi: { owner_profit: '131.93', total_profit: '390.93', reseller_profit: '259.00' }, alerts: [] }),
    trends: ref(null), rankings: ref({ top_products: [{ product_id: 1, title: 'Fixture', profit: '30.00', owner_profit: '22.00' }], top_channels: [] }),
    points: computed(() => [{ date: '2026-10-31', orders_total: 1, orders_paid: 1, payments_success: 1, payments_failed: 0, gmv_paid: '100.00', profit: '2.00', owner_profit: '-6.00' }]),
    maxOrder: ref(1), maxPayment: ref(1), maxFunnel: ref(1), funnelSteps: ref([]),
    filters: { range: '7d', from: '', to: '' }, setRange: vi.fn(),
    inventoryAlerts: ref([]), cardConverterAlerts: ref([]), cardConverterBindings: ref([]), cardConverterPending: ref({ local: 0, upstream: 0, total: 0 }),
  }),
}))

describe('DASH-01 owner and total profit', () => {
  it('shows both amounts, net reseller earnings and negative daily owner profit', () => {
    i18n.global.locale.value = 'zh-CN'
    const view = mount(Dashboard, { global: { plugins: [i18n, createPinia()], stubs: { RouterLink: true } } })
    expect(view.text()).toContain('站长利润')
    expect(view.text()).toContain('总利润')
    expect(view.text()).toContain('131.93')
    expect(view.text()).toContain('390.93')
    expect(view.text()).toContain('259.00')
    expect(view.text()).toContain('登记成本')
    expect(view.get('table').text()).toContain('-6.00')
    expect(view.text()).toContain('22.00')
    expect(view.text()).toContain('30.00')
    view.unmount()
  })
})
