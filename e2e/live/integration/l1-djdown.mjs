import { user, admin, setWallet, walletOf, note, sleep, load, save } from './lib.mjs'
const st = load(); const s = await admin('store'); const u = await user('store', 'qa-cred2@lab.test'); const me = await u.get('/me')
const sec = (await u.post('/api-credential/regenerate')).api_secret; const key = (await u.get('/api-credential')).api_key
await setWallet(s, me.id, 20); st.cred2Uid = me.id; save(st)
const d = await admin('dujiao')

let conn = (await d.page('/admin/site-connections?page=1&page_size=50'))[0].find((c) => c.name === 'QA Zebra 主站')
if (!conn) {
  const r = await d.raw('POST', '/admin/site-connections', { name: 'QA Zebra 主站', base_url: 'https://store.dot2.com', api_key: key, api_secret: sec, protocol: 'dujiao-next', callback_url: 'https://dujiao.dot2.com/api/v1/upstream/callback', retry_max: 3, retry_intervals: '[30,60,300]', exchange_rate: 1, price_markup_percent: 10, price_rounding_mode: 'none', auto_sync_price: true })
  note('I-046', 'DJ create connection -> ' + JSON.stringify(r.json).slice(0, 300)); conn = r.json.data
}
const ping = await d.raw('POST', `/admin/site-connections/${conn.id}/ping`, {}); note('I-046', 'DJ ping store -> ' + JSON.stringify(ping.json).slice(0, 250))
const up = await d.raw('GET', `/admin/upstream-products?connection_id=${conn.id}&page=1&page_size=100`)
const items = up.json.data?.items || up.json.data?.list || (Array.isArray(up.json.data) ? up.json.data : []); if (!items.length) console.log(JSON.stringify(up.json).slice(0,400))
note('I-046', `DJ sees ${items.length} upstream products: ${items.map((x) => (x.title?.['zh-CN'] || x.title) + '#' + x.id).join(', ').slice(0, 300)}`)
const imp = await d.raw('POST', '/admin/product-mappings/import', { connection_id: conn.id, upstream_product_id: st.qaB.product_id, auto_create_category: true })
note('I-046', 'DJ import qa-int-b -> ' + JSON.stringify(imp.json).slice(0, 250))
st.djDown = { conn: conn.id, local: imp.json.data?.local_product_id }; save(st)
