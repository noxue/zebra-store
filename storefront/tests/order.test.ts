import { describe, expect, it } from 'vitest'
import { encodeGuestAuthorization } from '@/api/order'
import {
  buildManualFormDataPayload,
  buildManualFormProducts,
  compileManualRegex,
  syncManualFormValues,
  validateManualForm,
  type ManualFormValues,
} from '@/utils/manualForm'
import {
  buildCryptoDetails,
  filterSupportedChannels,
  fulfillmentDeliveryLines,
  intersectAllowedChannelIds,
  isChannelDisabledForAmount,
  itemPaidAmount,
  manualSubmissionRows,
  readQueryValue,
  resolveChildStatus,
  resolveRechargeReturnNo,
  restrictChannels,
  splitWalletPayment,
} from '@/utils/orderPayment'
import { splitRechargePayload } from '@/composables/useRechargeOrderDetail'
import type { Order, OrderItem, PaymentChannel } from '@/api/types'

const t = (key: string, params: Record<string, unknown>) => `${key}|${String(params.name ?? '')}`

const schemaLine = {
  productId: 7,
  title: { 'zh-CN': '商品' },
  fulfillmentType: 'manual',
  manualFormSchema: {
    fields: [
      { key: 'mail', type: 'email', required: true, label: { 'zh-CN': '邮箱' } },
      { key: 'qty', type: 'number', min: 1, max: 5 },
      { key: 'region', type: 'radio', options: ['US', 'JP'] },
      { key: 'tags', type: 'checkbox', options: ['a', 'b'], required: true },
      { key: 'code', type: 'text', regex: '/^[A-Z]{3}$/' },
      { key: 'bad', type: 'unknown-type' },
    ],
  },
}

describe('LQA-I2 upstream products shown as auto still get their buyer form', () => {
  it('builds the form from the schema, validates the regex and shows the supplier message', () => {
    const line = {
      productId: 9,
      title: { 'zh-CN': 'ACG' },
      fulfillmentType: 'auto',
      manualFormSchema: {
        fields: [
          {
            key: 'account',
            type: 'text',
            required: true,
            label: { 'zh-CN': '账号' },
            regex: '^[0-9]{5,10}$',
            error_message: { 'zh-CN': '账号必须是5-10位数字' },
          },
        ],
      },
    }
    const products = buildManualFormProducts([line])
    expect(products).toHaveLength(1)
    const bad = validateManualForm(products, { '9': { account: 'abc' } }, t, 'zh-CN')
    expect(bad.errors['9:account']).toBe('账号必须是5-10位数字')
    const missing = validateManualForm(products, { '9': { account: '' } }, t, 'zh-CN')
    expect(missing.errors['9:account']).toBe('checkout.manualFormFieldRequired|账号')
    const good = validateManualForm(products, { '9': { account: '123456' } }, t, 'zh-CN')
    expect(good.valid).toBe(true)
    expect(buildManualFormDataPayload(products, { '9': { account: '123456' } })).toEqual({ '9': { account: '123456' } })
  })
})

describe('manual form', () => {
  const products = buildManualFormProducts([schemaLine, { ...schemaLine }, { ...schemaLine, productId: 8, fulfillmentType: 'auto', manualFormSchema: null }])

  it('groups by product, counts SKUs and drops unknown field types', () => {
    expect(products).toHaveLength(1)
    expect(products[0].itemKey).toBe('7')
    expect(products[0].skuCount).toBe(2)
    expect(products[0].fields.map((f) => f.key)).toEqual(['mail', 'qty', 'region', 'tags', 'code'])
  })

  it('validates required, email, range, options and regex', () => {
    const data: ManualFormValues = { '7': { mail: 'nope', qty: '9', region: 'CN', tags: [], code: 'abc' } }
    const res = validateManualForm(products, data, t, 'zh-CN')
    expect(res.valid).toBe(false)
    expect(res.errors['7:mail']).toBe('checkout.manualFormFieldInvalid|邮箱')
    expect(res.errors['7:qty']).toBe('checkout.manualFormFieldNumberRange|qty')
    expect(res.errors['7:region']).toBe('checkout.manualFormFieldOptionInvalid|region')
    expect(res.errors['7:tags']).toBe('checkout.manualFormFieldRequired|tags')
    expect(res.errors['7:code']).toBe('checkout.manualFormFieldInvalid|code')
    expect(res.firstError).toBe('checkout.manualFormFieldInvalid|邮箱')
  })

  it('accepts valid input and builds payload without empty values', () => {
    const data: ManualFormValues = { '7': { mail: 'a@b.co', qty: '', region: 'JP', tags: ['b'], code: 'ABC' } }
    expect(validateManualForm(products, data, t, 'zh-CN').valid).toBe(true)
    expect(buildManualFormDataPayload(products, data)).toEqual({ '7': { mail: 'a@b.co', region: 'JP', tags: ['b'], code: 'ABC' } })
  })

  it('syncs values to the current schema', () => {
    const synced = syncManualFormValues(products, { '7': { mail: 'x', stale: 'y' }, '99': { a: 'b' } })
    expect(synced).toEqual({ '7': { mail: 'x', qty: '', region: '', tags: [], code: '' } })
  })

  it('compiles /pattern/flags and rejects invalid regex', () => {
    expect(compileManualRegex('/^a$/i')?.test('A')).toBe(true)
    expect(compileManualRegex('^\\d+$')?.test('12')).toBe(true)
    expect(compileManualRegex('([')).toBeNull()
  })
})

