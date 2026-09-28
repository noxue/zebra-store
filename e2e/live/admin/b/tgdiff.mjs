import fs from 'node:fs'
export default async ({ api, log }) => {
  const orig = JSON.parse(fs.readFileSync('live/admin/out/b-backup-telegram-bot.json', 'utf8'))
  const now = (await api('GET', '/admin/settings/telegram-bot')).data
  const walk = (a, b, p = '') => { if (JSON.stringify(a) === JSON.stringify(b)) return; if (a && b && typeof a === 'object' && typeof b === 'object') { for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) walk(a[k], b[k], p + '.' + k) } else log('diff', p, JSON.stringify(a)?.slice(0, 150), '=>', JSON.stringify(b)?.slice(0, 150)) }
  walk(orig, now)
}
