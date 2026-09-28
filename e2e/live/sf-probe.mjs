// Dumps interactive elements of a page (selector discovery).
import { launch, open, go, shot } from './sf-lib.mjs'
const [url, name = 'probe', mobile = ''] = process.argv.slice(2)
const b = await launch()
const { page, diag } = await open(b, { mobile: mobile === 'm' })
await page.addInitScript(() => localStorage.setItem('announcement_dismiss', JSON.stringify({ mode: 'forever', version: '*' })))
const ms = await go(page, url, diag)
await page.waitForTimeout(1000)
const els = await page.evaluate(() => [...document.querySelectorAll('input,button,select,textarea,a[href]')].filter(e=>e.offsetParent!==null).map((e) => `${e.tagName} ${e.getAttribute('type')||''} name=${e.getAttribute('name')||''} ph=${e.getAttribute('placeholder')||''} href=${e.getAttribute('href')||''} txt=${(e.innerText||'').trim().slice(0,40).replace(/\n/g,' ')}`))
console.log(ms + 'ms\n' + els.join('\n'))
console.log(JSON.stringify(diag))
await shot(page, name, true)
await b.close()
