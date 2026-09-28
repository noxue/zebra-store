// Removes / deactivates QA storefront fixtures created by sf-setup-*.mjs.
import fs from 'node:fs'
import { adm } from './sf-lib.mjs'
const fx = JSON.parse(fs.readFileSync('.state/sf-fixtures.json', 'utf8'))
const log = (k, r) => console.log(k, r.status_code, r.msg)
for (const id of [fx.coupon, fx.coupon2]) { const r = await adm(`coupons/${id}`, { method: 'DELETE' }); log('coupon del ' + id, r) }
log('promo del', await adm(`promotions/${fx.promo}`, { method: 'DELETE' }))
log('wholesale clear', await adm('products/7/wholesale-prices', { method: 'PATCH', body: { wholesale_prices: [] } }))
log('unassign level', await adm('users/9/member-level', { method: 'PUT', body: { member_level_id: 0 } }))
log('level del', await adm(`member-levels/${fx.level}`, { method: 'DELETE' }))
for (const id of fx.posts || []) log('post del ' + id, await adm(`posts/${id}`, { method: 'DELETE' }))
log('banner del', await adm(`banners/${fx.banner}`, { method: 'DELETE' }))
for (const id of [fx.manual, fx.auto]) log('product off ' + id, await adm(`products/${id}`, { method: 'PATCH', body: { is_active: false } }))
