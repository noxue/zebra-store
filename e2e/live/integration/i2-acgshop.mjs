import { acgAdmin, load, save, note } from './lib.mjs'
const st = load(); const s = await acgAdmin(); const c = st.compat
const list = async () => (await s.post('/admin/api/store/data', { page: 1, limit: 50 })).data.list
for (const [type, domain, tag] of [[1, 'https://store.dot2.com:443', 'I-030'], [0, 'http://store.dot2.com', 'I-021']]) {
  const r = await s.post('/admin/api/store/save', { type, domain, app_id: c.app_id, app_key: c.app_key })
  note(tag, `ACG add shared store type=${type} domain=${domain}: ${JSON.stringify(r).slice(0, 200)}`)
}
const rows = await list(); for (const r of rows) console.log(JSON.stringify(r).slice(0, 300))
st.acgShops = rows.filter((r) => String(r.app_id) === String(c.app_id)).map((r) => ({ id: r.id, type: r.type, domain: r.domain })); save(st)
for (const sh of st.acgShops) { const con = await s.post('/admin/api/store/connect', { id: sh.id }); note(sh.type === 1 ? 'I-030' : 'I-021', `connect shop #${sh.id} type ${sh.type}: ${JSON.stringify(con).slice(0, 250)}`) }
