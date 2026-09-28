import { launch, open } from '/Volumes/KINGSTON/codes/rust/zebra-store/e2e/live/sf-lib.mjs'
const b = await launch(); const { page } = await open(b)
await page.goto('https://store.dot2.com/products'); await page.waitForTimeout(3000)
const h = await page.evaluate(() => { const el=[...document.querySelectorAll('*')].find(e=>e.childElementCount===0 && e.textContent.trim()==='AWS 账号'); const c=el.closest('article,[class*=zs-card],a,[role=link]'); return c.tagName+' '+c.getAttribute('class')+' role='+c.getAttribute('role')+' href='+c.getAttribute('href')+' tabindex='+c.getAttribute('tabindex')+'\n'+[...c.querySelectorAll('a,button')].map(x=>x.tagName+' '+x.getAttribute('href')+' '+x.getAttribute('aria-label')).join('\n') })
console.log(h)
console.log(await page.evaluate(()=>[...document.querySelectorAll('input')].map(i=>i.placeholder+' vis='+(i.offsetParent!==null))))
await b.close()
