import fs from 'node:fs'
import path from 'node:path'
import { TS } from './common.mjs'
const LOGO = path.resolve('fixtures/logo.png')
export default async ({ page, api, shot, log, admin, base }) => {
  await page.goto(`${admin}/media`)
  await page.waitForLoadState('networkidle')
  const before = JSON.stringify((await api("GET", "/admin/media?page=1&page_size=1")).data?.total ?? "")
  const buf = fs.readFileSync(LOGO)
  // already uploaded
  await page.waitForTimeout(3000)
  const list = (await api('GET', `/admin/media?page=1&page_size=50&search=qa-media-${TS}`)).data?.items ?? []
  log('uploaded media', list.length, JSON.stringify(list.map((m) => [m.id, m.name, m.path ?? m.url])).slice(0, 400))
  await shot(page, 'c-media-grid')
  // rename first via UI
  const card = page.getByTitle('Click to edit name').filter({ hasText: `qa-media-${TS}-1` }).first()
  if (await card.count()) {
    await card.click()
    const inp = page.locator('input:focus')
    await inp.fill(`qa-media-${TS}-renamed`)
    await inp.press('Enter')
    await page.waitForTimeout(1200)
    log('rename msg', (await page.locator('[role=status]').allInnerTexts()).join('|'))
  }
  // malicious uploads via API
  const up = async (name, type, body, scene = 'common') => page.evaluate(async ({ name, type, body, scene }) => {
    const fd = new FormData()
    fd.append('file', new Blob([Uint8Array.from(atob(body), (c) => c.charCodeAt(0))], { type }), name)
    fd.append('scene', scene)
    const r = await fetch('/api/v1/admin/upload', { method: 'POST', headers: { authorization: `Bearer ${localStorage.getItem('admin_token')}` }, body: fd })
    return `${r.status} ${(await r.text()).slice(0, 200)}`
  }, { name, type, body, scene })
  const b64 = (s) => Buffer.from(s).toString('base64')
  log('php', await up('evil.php', 'application/x-php', b64('<?php system($_GET[1]); ?>')))
  log('php disguised png ext', await up('evil.png', 'image/png', b64('<?php system($_GET[1]); ?>')))
  log('png content .php name', await up('real.php', 'image/png', buf.toString('base64')))
  log('traversal filename', await up('../../../etc/qa.png', 'image/png', buf.toString('base64')))
  log('traversal scene', await up('qa.png', 'image/png', buf.toString('base64'), '../../x'))
  log('svg with script', await up('x.svg', 'image/svg+xml', b64('<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>')))
  log('html', await up('x.html', 'text/html', b64('<script>alert(1)</script>')))
  // rename with traversal / huge name
  const m0 = (await api('GET', `/admin/media?page=1&page_size=50&search=qa-media-${TS}`)).data?.items ?? []
  if (m0[0]) {
    const r = await api('PUT', `/admin/media/${m0[0].id}`, { name: '../../../etc/passwd' })
    log('rename traversal', r.status_code, r.msg, JSON.stringify(r.data).slice(0, 200))
    const r2 = await api('PUT', `/admin/media/${m0[0].id}`, { name: '' })
    log('rename empty', r2.status_code, r2.msg)
  }
  // cleanup traversal uploads that succeeded: list recent
  const recent = (await api('GET', '/admin/media?page=1&page_size=15')).data?.items ?? []
  log('recent', JSON.stringify(recent.map((m) => [m.id, m.name, m.path ?? m.url, m.scene])).slice(0, 1200))
  fs.writeFileSync('live/admin/out/c-media-recent.json', JSON.stringify(recent))
  log('total before', before, 'after', JSON.stringify((await api("GET", "/admin/media?page=1&page_size=1")).data?.total ?? ""))
}
