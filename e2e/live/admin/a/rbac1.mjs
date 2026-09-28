// B-001 login errors, B-040 builtin roles, B-041/B-042 setup, B-043 protected admin
import { browser as mkBrowser } from '../lib.mjs'
const notices = async (page) => (await page.locator('[role=status]').allInnerTexts()).map((s) => s.trim()).filter(Boolean)
async function toast(page, ms = 2500) { await page.waitForTimeout(ms); return notices(page) }

export default async ({ b, page, api, shot, log, admin, SITES }) => {
  // ---- B-001: login errors in a fresh context
  const c2 = await b.newContext({ viewport: { width: 1440, height: 900 }, locale: 'zh-CN' })
  const lp = await c2.newPage()
  const errs = []
  lp.on('response', async (r) => { if (r.url().includes('/admin/login')) { try { errs.push(`${r.status()} ${JSON.stringify(await r.json())}`) } catch {} } })
  await lp.goto(`${admin}/login`)
  await lp.waitForLoadState('networkidle')
  await shot(lp, 'a-login-page')
  // empty submit
  await lp.locator('button[type=submit]').click()
  log('empty submit notices', await toast(lp, 1200), 'url', lp.url())
  for (const [u, p] of [['qa-nobody-x', 'whatever123'], ['admin', 'definitely-wrong-pw']]) {
    await lp.locator('input').first().fill(u)
    await lp.locator('input[type=password]').fill(p)
    await lp.locator('button[type=submit]').click()
    log('login', u, 'notices', await toast(lp, 2000))
  }
  await shot(lp, 'a-login-error')
  log('login responses', errs.join(' || '))
  await c2.close()

  // ---- B-040: roles list
  await page.goto(`${admin}/authz`)
  await page.waitForLoadState('networkidle')
  const roleNames = await page.locator('text=/^(finance|integration|operations|readonly_auditor|support|system_admin)$/').allInnerTexts()
  log('B-040 builtin role labels visible', roleNames.length, roleNames.join(','))
  // select readonly_auditor and try to add policy (should be disabled)
  await page.getByText('readonly_auditor', { exact: true }).first().click()
  await page.waitForTimeout(800)
  const addBtnDisabled = await page.getByRole('button', { name: /添加策略/ }).isDisabled()
  const revokeDisabled = await page.locator('button:has-text("删除")').filter({ hasText: '删除' }).nth(3).isDisabled().catch(() => 'n/a')
  log('immutable: add policy disabled', addBtnDisabled, 'revoke disabled', revokeDisabled)
  // try deleting a builtin role through trash icon
  const dr = await api('DELETE', '/admin/authz/roles/role%3Areadonly_auditor')
  log('API delete builtin role', JSON.stringify(dr))
  const gp = await api('POST', '/admin/authz/policies', { role: 'role:readonly_auditor', object: '/admin/users', action: 'GET' })
  log('API grant on builtin role', JSON.stringify(gp))
  const pol = await api('GET', '/admin/authz/roles/role%3Areadonly_auditor/policies')
  log('readonly_auditor policies', (pol.data || []).map((x) => `${x.action || x.method} ${x.object}`).join(' ; '))

  // ---- create role qa-orders-viewer via UI
  const roleInput = page.locator('input[placeholder*="operations_"]')
  await roleInput.fill('qa-orders-viewer')
  await page.getByRole('button', { name: '创建', exact: true }).click()
  log('create role notices', await toast(page))
  await page.waitForTimeout(800)
  const roles = await api('GET', '/admin/authz/roles?include_metadata=true')
  log('roles now', JSON.stringify(roles.data))
  const myRole = (roles.data || []).map((r) => r.role).find((r) => r.includes('qa-orders-viewer') || r.includes('qa_orders_viewer'))
  // select it and grant via catalog search
  await page.getByText(/^qa[-_]orders[-_]viewer$/).first().click().catch((e) => log('select role fail', e.message))
  await page.waitForTimeout(800)
  await page.locator('input[placeholder*="orders GET"]').fill('orders GET')
  await page.waitForTimeout(800)
  await shot(page, 'a-catalog-search')
  const rows = page.locator('div', { hasText: /^GET\s*\/admin\/orders$/ })
  log('catalog rows GET /admin/orders', await rows.count())
  // find the add button in the item containing exactly "/admin/orders"
  const item = page.locator('code, span, div').filter({ hasText: /^\/admin\/orders$/ }).first()
  const card = item.locator('xpath=ancestor::*[.//button][1]')
  await card.locator('button').first().click()
  log('grant notices', await toast(page))
  const pols = await api('GET', `/admin/authz/roles/${encodeURIComponent(myRole || 'role:qa-orders-viewer')}/policies`)
  log('qa role policies', JSON.stringify(pols.data))
  await shot(page, 'a-custom-role-policies')

  // ---- create admins qa-auditor and qa-admin via UI
  for (const [u, pw] of [['qa-auditor', 'QaAudit#2026pass'], ['qa-admin', 'QaAdmin#2026pass']]) {
    await page.locator('input[placeholder*="3-64"]').fill(u)
    await page.locator('input[autocomplete="new-password"]').fill(pw)
    await page.getByRole('button', { name: '创建管理员' }).click()
    log('create admin', u, await toast(page))
  }
  // validation: short username / empty password
  await page.locator('input[placeholder*="3-64"]').fill('ab')
  await page.locator('input[autocomplete="new-password"]').fill('')
  await page.getByRole('button', { name: '创建管理员' }).click()
  log('create admin invalid (ab, empty pw)', await toast(page))
  await page.locator('input[placeholder*="3-64"]').fill('qa-weak')
  await page.locator('input[autocomplete="new-password"]').fill('123')
  await page.getByRole('button', { name: '创建管理员' }).click()
  log('create admin weak pw 123', await toast(page))
  await page.locator('input[placeholder*="3-64"]').fill('qa-admin')
  await page.locator('input[autocomplete="new-password"]').fill('QaAdmin#2026pass')
  await page.getByRole('button', { name: '创建管理员' }).click()
  log('create admin duplicate', await toast(page))
  await page.getByRole('button', { name: '重置' }).first().click()

  const admins = (await api('GET', '/admin/authz/admins')).data
  log('admins', JSON.stringify(admins.map((a) => [a.id, a.username, a.is_super, a.roles])))
  const idOf = (u) => admins.find((a) => a.username === u)?.id

  // ---- assign roles via UI section
  const assign = async (u, role) => {
    const sel = page.locator('select').filter({ has: page.locator(`option:text-matches("^${u}")`) }).first()
    const opts = await sel.locator('option').allInnerTexts()
    const idx = opts.findIndex((o) => o.startsWith(u))
    await sel.selectOption({ index: idx })
    await page.waitForTimeout(800)
    await page.locator('label').filter({ hasText: new RegExp(`^\\s*${role}\\s*$`) }).first().click()
    await page.getByRole('button', { name: '保存角色分配' }).click()
    log('assign', u, role, await toast(page))
  }
  await assign('qa-auditor', 'readonly_auditor')
  await assign('qa-admin', 'qa-orders-viewer')
  const admins2 = (await api('GET', '/admin/authz/admins')).data
  log('admins after assign', JSON.stringify(admins2.map((a) => [a.id, a.username, a.roles])))
  await shot(page, 'a-authz-after-setup')

  // ---- B-043: delete bootstrap admin via UI
  await page.locator('tr', { hasText: 'admin' }).filter({ hasNotText: 'qa-' }).first().getByRole('button', { name: '删除' }).click()
  await page.waitForTimeout(500)
  await shot(page, 'a-delete-bootstrap-confirm', false)
  await page.locator('[role=dialog] button').last().click()
  log('B-043 delete bootstrap notices', await toast(page))
  const d1 = await api('DELETE', `/admin/authz/admins/${idOf('admin')}`)
  log('B-043 api', JSON.stringify(d1))
  // demote bootstrap via update?
  const up = await api('PUT', `/admin/authz/admins/${idOf('admin')}`, { username: 'admin', is_super: false })
  log('B-043 demote bootstrap api', JSON.stringify(up))
  const after = (await api('GET', '/admin/authz/admins')).data.find((a) => a.username === 'admin')
  log('bootstrap after', JSON.stringify(after))
  await shot(page, 'a-authz-final')
}
