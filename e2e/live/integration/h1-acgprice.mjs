import { acgAdmin, admin, updateConn, load, note } from './lib.mjs'
const st = load(); const s = await acgAdmin()
const it = ((await s.post('/admin/api/commodity/data', { page: 1, limit: 5, 'equal-name': 'QA ACG 自动卡' })).data.list)[0]
const keys = ['id', 'category_id', 'name', 'description', 'price', 'user_price', 'factory_price', 'status', 'api_status', 'delivery_way', 'delivery_auto_mode', 'contact_type', 'password_status', 'hide', 'sort', 'coupon', 'config']
const body = Object.fromEntries(keys.map((k) => [k, it[k] ?? '']))
body.price = '9'; body.user_price = '7.5'
note('I-005', 'ACG save price 8->9 user_price 7->7.5: ' + JSON.stringify(await s.post('/admin/api/commodity/save', body)).slice(0, 100))
note('I-005', 'ACG add 5 cards: ' + JSON.stringify(await s.post('/admin/api/card/save', { commodity_id: it.id, card_type: 0, race_get_mode: 0, unique: 1, secret: Array.from({ length: 5 }, (_, i) => `QA-ACG-X${i + 1}`).join('\n') })).slice(0, 100))
const a = await admin('store'); const c = await updateConn(a, 3, '', { auto_sync_price: true }); note('I-005', 'store conn 3 auto_sync_price=' + c.auto_sync_price)
