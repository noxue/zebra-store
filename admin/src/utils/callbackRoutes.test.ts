import { describe, expect, it } from 'vitest'
import { buildCallbackRoutesSavePayload, getCallbackRouteDisplayValue, validateCallbackRoutes } from './callbackRoutes'

const empty = buildCallbackRoutesSavePayload({})

describe('callback routes', () => {
  it('stores defaults as empty strings and trims trailing slashes', () => {
    const payload = buildCallbackRoutesSavePayload({ payment_callback: '/api/v1/payments/callback/', stripe_webhook: '/api/hooks/stripe/' })
    expect(payload.payment_callback).toBe('')
    expect(payload.stripe_webhook).toBe('/api/hooks/stripe')
    expect(getCallbackRouteDisplayValue('paypal_webhook', '')).toBe('/api/v1/payments/webhook/paypal')
  })

  it('accepts defaults and custom api paths', () => {
    expect(validateCallbackRoutes(empty)).toBeNull()
    expect(validateCallbackRoutes({ ...empty, payment_callback: '/api/pay/cb' })).toBeNull()
  })

  it('requires the /api/ prefix', () => {
    expect(validateCallbackRoutes({ ...empty, payment_callback: '/pay/cb' })).toBe('mustStartWithApi')
  })

  it('rejects reserved prefixes in both directions', () => {
    expect(validateCallbackRoutes({ ...empty, paypal_webhook: '/api/v1/admin/hook' })).toBe('conflictWithSystem')
    expect(validateCallbackRoutes({ ...empty, paypal_webhook: '/api/v1/public' })).toBe('conflictWithSystem')
    expect(validateCallbackRoutes({ ...empty, paypal_webhook: '/api/v1/upstream/api/x' })).toBe('conflictWithSystem')
  })

  it('rejects duplicates', () => {
    expect(validateCallbackRoutes({ ...empty, paypal_webhook: '/api/hook', stripe_webhook: '/api/hook' })).toBe('duplicatePath')
  })
})
