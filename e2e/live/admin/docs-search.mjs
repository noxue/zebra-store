import { browser, log, shot } from './lib.mjs'
const b = await browser(); const page = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
await page.goto('https://docs.dot2.com/deploy/'); await page.waitForLoadState('networkidle')
const side = await page.locator('.VPSidebar a').evaluateAll((as) => as.map((a) => a.getAttribute('href')))
log('sidebar', side.length, side.join(' '))
for (const q of ['数据库', 'backup', '备份']) {
  await page.keyboard.press('Control+k'); await page.waitForTimeout(500)
  await page.keyboard.type(q, { delay: 50 }); await page.waitForTimeout(2500)
  log('search', q, (await page.locator('.VPLocalSearchBox').innerText().catch(() => 'none')).slice(0, 300).replace(/\n/g, ' | '))
  await shot(page, 'o012-docs-search-' + q.replace(/[^a-z]/g, 'x'), false)
  await page.keyboard.press('Escape')
}
const bk = side.find((s) => /backup/.test(s || ''))
if (bk) { await page.goto('https://docs.dot2.com' + bk.replace(/\.html$/, '')); log('BACKUP', (await page.locator('main').innerText()).slice(0, 2500).replace(/\n/g, ' | ')) }
const broken = []
for (const l of side.filter(Boolean)) { await page.goto(new URL(l, 'https://docs.dot2.com/deploy/').href); await page.waitForLoadState('networkidle').catch(() => {}); const bi = await page.locator('.vp-doc img').evaluateAll((is) => is.filter((i) => i.naturalWidth === 0).map((i) => i.getAttribute('src'))); if (bi.length) broken.push(l + ':' + bi) ; if (/404|not found/i.test(await page.title())) broken.push('404 ' + l) }
log('sidebar pages broken', JSON.stringify(broken))
await b.close()
