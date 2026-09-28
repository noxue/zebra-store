import fs from 'node:fs'
export default async ({ api, log }) => {
  const bk = JSON.parse(fs.readFileSync('live/admin/out/b-backup-1790358812585.json', 'utf8'))
  // order_config backup was {} (defaults); put back {}
  for (const [k, v] of Object.entries(bk)) {
    const url = k.startsWith('settings:') ? '/admin/settings?key=' + k.slice(9) : k
    if (url.includes('logs') || url.includes('runtime')) continue
    const now = await api('GET', url)
    const same = JSON.stringify(now.data) === JSON.stringify(v.data)
    if (!same) log('DIFF', k, JSON.stringify(v.data).slice(0, 160), '=>', JSON.stringify(now.data).slice(0, 160))
  }
  const m = await api('GET', '/admin/media?page=1&page_size=50&keyword=qa-')
  const mine = (m.data || []).filter(x => /^qa-(logo|favicon)/.test(x.name || x.original_name || ''))
  log('qa media', JSON.stringify(mine.map(x => [x.id, x.name, x.path || x.url])))
  for (const x of mine) log('del media', x.id, (await api('DELETE', '/admin/media/' + x.id)).status_code)
}
