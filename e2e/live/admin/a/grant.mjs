export default async ({ api, log }) => {
  for (const [action, object] of [['GET','/admin/authz/me'],['GET','/admin/2fa/status'],['POST','/admin/2fa/setup'],['POST','/admin/2fa/enable'],['PUT','/admin/password'],['GET','/admin/compliance/status'],['GET','/admin/orders/:id']]) {
    const r = await api('POST', '/admin/authz/policies', { role: 'role:qa-orders-viewer', object, action }); log(action, object, r.status_code, r.msg)
  }
}
