// node acgcards.mjs <commodityName> <lock|unlock>
import { acgAdmin, load, save, note } from './lib.mjs'
const [name, mode] = process.argv.slice(2); const s = await acgAdmin(); const st = load()
const it = ((await s.post('/admin/api/commodity/data', { page: 1, limit: 5, 'equal-name': name })).data.list)[0]
if (mode === 'lock') {
  const d = await s.post('/admin/api/card/data', { page: 1, limit: 200, 'equal-commodity_id': it.id, 'equal-status': 0 })
  const ids = (d.data.list || []).map((c) => c.id); st.acgLocked = ids; save(st)
  note('I-007', `ACG lock ${ids.length} cards of ${name}: ` + JSON.stringify(await s.post('/admin/api/card/lock', { 'list[]': ids })).slice(0, 100))
} else note('I-007', 'ACG unlock: ' + JSON.stringify(await s.post('/admin/api/card/unlock', { 'list[]': st.acgLocked })).slice(0, 100))
