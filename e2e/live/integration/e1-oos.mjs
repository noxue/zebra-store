// node e1-oos.mjs <product_id> <empty|restore>
import { admin, note, load, save } from './lib.mjs'
const [pid, mode] = process.argv.slice(2); const s = await admin('store'); const st = load()
if (mode === 'empty') {
  const [rows] = await s.page(`/admin/card-secrets?product_id=${pid}&status=available&page=1&page_size=200`)
  const ids = rows.map((r) => r.id)
  const r = await s.raw('PATCH', '/admin/card-secrets/batch-status', { ids, status: 'used' })
  note('I-074', `S product ${pid}: marked ${ids.length} available cards as used -> ${JSON.stringify(r.json).slice(0, 150)}`)
  st.oos = { ...(st.oos || {}), [pid]: ids }; save(st)
} else {
  const ids = st.oos[pid]
  const r = await s.raw('PATCH', '/admin/card-secrets/batch-status', { ids, status: 'available' })
  note('I-074', `S product ${pid}: restored ${ids.length} cards -> ${JSON.stringify(r.json).slice(0, 150)}`)
}
