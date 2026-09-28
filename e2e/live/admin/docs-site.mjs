import { browser, log, shot, watch } from './lib.mjs'
const b = await browser(); const ctx = await b.newContext({ viewport: { width: 1440, height: 900 } }); const page = await ctx.newPage()
const sink = watch(page, []); const bad = []
page.on('response', (r) => { if (r.status() >= 400) bad.push(r.status() + ' ' + r.url()) })
let t0 = Date.now(); await page.goto('https://docs.dot2.com/'); await page.waitForLoadState('networkidle'); log('home', Date.now() - t0, 'ms', await page.title())
await shot(page, 'o012-docs-home')
const links = await page.locator('a[href^="/"]').evaluateAll((as) => [...new Set(as.map((a) => a.getAttribute('href')))])
log('links', links.length, links.slice(0, 40).join(' '))
// search
const sb = page.locator('button.DocSearch, #local-search button, .VPNavBarSearch button').first()
if (await sb.count()) { await sb.click(); await page.keyboard.type('备份'); await page.waitForTimeout(1500); log('search results', (await page.locator('.VPLocalSearchBox, .DocSearch-Modal').innerText().catch(() => '')).slice(0, 400).replace(/\n/g, ' | ')); await shot(page, 'o012-docs-search', false); await page.keyboard.press('Escape') } else log('no search button')
// crawl sidebar pages
const pages = links.filter((l) => !l.includes('#')).slice(0, 80)
const imgs = []
for (const l of pages) {
  await page.goto('https://docs.dot2.com' + l); await page.waitForLoadState('networkidle').catch(() => {})
  const broken = await page.locator('main img, .vp-doc img').evaluateAll((is) => is.filter((i) => !i.complete || i.naturalWidth === 0).map((i) => i.getAttribute('src')))
  if (broken.length) imgs.push(l + ' ' + broken.join(','))
}
log('pages', pages.length, 'broken imgs', JSON.stringify(imgs).slice(0, 1500))
log('4xx', JSON.stringify([...new Set(bad)]).slice(0, 1500)); log('problems', JSON.stringify(sink).slice(0, 800))
// check backup page content
for (const l of pages.filter((p) => /backup|ops|deploy|运维|备份|database/i.test(p))) { await page.goto('https://docs.dot2.com' + l); log('PAGE', l, (await page.locator('main').innerText()).slice(0, 1500).replace(/\n/g, ' | ')) }
await b.close()
