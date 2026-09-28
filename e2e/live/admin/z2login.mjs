import { browser, log, shot, watch } from './lib.mjs'
const b = await browser(); const page = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
const s = watch(page, [])
await page.goto('https://zs2.dot2.com/admin/login'); await page.waitForLoadState('networkidle')
await page.locator('input').first().fill('admin'); await page.locator('input[type=password]').fill('ecsT9w2sBJbBksAmZs7k')
await page.locator('button[type=submit]').click(); await page.waitForTimeout(4000)
log(page.url(), JSON.stringify(s)); log((await page.locator('body').innerText()).slice(0, 500).replace(/\n/g, ' | '))
await shot(page, 'z2-login', false); await b.close()
