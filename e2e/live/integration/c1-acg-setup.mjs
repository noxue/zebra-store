import { acgAdmin, acgMember, acgUser, load, save, note } from './lib.mjs'
const st = load(); const s = await acgAdmin()
const find = async (kind, name) => ((await s.post(`/admin/api/${kind}/data`, { page: 1, limit: 50, 'equal-name': name })).data?.list || [])[0]
let cat = await find('category', 'QA 对接')
if (!cat) { console.log(await s.post('/admin/api/category/save', { name: 'QA 对接', status: 1, hide: 0, sort: 9 })); cat = await find('category', 'QA 对接') }
const base = { category_id: cat.id, description: '<p>QA</p>', factory_price: '0', status: 1, api_status: 1, contact_type: 0, password_status: 0, hide: 0, sort: 0, coupon: 0, config: '', delivery_auto_mode: 0 }
async function ensure(name, extra, cards) {
  let it = await find('commodity', name)
  if (!it) { const r = await s.post('/admin/api/commodity/save', { ...base, name, ...extra }); console.log('save', name, JSON.stringify(r).slice(0, 200)); it = await find('commodity', name) }
  if (cards && Number(it.card ?? it.stock ?? 0) === 0) console.log('cards', JSON.stringify(await s.post('/admin/api/card/save', { commodity_id: it.id, card_type: 0, race_get_mode: 0, unique: 1, secret: cards.join('\n') })).slice(0, 200))
  it = await find('commodity', name); console.log(name, JSON.stringify(it).slice(0, 400)); return { id: it.id, code: it.code }
}
st.acgAuto = await ensure('QA ACG 自动卡', { price: '8', user_price: '7', delivery_way: 0 }, Array.from({ length: 20 }, (_, i) => `QA-ACG-${String(i + 1).padStart(4, '0')}`))
st.acgManual = await ensure('QA ACG 人工发货', { price: '6', user_price: '6', delivery_way: 1, stock: 50, delivery_message: 'QA 人工发货提示：客服会在 24 小时内处理' })
st.acgRegex = await ensure('QA ACG 正则控件', { price: '4', user_price: '4', delivery_way: 0, widget: JSON.stringify([{ cn: '游戏账号', name: 'account', placeholder: '5-10位数字', type: 'text', regex: '^[0-9]{5,10}$', error: '账号必须是5-10位数字', dict: '' }]) }, Array.from({ length: 10 }, (_, i) => `QA-ACGRX-${String(i + 1).padStart(4, '0')}`))
await acgMember('qaintsup')
let row = await acgUser(s, 'qaintsup')
if (Number(row.balance) < 100) await s.post('/admin/api/user/recharge', { id: row.id, action: 1, amount: (100 - Number(row.balance)).toFixed(2), log: 'qa', total: 0 })
row = await acgUser(s, 'qaintsup'); console.log('member', row.id, row.balance, row.app_key)
st.acgSup = { app_id: String(row.id), app_key: row.app_key }
save(st)
