// F-001..F-004, F-006, F-009, F-010, F-012..F-016 browsing flows on the main store.
import { launch, open as open0, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const open = async (b, o) => { const r = await open0(b, o); await noAnnouncement(r.ctx); return r }
const b = await launch()
const dismiss = () => localStorage.setItem('announcement_dismiss', JSON.stringify({ mode: 'forever', version: '*' }))
const bodyText = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')

// ---- F-001 home after dismissing announcement
if (!process.env.SKIP12) {
  const { ctx, page, diag } = await open(b)
  await go(page, S + '/', diag)
  await sleep(1500)
  const t = await bodyText(page)
  const banner = await page.locator('[class*=banner], [class*=carousel], [class*=swiper]').count()
  const hasPosts = /最新动态|最新文章/.test(t)
  const s = await shot(page, 'F001-home-S-dismissed-full', true)
  const imgs = await page.evaluate(() => [...document.images].filter((i) => i.complete && i.naturalWidth === 0).map((i) => i.src))
  record({ flow: 'F-001', banner, hasPosts, brokenImgs: imgs, snippet: t.slice(0, 200), tail: t.slice(-500), diag, shots: [s] })
  await ctx.close()
}

// ---- F-002 announcement dismissal strategies
if (!process.env.SKIP12) {
  const res = {}
  for (const [label, sel] of [['session', '关闭'], ['today', '今日不再提示'], ['forever', '不再提示']]) {
    const { ctx, page, diag } = await open(b)
    await go(page, S + '/', diag)
    await sleep(800)
    const dialog = page.getByRole('dialog')
    const visible1 = await dialog.isVisible().catch(() => false)
    await dialog.getByText(sel, { exact: true }).first().click()
    await sleep(500)
    const ls = await page.evaluate(() => ({ l: localStorage.getItem('announcement_dismiss'), s: sessionStorage.getItem('announcement_closed') }))
    await page.reload(); await sleep(1500)
    const visibleAfterReload = await dialog.isVisible().catch(() => false)
    // new tab same context (session storage not shared)
    const p2 = await ctx.newPage(); await p2.goto(S + '/'); await sleep(2000)
    const visibleNewTab = await p2.getByRole('dialog').isVisible().catch(() => false)
    res[label] = { visible1, ls, visibleAfterReload, visibleNewTab }
    await ctx.close()
  }
  record({ flow: 'F-002', res })
}

// ---- F-003 categories + F-004 search (desktop and mobile)
if (!process.env.SKIP36) for (const mobile of [false, true]) {
  const { ctx, page, diag } = await open(b, { mobile })
  await go(page, S + '/products', diag)
  await sleep(1000)
  const m = mobile ? 'm' : 'd'
  const shots = []
  if (mobile) {
    shots.push(await shot(page, `F003-products-${m}`))
    // find a category trigger on mobile
    const btn = page.getByRole('button', { name: /分类|筛选|Categories/ }).first()
    const hasBtn = await btn.isVisible().catch(() => false)
    if (hasBtn) { await btn.click(); await sleep(700); shots.push(await shot(page, `F003-mobile-drawer-${m}`)) }
    record({ flow: 'F-003', mobile, drawerButton: hasBtn })
  }
  const cat = page.getByRole('button', { name: /AI 账号/ }).first()
  await cat.click(); await sleep(1500)
  const url = page.url()
  const cards = await page.evaluate(() => [...document.querySelectorAll('article')].filter((a) => a.offsetParent !== null).map((a) => (a.querySelector('h3,h2,h4')?.textContent || a.textContent).trim().slice(0, 30)))
  shots.push(await shot(page, `F003-category-ai-${m}`))
  record({ flow: 'F-003', mobile, url, cards })
  // search
  await go(page, S + '/products', diag); await sleep(800)
  const input = page.locator('input[placeholder*="搜索"]:visible').first()
  if (mobile && !(await input.isVisible())) { const b2 = page.getByRole('button', { name: /分类|筛选|搜索/ }).first(); if (await b2.isVisible().catch(() => false)) await b2.click() }
  await input.fill('Claude')
  const t0 = Date.now(); await sleep(150)
  const early = await page.evaluate(() => [...document.querySelectorAll('article')].filter((a) => a.offsetParent !== null).map((a) => (a.querySelector('h3,h2,h4')?.textContent || a.textContent).trim().slice(0, 30)))
  await sleep(1500)
  const after = await page.evaluate(() => [...document.querySelectorAll('article')].filter((a) => a.offsetParent !== null).map((a) => (a.querySelector('h3,h2,h4')?.textContent || a.textContent).trim().slice(0, 30)))
  shots.push(await shot(page, `F004-search-claude-${m}`))
  await input.fill('zzzqqq-none'); await sleep(1500)
  const noResultText = (await bodyText(page)).match(/.{0,30}(暂无|没有|清除).{0,30}/g)
  shots.push(await shot(page, `F004-search-none-${m}`))
  const clear = page.getByRole('button', { name: /清除筛选|清除/ }).first()
  const hasClear = await clear.isVisible().catch(() => false)
  if (hasClear) { await clear.click(); await sleep(1200) }
  const afterClear = await page.evaluate(() => [...document.querySelectorAll('article')].filter((a) => a.offsetParent !== null).length)
  record({ flow: 'F-004', mobile, early: early.length, after, noResultText, hasClear, afterClear, url: page.url(), diag, shots })
  await ctx.close()
}

// ---- F-006 detail pages (desktop + mobile), F-015 sticky bar
if (!process.env.SKIP36) for (const mobile of [false, true]) {
  for (const slug of ['lab-e2e-card', 'aws-account', 'chatgpt-team']) {
    const { ctx, page, diag } = await open(b, { mobile })
    const ms = await go(page, `${S}/products/${slug}`, diag)
    await sleep(1000)
    const t = await bodyText(page)
    const m = mobile ? 'm' : 'd'
    const s1 = await shot(page, `F006-detail-${slug}-${m}`)
    const s2 = await shot(page, `F006-detail-${slug}-${m}-full`, true)
    let sticky
    if (mobile) { await page.mouse.wheel(0, 900); await sleep(800); sticky = await shot(page, `F015-detail-scrolled-${slug}-${m}`) }
    record({ flow: 'F-006', mobile, slug, ms, text: t.slice(0, 900), diag, shots: [s1, s2, sticky].filter(Boolean) })
    await ctx.close()
  }
}

// ---- F-009 language switch
if (!process.env.SKIP9) {
  const { ctx, page, diag } = await open(b)
  const langs = []
  page.on('request', (r) => { if (r.url().includes('/api/v1/')) langs.push(r.headers()['x-lang']) })
  await go(page, `${S}/products/aws-account`, diag); await sleep(800)
  const out = {}
  for (const [label, code] of [['繁體', 'zh-TW'], ['English', 'en-US'], ['简体', 'zh-CN']]) {
    await page.locator('header button:has-text("简体"), header button:has-text("繁體"), header button:has-text("English"), header button:has-text("EN")').first().click()
    await sleep(400)
    await page.getByText(label, { exact: false }).last().click()
    await sleep(1500)
    const t = await bodyText(page)
    const ls = await page.evaluate(() => localStorage.getItem('locale'))
    out[code] = { ls, text: t.slice(0, 700), shot: await shot(page, `F009-lang-${code}`) }
    if (code === 'en-US') {
      await page.reload(); await sleep(1500)
      out.enAfterReload = (await bodyText(page)).slice(0, 200)
      for (const path of ['/', '/products', '/cart', '/guest/orders', '/auth/login', '/auth/register', '/blog', '/notice', '/about']) {
        await go(page, S + path, diag); await sleep(800)
        const tx = await bodyText(page)
        const cjk = tx.match(/[一-鿿][一-鿿，。：、！？（）\w ]{0,20}/g) || []
        out['en' + path] = { cjk: [...new Set(cjk)].slice(0, 25), shot: await shot(page, `F009-en${path.replace(/\//g, '_') || '_home'}`, true) }
      }
      await go(page, `${S}/products/aws-account`, diag); await sleep(800)
    }
  }
  record({ flow: 'F-009', out, xlangSeen: [...new Set(langs)], diag })
  await ctx.close()
}

// ---- F-010 theme
{
  const { ctx, page, diag } = await open(b)
  await go(page, S + '/', diag); await sleep(800)
  const before = await page.evaluate(() => document.documentElement.className)
  await page.locator('header button[aria-label*="主题"], header button[title*="主题"], header button[aria-label*="theme" i]').first().click().catch(async () => {
    // fallback: the moon icon button (first icon-only button in header)
    const btns = page.locator('header button'); const n = await btns.count()
    for (let i = 0; i < n; i++) { if (!(await btns.nth(i).innerText()).trim()) { await btns.nth(i).click(); break } }
  })
  await sleep(800)
  const after = await page.evaluate(() => ({ cls: document.documentElement.className, ls: localStorage.getItem('dujiao_theme') }))
  const s1 = await shot(page, 'F010-dark-home', true)
  await page.reload(); await sleep(1500)
  const afterReload = await page.evaluate(() => document.documentElement.className)
  await go(page, `${S}/products/aws-account`, diag); await sleep(800)
  const s2 = await shot(page, 'F010-dark-detail', true)
  await go(page, `${S}/auth/login`, diag); await sleep(800)
  const s3 = await shot(page, 'F010-dark-login')
  record({ flow: 'F-010', before, after, afterReload, diag, shots: [s1, s2, s3] })
  await ctx.close()
  // reduced motion
  const c2 = await b.newContext({ reducedMotion: 'reduce', viewport: { width: 1440, height: 900 } })
  const p2 = await c2.newPage(); await p2.goto(S + '/'); await sleep(2000)
  const petals = await p2.evaluate(() => document.querySelectorAll('[class*=sakura], [class*=petal], canvas').length)
  const c3 = await b.newContext({ viewport: { width: 1440, height: 900 } })
  const p3 = await c3.newPage(); await p3.goto(S + '/'); await sleep(2000)
  const petalsNormal = await p3.evaluate(() => document.querySelectorAll('[class*=sakura], [class*=petal], canvas').length)
  record({ flow: 'F-010', reducedMotionPetals: petals, normalPetals: petalsNormal })
  await c2.close(); await c3.close()
}

// ---- F-012 content pages, F-013 404, F-014 seo files
{
  const { ctx, page, diag } = await open(b)
  const out = {}
  for (const path of ['/blog', '/notice', '/about', '/terms', '/privacy', '/not-exist']) {
    const ms = await go(page, S + path, diag); await sleep(800)
    out[path] = { ms, text: (await bodyText(page)).slice(0, 500), shot: await shot(page, `F012${path.replace(/\//g, '_')}`, true) }
  }
  // first blog post
  await go(page, S + '/blog', diag); await sleep(800)
  const links = await page.locator('a[href^="/blog/"]').evaluateAll((as) => as.map((a) => a.getAttribute('href')))
  if (links[0]) { await go(page, S + links[0], diag); await sleep(800); out.post = { href: links[0], text: (await bodyText(page)).slice(0, 700), shot: await shot(page, 'F012-blog-post', true) } }
  await go(page, S + '/notice', diag); await sleep(800)
  const nlinks = await page.locator('a[href^="/blog/"], a[href^="/notice/"]').evaluateAll((as) => as.map((a) => a.getAttribute('href')))
  out.noticeLinks = nlinks
  record({ flow: 'F-012', out, diag })
  for (const f of ['/sitemap.xml', '/robots.txt']) {
    const r = await fetch(S + f); const t = await r.text()
    record({ flow: 'F-014', f, status: r.status, ct: r.headers.get('content-type'), body: t.slice(0, 2500) })
  }
  await ctx.close()
}

// ---- F-016 quick buy on product card
for (const mobile of [false, true]) {
  const { ctx, page, diag } = await open(b, { mobile })
  await go(page, S + '/products', diag); await sleep(1000)
  // cart icon button inside first card (aws-account multi-sku)
  await page.locator('article', { hasText: 'AWS 账号' }).getByRole('button', { name: '快速购买' }).click()
  await sleep(1000)
  const m = mobile ? 'm' : 'd'
  const s1 = await shot(page, `F016-quickbuy-${m}`)
  const dlg = (await page.getByRole('dialog').innerText().catch(() => '')).replace(/\s+/g, ' ')
  record({ flow: 'F-016', mobile, dlg: dlg.slice(0, 600), diag, shots: [s1] })
  await ctx.close()
}
await b.close()
