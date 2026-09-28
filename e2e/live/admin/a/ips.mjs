export default async ({ api, log }) => {
  const r = await api('GET', '/admin/user-login-logs?page=1&page_size=8'); log(JSON.stringify((r.data||[]).map(x=>[x.email,x.client_ip,x.status,x.fail_reason,x.created_at])))
  const a = await api('GET', '/admin/authz/admins'); log(JSON.stringify(a.data.map(x=>[x.username,x.last_login_at,x.totp_enabled])))
}
