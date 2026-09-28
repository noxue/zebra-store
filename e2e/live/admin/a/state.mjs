export default async ({ api, log }) => {
  for (const p of ['/admin/authz/me', '/admin/authz/admins', '/admin/authz/roles?include_metadata=true', '/admin/compliance/status', '/admin/system/version', '/admin/system/update/capability', '/admin/2fa/status', '/admin/authz/audit-logs?page=1&page_size=5']) {
    const r = await api('GET', p); log(p, JSON.stringify(r).slice(0, 1800))
  }
}
