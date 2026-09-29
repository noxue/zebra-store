// Regression tests for bugfix-lessons §21 items (FE-01, FE-05, FE-06, FE-11, FE-12, FE-26,
// DLV-02 / UPL-02 storefront sanitising).
import { describe, expect, it } from 'vitest'
import { ref } from 'vue'
import type { PaymentCreateResult } from '@/api/types'
import { usePaymentLink } from '@/composables/usePaymentLink'
import { sanitizeHtml } from '@/utils/content'
import { amountToCents, calculateFeeCents, addAmounts } from '@/utils/money'
import { isSafeNavUrl } from '@/utils/navUrl'
import { PAYMENT_RETURN_MARKERS, readQueryFlag, readQueryValue, splitWalletPayment } from '@/utils/orderPayment'
import {
  getCachedPaymentRestorePolicy,
  getPaymentResetPolicy,
  isMobilePaymentDevice,
  resolvePaymentLinkNavigationTarget,
  resolvePaymentPresentationMode,
  shouldAutoOpenPaymentLink,
} from '@/utils/paymentResumePolicy'

/** Query object as vue-router builds it from a raw search string. */
const queryOf = (search: string) => Object.fromEntries(new URLSearchParams(search).entries())

describe('FE-01 auto redirect opens in the current tab; wap/page are redirects', () => {
  it('automatic navigation stays in the tab, a manual click opens a new window', () => {
    expect(resolvePaymentLinkNavigationTarget(true)).toBe('current-tab')
    expect(resolvePaymentLinkNavigationTarget(false)).toBe('new-window')
  })
  it('wap and page behave like redirect, qr shows a code', () => {
    expect(resolvePaymentPresentationMode('wap')).toBe('redirect')
    expect(resolvePaymentPresentationMode('PAGE')).toBe('redirect')
    expect(resolvePaymentPresentationMode('qr')).toBe('qr')
    expect(shouldAutoOpenPaymentLink({ interaction_mode: 'wap', pay_url: 'https://p/x' })).toBe(true)
  })
  it('Huifu direct mode stays inline on desktop and opens the hosted page on mobile', () => {
    const payment = { provider_type: 'huifu', interaction_mode: 'qr', pay_url: 'https://pay.huifu.example/x' }
    expect(isMobilePaymentDevice(1280, 'Mozilla/5.0')).toBe(false)
    expect(isMobilePaymentDevice(390, 'Mozilla/5.0 (iPhone)')).toBe(true)
    expect(isMobilePaymentDevice(1024, 'Mozilla/5.0 (Macintosh)', 5)).toBe(true)
    expect(resolvePaymentPresentationMode('qr', 'huifu', false)).toBe('qr')
    expect(resolvePaymentPresentationMode('qr', 'huifu', true)).toBe('redirect')
    expect(shouldAutoOpenPaymentLink(payment, false)).toBe(false)
    expect(shouldAutoOpenPaymentLink(payment, true)).toBe(true)
    expect(shouldAutoOpenPaymentLink({ ...payment, interaction_mode: 'redirect' }, false)).toBe(true)
  })
})

describe('FE-05 changing the payment method never resumes the previous payment', () => {
  it('change_payment_method clears the channel and stops watching without resuming', () => {
    expect(getPaymentResetPolicy('change_payment_method')).toEqual({
      resumeLatestPayment: false,
      clearSelectedChannel: true,
      stopActivePaymentWatch: true,
    })
    expect(getCachedPaymentRestorePolicy().autoOpenPayLink).toBe(false)
  })
  it('qr mode or a blank pay_url never auto-opens', () => {
    expect(shouldAutoOpenPaymentLink({ interaction_mode: 'qr', pay_url: 'https://p/x' })).toBe(false)
    expect(shouldAutoOpenPaymentLink({ interaction_mode: 'redirect', pay_url: '   ' })).toBe(false)
    expect(shouldAutoOpenPaymentLink({ interaction_mode: 'redirect', pay_url: 'https://p/x', fee_policy: 'customer_surcharge' })).toBe(false)
  })
  it('a bepusdt return is recognised as a gateway return', () => {
    const query = queryOf('order_no=DJ1&bepusdt_return=1')
    expect(PAYMENT_RETURN_MARKERS.some((m) => readQueryValue(query, m) === '1')).toBe(true)
  })
})

describe('FE-06 custom navigation URLs are restricted', () => {
  it('drops script / protocol-relative URLs', () => {
    expect(isSafeNavUrl('javascript:alert(1)', true)).toBe(false)
    expect(isSafeNavUrl('javascript:alert(1)', false)).toBe(false)
    expect(isSafeNavUrl('//evil.com', false)).toBe(false)
    expect(isSafeNavUrl('/\\evil.com', false)).toBe(false)
    expect(isSafeNavUrl('https://docs.example.com', true)).toBe(true)
    expect(isSafeNavUrl('/blog', false)).toBe(true)
    expect(isSafeNavUrl('blog', false)).toBe(true)
  })
})

describe('FE-11 gateway return query parsing', () => {
  it('accepts &amp;-escaped queries, out_trade_no and yes/1 flags', () => {
    const q = queryOf('order_no=DJ1&amp;guest=1&amp;epay_return=1')
    expect(readQueryValue(q, 'order_no')).toBe('DJ1')
    expect(readQueryFlag(q, 'guest')).toBe(true)
    expect(readQueryValue(q, 'epay_return')).toBe('1')
    const outTradeNo = queryOf('out_trade_no=DJ2')
    expect(readQueryValue(outTradeNo, 'order_no') || readQueryValue(outTradeNo, 'out_trade_no')).toBe('DJ2')
    expect(readQueryFlag(queryOf('guest=yes'), 'guest')).toBe(true)
    expect(readQueryFlag(queryOf('guest=no'), 'guest')).toBe(false)
  })
})

describe('FE-12 money in integer cents', () => {
  it('rounds half up and computes fees and wallet splits exactly', () => {
    expect(amountToCents('10.005')).toBe(1001)
    expect(calculateFeeCents(1000, 150)).toBe(15)
    const total = addAmounts('0.10', '0.20')
    expect(total).toBe('0.30')
    expect(splitWalletPayment(total, '0.30', true)).toEqual({ walletCents: 30, onlineCents: 0 })
  })
})

describe('FE-26 missing QR code falls back to the pay link', () => {
  it('encodes pay_url in qr mode when qr_code is empty', () => {
    const payment = ref({ interaction_mode: 'qr', qr_code: '', pay_url: 'https://p/x' } as unknown as PaymentCreateResult)
    const link = usePaymentLink(payment)
    expect(link.showQRCode.value).toBe(true)
    expect(link.qrDisplayContent.value).toBe('https://p/x')
    expect(link.qrUsingPayLinkFallback.value).toBe(true)
  })
})

describe('DLV-02 / UPL-02 rich HTML is sanitised before rendering', () => {
  it('removes event handlers, javascript: links and scripts', () => {
    const out = sanitizeHtml('<p>ok</p><img src=x onerror=alert(1)><a href="javascript:alert(1)">x</a><script>alert(1)</script>')
    expect(out).toContain('<p>ok</p>')
    expect(out).not.toMatch(/onerror/i)
    expect(out).not.toMatch(/javascript:/i)
    expect(out).not.toMatch(/<script/i)
  })
})
