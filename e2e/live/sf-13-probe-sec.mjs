import { launch, open, go, shot, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
for (const p of ['/me/security', '/me/profile', '/me']) {
  await go(page, S + p, diag); await sleep(1500)
  console.log('==', p, (await page.locator('main').innerText()).replace(/\s+/g, ' ').slice(0, 1200))
  console.log(await page.evaluate(() => [...document.querySelectorAll('main input, main button, main select')].filter(e=>e.offsetParent).map((e) => `${e.tagName}:${e.type}:${e.placeholder||''}:${(e.innerText||e.value||'').trim().slice(0,20)}`).join(' | ')))
  await shot(page, 'probe' + p.replace(/\//g, '_'), true)
}
console.log(JSON.stringify(diag))
await b.close()
