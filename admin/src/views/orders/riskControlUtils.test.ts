import { describe, expect, it } from 'vitest'
import { buildRiskControlPayload, defaultRiskControlForm, normalizeLines, parseRiskControlConfig } from './riskControlUtils'

describe('risk control config', () => {
  it('empty config keeps recommended defaults', () => {
    expect(parseRiskControlConfig({})).toEqual(defaultRiskControlForm())
    expect(parseRiskControlConfig(null)).toEqual(defaultRiskControlForm())
  })

  it('parses nested v2 config with fallbacks', () => {
    const form = parseRiskControlConfig({
      enabled: true,
      common: { ip_blacklist: ['1.1.1.1', '2.2.2.0/24'] },
      guest: { enabled: false, max_pending_orders_per_ip: 4, rate_limit: { enabled: false, max_requests: 9 } },
      member: { max_pending_orders_per_user: 0 },
    })
    expect(form.enabled).toBe(true)
    expect(form.common.ip_blacklist_text).toBe('1.1.1.1\n2.2.2.0/24')
    expect(form.guest.enabled).toBe(false)
    expect(form.guest.max_pending_orders_per_ip).toBe(4)
    expect(form.guest.max_quantity_per_product_per_order).toBe(1)
    expect(form.guest.rate_limit).toEqual({ enabled: false, window_seconds: 60, max_requests: 9, block_seconds: 120 })
    expect(form.member.enabled).toBe(true)
    expect(form.member.max_pending_orders_per_user).toBe(0)
    expect(form.member.rate_limit.max_requests).toBe(10)
  })

  it('parses legacy flat config', () => {
    const form = parseRiskControlConfig({ enabled: true, ip_blacklist: ['9.9.9.9'], max_pending_orders_per_ip: 7, order_rate_limit: { enabled: true } })
    expect(form.guest.max_pending_orders_per_ip).toBe(7)
    expect(form.guest.payment_expire_minutes).toBe(0)
    expect(form.member.max_pending_orders_per_user).toBe(3)
    expect(form.member.max_pending_orders_per_ip).toBe(7)
    expect(form.guest.rate_limit).toEqual({ enabled: true, window_seconds: 60, max_requests: 5, block_seconds: 120 })
  })

  it('builds v2 payload with deduped blacklist and numeric values', () => {
    const form = defaultRiskControlForm()
    form.common.ip_blacklist_text = ' 1.1.1.1 \n\n1.1.1.1\n2.2.2.2'
    form.guest.max_pending_orders_per_ip = ''
    const payload = buildRiskControlPayload(form)
    expect(payload.version).toBe(2)
    expect(payload.common.ip_blacklist).toEqual(['1.1.1.1', '2.2.2.2'])
    expect(payload.guest.max_pending_orders_per_ip).toBe(0)
    expect(payload.member.rate_limit).toEqual({ enabled: false, window_seconds: 60, max_requests: 10, block_seconds: 120 })
  })

  it('normalizeLines', () => {
    expect(normalizeLines('a\n a \nb\n')).toEqual(['a', 'b'])
  })
})
