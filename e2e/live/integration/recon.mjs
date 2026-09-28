import { admin, log } from './lib.mjs'
const s = await admin('store'), z = await admin('zs2')
for (const [n, a] of [['store', s], ['zs2', z]]) {
  const [conns] = await a.page('/admin/site-connections?page=1&page_size=50')
  for (const c of conns) console.log(n, JSON.stringify(c).slice(0, 900))
  const [maps] = await a.page('/admin/product-mappings?page=1&page_size=50')
  for (const m of maps) console.log(n, 'MAP', JSON.stringify(m).slice(0, 500))
  console.log(n, 'procstats', JSON.stringify(await a.get('/admin/procurement-orders/stats')))
}
const [profiles] = await s.page('/admin/resellers/profiles?page=1&page_size=50')
for (const p of profiles) console.log('RSL', JSON.stringify(p).slice(0, 600))
