// Creates QA-owned marketing fixtures for storefront pricing flows (F-045..F-049). Undo with sf-teardown.mjs.
import fs from 'node:fs'
import { adm } from './sf-lib.mjs'
const out = {}
const iso = (d) => new Date(Date.now() + d * 86400000).toISOString()
out.coupon = await adm('coupons', { method: 'POST', body: { code: 'QASF10', type: 'percent', value: '10', min_amount: '20', max_discount: '0', usage_limit: 50, per_user_limit: 1, scope_ref_ids: [7, 6], starts_at: iso(-1), ends_at: iso(3), is_active: true } })
out.coupon2 = await adm('coupons', { method: 'POST', body: { code: 'QASFNOWHL', type: 'fixed', value: '3', min_amount: '0', max_discount: '0', usage_limit: 50, per_user_limit: 5, disabled_wholesale_price: true, scope_ref_ids: [7], starts_at: iso(-1), ends_at: iso(3), is_active: true } })
out.promo = await adm('promotions', { method: 'POST', body: { name: 'QA-SF 活动价 GCP', type: 'special_price', scope_ref_id: 6, value: '79.00', min_amount: '0', starts_at: iso(-1), ends_at: iso(3), is_active: true } })
out.wholesale = await adm('products/7/wholesale-prices', { method: 'PATCH', body: { wholesale_prices: [{ sku_id: 10, min_quantity: 3, unit_price: '8.00' }] } })
out.level = await adm('member-levels', { method: 'POST', body: { name: { 'zh-CN': 'QA-SF 九五折', 'zh-TW': 'QA-SF 九五折', 'en-US': 'QA-SF 95%' }, slug: 'qa-sf-95', discount_rate: '95', recharge_threshold: '0', spend_threshold: '0', is_default: false, sort_order: 99, is_active: true } })
if (out.level.data?.id) out.setLevel = await adm('users/9/member-level', { method: 'PUT', body: { member_level_id: out.level.data.id } })
for (const [k, v] of Object.entries(out)) console.log(k, v.status_code, v.msg, JSON.stringify(v.data).slice(0, 200))
fs.writeFileSync('.state/sf-fixtures.json', JSON.stringify({ coupon: out.coupon.data?.id, coupon2: out.coupon2.data?.id, promo: out.promo.data?.id, level: out.level.data?.id }))
