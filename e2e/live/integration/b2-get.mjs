import { zs, zsDecrypt, load, note } from './lib.mjs'
const st = load(); const { key, secret } = st.qaCred
for (const no of [st.o1, st.m1]) {
  const r = await zs('https://store.dot2.com', key, secret, 'GET', `/orders/${no}`)
  const d = r.json.data
  note('I-070', `GET ${no}: status=${d.status} items=${JSON.stringify(d.items.map((i) => ({ s: i.status, enc: i.delivery?.encrypted, alg: i.delivery?.alg, ct: (i.delivery?.ciphertext || '').slice(0, 16) })))}`)
  for (const i of d.items) if (i.delivery) note('I-070', 'decrypted: ' + JSON.stringify(zsDecrypt(i.delivery, secret)).slice(0, 160))
}
