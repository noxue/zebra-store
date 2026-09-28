export default async ({ api, log }) => {
  const m = await api('GET', '/admin/media?page=1&page_size=50&keyword=qa-')
  const arr = Array.isArray(m.data) ? m.data : (m.data?.items || m.data?.list || [])
  log('keys', Object.keys(m.data || {}), arr.length)
  const mine = arr.filter(x => /^qa-(logo|favicon)/.test(x.name || x.original_name || x.filename || ''))
  log('qa media', JSON.stringify(mine.map(x => [x.id, x.name])))
  for (const x of mine) log('del media', x.id, (await api('DELETE', '/admin/media/' + x.id)).status_code)
}
