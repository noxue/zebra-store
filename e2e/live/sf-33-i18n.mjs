// i18n sweep: logged-in + guest pages in en-US and zh-TW; lists leftover Chinese (en) and simplified-only strings (zh-TW).
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const PAGES = ['/', '/products', '/products/aws-account', '/cart', '/checkout', '/me', '/me/orders', '/me/wallet', '/me/affiliate', '/me/gift-cards', '/me/security', '/me/api', '/me/profile', '/orders/DJ20260926022209006712', '/guest/orders', '/auth/login', '/auth/register', '/blog/qa-sf-howto', '/notice', '/about', '/terms', '/not-exist', '/reseller/apply']
const DATA = /(Google|ChatGPT|Claude|AWS|GCP|QA|联调|独角|ACG|测试|TEST-|斑马小铺|zebra-lab|Lab|自动发货|人工发货|云服务|谷歌|多规格|独享|团队席位|高额度|含试用|邮箱账号|邮箱账号|充值账号|区服|角色ID|player@|新号|月卡|九五折|今晚|维护|购买教程|三步|步骤|选择商品|亚服|美服|凭证号|使用说明：|已为|订单余额支付|礼品卡兑换：|管理员|partial|concurrent|offline|本商品|购买后|卡密为|这是多站点)/
const SIMP = /[订单账号钱包设置购买优惠说明记录验证余额发货页码时间类型状态详情关闭确认删除选择规格数量总计实付应付充值兑换请输入邮箱密码登录注册个人中心会员价格库存售罄]/
const b = await launch()
const out = {}
for (const loc of ['en-US', 'zh-TW']) {
  const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
  await ctx.addInitScript((l) => localStorage.setItem('locale', l), loc)
  // put something in the cart for /checkout
  await ctx.addInitScript(() => { if (!localStorage.getItem('cart_items') || localStorage.getItem('cart_items') === '[]') localStorage.setItem('cart_items', JSON.stringify([{ productId: 7, skuId: 10, skuCode: 'new', slug: 'google-account', title: { 'zh-CN': 'Google 账号', 'zh-TW': 'Google 帳號', 'en-US': 'Google Account' }, priceAmount: '9.90', quantity: 1, minPurchaseQuantity: 1, maxPurchaseQuantity: 5, purchaseType: 'guest', fulfillmentType: 'auto' }])) })
  for (const p of PAGES) {
    await go(page, S + p, diag); await sleep(1500)
    const lines = await page.evaluate(() => { const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT); const r = []; let n; while ((n = w.nextNode())) { const t = n.textContent.trim(); if (t && n.parentElement && n.parentElement.offsetParent !== null) r.push(t) } return r })
    const bad = loc === 'en-US' ? lines.filter((l) => /[一-鿿]/.test(l) && !DATA.test(l)) : lines.filter((l) => SIMP.test(l) && !DATA.test(l))
    const keys = lines.filter((l) => /^[a-z]+(\.[a-zA-Z_]+){1,}$/.test(l))
    const name = `I18N-${loc}${p.replace(/\//g, '_') || '_home'}`
    out[`${loc} ${p}`] = { bad: [...new Set(bad)].slice(0, 20), rawKeys: keys.slice(0, 10), shot: (bad.length || keys.length) ? await shot(page, name, true) : undefined }
  }
  out[`${loc} diag`] = diag
  await ctx.close()
}
record({ flow: 'I18N', out })
await b.close()
