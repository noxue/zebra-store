import crypto from 'node:crypto'
import { load } from './lib.mjs'
const st = load(); const { app_id, app_key } = st.compat
const sg = crypto.createHash('md5').update(`app_id=${app_id}&key=${app_key}`).digest('hex')
for (const path of ['/shared/authentication/connect', '/api/v1/public/config']) {
  const ts = []
  for (let i = 0; i < 5; i++) { const t0 = Date.now(); const r = await fetch('https://store.dot2.com' + path, path.startsWith('/shared') ? { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: `app_id=${app_id}&sign=${sg}` } : {}); await r.text(); ts.push(Date.now() - t0) }
  console.log(path, 'sequential ms', ts.join(','))
  const t0 = Date.now(); await Promise.all(Array.from({ length: 40 }, () => fetch('https://store.dot2.com' + path, path.startsWith('/shared') ? { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: `app_id=${app_id}&sign=${sg}` } : {}).then((r) => r.text())))
  console.log(path, '40 parallel ms', Date.now() - t0)
}