describe('guest authorization header', () => {
  it('base64url encodes lower(email)\\npassword without padding', () => {
    expect(encodeGuestAuthorization(' G@Test.com ', 'pass1234')).toBe('Guest Z0B0ZXN0LmNvbQpwYXNzMTIzNA')
  })
})

const ch = (id: number, extra: Partial<PaymentChannel> = {}): PaymentChannel => ({
  id,
  name: `c${id}`,
  channel_type: 'alipay',
  provider_type: 'epay',
  interaction_mode: 'redirect',
  ...extra,
})

describe('payment channels', () => {
  it('filters unsupported epay sub-types', () => {
    const list = [ch(1), ch(2, { channel_type: 'usdt' }), ch(3, { provider_type: 'official', channel_type: 'paypal' })]
    expect(filterSupportedChannels(list).map((c) => c.id)).toEqual([1, 3])
  })

  it('intersects per-product allowed channel ids', () => {
    expect(intersectAllowedChannelIds([undefined, []])).toBeNull()
    expect(intersectAllowedChannelIds([[1, 2, 3], null, [2, 3]])).toEqual([2, 3])
    expect(restrictChannels([ch(1), ch(2)], [])).toEqual([])
    expect(restrictChannels([ch(1), ch(2)], [2]).map((c) => c.id)).toEqual([2])
  })

  it('disables channels outside min/max unless hidden', () => {
    const limited = ch(1, { min_amount: '10.00', max_amount: '100.00' })
    expect(isChannelDisabledForAmount(limited, 500, true)).toBe(true)
    expect(isChannelDisabledForAmount(limited, 5000, true)).toBe(false)
    expect(isChannelDisabledForAmount(limited, 500, false)).toBe(false)
    expect(isChannelDisabledForAmount({ ...limited, hide_amount_out_range: true }, 500, true)).toBe(false)
  })

  it('splits wallet and online amounts', () => {
    expect(splitWalletPayment('78.40', '25.00', true)).toEqual({ walletCents: 2500, onlineCents: 5340 })
    expect(splitWalletPayment('78.40', '100', true)).toEqual({ walletCents: 7840, onlineCents: 0 })
    expect(splitWalletPayment('78.40', '100', false)).toEqual({ walletCents: 0, onlineCents: 7840 })
  })
})

describe('payment return query', () => {
  it('reads amp;-prefixed and case-insensitive keys', () => {
    expect(readQueryValue({ 'amp;order_no': 'DJ1' }, 'order_no')).toBe('DJ1')
    expect(readQueryValue({ ORDER_NO: ['', 'DJ2'] }, 'order_no')).toBe('DJ2')
  })

  it('detects recharge returns', () => {
    expect(resolveRechargeReturnNo({ order_no: 'WR123' })).toBe('WR123')
    expect(resolveRechargeReturnNo({ order_no: 'DJ123' })).toBe('')
    expect(resolveRechargeReturnNo({ recharge_no: 'X1' })).toBe('X1')
  })

  it('builds crypto details', () => {
    const d = buildCryptoDetails({ token_id: 'tron-usdt', chain: 'trc20', chain_amount: '1.5', wallet_address: 'T123' })
    expect(d.map((x) => [x.key, x.value])).toEqual([
      ['token', 'USDT'],
      ['chain', 'TRON'],
      ['amount', '1.5'],
      ['wallet_address', 'T123'],
    ])
    expect(d[0].detail).toBe('tron-usdt')
  })
})

describe('order display', () => {
  const item: OrderItem = { title: {}, quantity: 1, unit_price: '78.40', total_price: '78.40', coupon_discount_amount: '7.84' }
  it('computes item paid amount after coupon', () => {
    expect(itemPaidAmount(item)).toBe('70.56')
  })

  it('resolves child refund status', () => {
    const child = { order_no: 'c', status: 'delivered', currency: 'CNY', total_amount: '10.00', created_at: '', refunded_amount: '10.00' } as Order
    expect(resolveChildStatus(child, null)).toBe('refunded')
    expect(resolveChildStatus({ ...child, refunded_amount: '1.00' }, null)).toBe('partially_refunded')
    expect(resolveChildStatus({ ...child, refunded_amount: '0' }, { ...child, status: 'refunded' })).toBe('refunded')
  })

  it('formats delivery lines and manual submission rows', () => {
    expect(fulfillmentDeliveryLines({ delivery_data: { note: 'hi', entries: [{ key: 'k', value: 'v' }, { key: '', value: 'x' }] } })).toEqual(['hi', 'k: v', 'x'])
    expect(manualSubmissionRows({ extra: 1, mail: 'a@b' }, { fields: [{ key: 'mail', type: 'email', label: { 'en-US': 'Mail' } }] }, 'en-US')).toEqual([
      { key: 'mail', label: 'Mail', value: 'a@b' },
      { key: 'extra', label: 'extra', value: '1' },
    ])
  })

  it('splits recharge payload into recharge + payment', () => {
    const res = splitRechargePayload({ recharge: null, payment_id: 7, pay_url: 'u', interaction_mode: 'redirect' })
    expect(res.payment?.id).toBe(7)
    expect(res.payment?.pay_url).toBe('u')
  })
})
