export default async ({ page, admin, shot, log, problems }) => {
  const loc = process.argv[4] || 'zh-CN'
  const tabs = ['basic','template','navigation','about','legal','home_announcement','smtp','order_email_template','captcha','telegram','google','dashboard','upstream_sync']
  for (const t of tabs) {
    const n0 = problems.length
    await page.goto(`${admin}/settings?tab=${t}`); await page.waitForLoadState('networkidle'); await page.waitForTimeout(500)
    const info = await page.evaluate((loc) => {
      const main = document.querySelector('main') || document.body
      const text = main.innerText
      const keys = [...new Set(text.match(/\badmin\.[a-zA-Z0-9_.]+\b/g) || [])]
      let cjk = []
      if (loc === 'en-US') {
        const els = [...main.querySelectorAll('button,label,th,h1,h2,h3,h4,p,span,option,small')]
        cjk = [...new Set(els.filter(e => e.children.length === 0).map(e => e.textContent.trim()).filter(s => /[一-鿿]/.test(s) && s.length < 80))].slice(0, 20)
      }
      if (loc === 'zh-TW') {
        // simplified-only chars commonly left untranslated
        const els = [...main.querySelectorAll('button,label,th,h1,h2,h3,h4,p,span,option,small')]
        cjk = [...new Set(els.filter(e => e.children.length === 0).map(e => e.textContent.trim()).filter(s => /[设置单订户统启关项输码认证务选择开页数据图库线时间达额网实报邮]/.test(s) && s.length < 80))].slice(0, 20)
      }
      return { keys, cjk }
    }, loc)
    await shot(page, `b-settings-${loc}-${t}`)
    log(t, JSON.stringify(info), JSON.stringify(problems.slice(n0)))
  }
}
