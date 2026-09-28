// node watchproc.mjs <site> <procId> <tag> <seconds>
import { admin, note, sleep } from './lib.mjs'
const [site, id, tag, secs] = process.argv.slice(2); const a = await admin(site)
let last = ''; const t0 = Date.now()
while ((Date.now() - t0) / 1000 < Number(secs)) {
  const p = await a.get(`/admin/procurement-orders/${id}`)
  const cur = `${p.status} retry=${p.retry_count} next=${p.next_retry_at || ''} err=${String(p.error_message || '').slice(0, 90).replace(/\n/g, ' ')}`
  if (cur !== last) { note(tag, `${site} proc #${id} @+${((Date.now() - t0) / 1000).toFixed(0)}s: ${cur} | local_order=${p.local_order?.status}`); last = cur }
  await sleep(10000)
}
