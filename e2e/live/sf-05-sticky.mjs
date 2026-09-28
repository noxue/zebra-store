// F-015 sticky purchase bar stable screenshot (mobile).
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const { ctx, page, diag } = await open(b, { mobile: true }); await noAnnouncement(ctx)
await go(page, S + '/products/aws-account', diag); await sleep(800)
await page.evaluate(() => window.scrollTo(0, 1200)); await sleep(2500)
const s = await shot(page, 'F015-sticky-bar-m')
const overlap = await page.evaluate(() => {
  const buy = [...document.querySelectorAll('button')].filter((x) => /立即购买/.test(x.innerText)).map((x) => x.getBoundingClientRect()).find((r) => r.bottom > window.innerHeight - 250)
  const fab = [...document.querySelectorAll('button')].find((x) => /顶部|top/i.test(x.getAttribute('aria-label') || x.title || ''))?.getBoundingClientRect()
  return { buy, fab, fabLabel: [...document.querySelectorAll('button')].filter((x) => { const r = x.getBoundingClientRect(); return r.right > 330 && r.bottom > 650 }).map((x) => (x.getAttribute('aria-label') || x.innerText).slice(0, 20)) }
})
record({ flow: 'F-015', overlap, shots: [s] })
await b.close()
