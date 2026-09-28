// Measures load timings and lists slowest requests.
import { launch, open } from './sf-lib.mjs'
const [url] = process.argv.slice(2)
const b = await launch()
const { page } = await open(b)
const reqs = []
page.on('requestfinished', async (r) => { const t = r.timing(); reqs.push([Math.round(t.responseEnd), r.url().slice(0, 110)]) })
const t0 = Date.now()
await page.goto(url, { waitUntil: 'load' }); const load = Date.now() - t0
await page.waitForLoadState('networkidle'); const idle = Date.now() - t0
const nav = await page.evaluate(() => JSON.stringify(performance.getEntriesByType('navigation')[0].toJSON(), ['domContentLoadedEventEnd', 'loadEventEnd', 'responseStart']))
const fcp = await page.evaluate(() => performance.getEntriesByName('first-contentful-paint')[0]?.startTime)
console.log({ load, idle, nav, fcp })
reqs.sort((a, b) => b[0] - a[0]); console.log(reqs.slice(0, 12))
await b.close()
