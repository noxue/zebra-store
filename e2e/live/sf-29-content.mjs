// F-001 banner/latest posts, F-012 blog/notice pages, F-014 sitemap content, B-093 mobile image.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const body = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
for (const mobile of [false, true]) {
  const m = mobile ? 'm' : 'd'
  const { ctx, page, diag } = await open(b, { mobile }); await noAnnouncement(ctx)
  await go(page, S + '/', diag); await sleep(2000)
  const bannerImgs = await page.evaluate(() => [...document.images].filter((i) => i.offsetParent && i.getBoundingClientRect().width > 250).map((i) => i.currentSrc.split('/').pop()).slice(0, 3))
  const t = await body(page)
  const s1 = await shot(page, `F001-home-banner-${m}`)
  const s2 = await shot(page, `F001-home-full-${m}`, true)
  // click banner
  const ban = page.getByText('QA 横幅').first()
  let bannerNav = ''
  if (await ban.isVisible().catch(() => false)) { await ban.click(); await sleep(2000); bannerNav = page.url() }
  record({ flow: 'F-001', mobile, bannerImgs, hasBanner: /QA 横幅/.test(t), latest: t.match(/最新动态.{0,200}/)?.[0], bannerNav, diag, shots: [s1, s2] })
  if (!mobile) {
    await go(page, S + '/blog', diag); await sleep(1500)
    const bl = await body(page)
    const s3 = await shot(page, 'F012-blog-list-d', true)
    await go(page, S + '/blog/qa-sf-howto', diag); await sleep(1500)
    const bd = await body(page)
    const s4 = await shot(page, 'F012-blog-detail-d', true)
    const rel = page.getByText('Google 账号').first()
    let relNav = ''
    if (await rel.isVisible().catch(() => false)) { await rel.click(); await sleep(2000); relNav = page.url() }
    await go(page, S + '/blog/qa-sf-draft', diag); await sleep(1500)
    const draft = (await body(page)).slice(0, 300)
    const s5 = await shot(page, 'F012-blog-draft-d')
    await go(page, S + '/notice', diag); await sleep(1500)
    const nt = await body(page)
    const s6 = await shot(page, 'F012-notice-list-d', true)
    const nl = page.getByText('QA 维护公告').first()
    let noticeNav = ''
    if (await nl.isVisible().catch(() => false)) { await nl.click(); await sleep(2000); noticeNav = page.url() + ' ' + (await body(page)).slice(0, 300) }
    record({ flow: 'F-012', blogList: bl.match(/博客.{0,300}/)?.[0], draftInList: /QA 草稿/.test(bl), detail: bd.slice(0, 700), relNav, draft, notice: nt.match(/公告 ✦.{0,300}/)?.[0], noticeNav, diag, shots: [s3, s4, s5, s6] })
    const sm = await (await fetch(S + '/sitemap.xml')).text()
    record({ flow: 'F-014', hasPublished: sm.includes('qa-sf-howto'), hasDraft: sm.includes('qa-sf-draft'), hasNotice: sm.includes('qa-sf-notice'), hasOffShelf: sm.includes('qa-sf-lastcard'), productUrls: (sm.match(/\/products\/[^<]+/g) || []).length })
  }
  await ctx.close()
}
await b.close()
