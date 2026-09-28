/**
 * admin 站点的挂载前缀，运行时解析。
 *
 * fullstack 单二进制模式下 admin 可以挂在任意自定义路径（config.yml 的 `web.admin_path`），
 * 该前缀在构建期是未知的：Vite 只往 index.html 注入 `<base href="__DJ_ADMIN_BASE__/">`
 * 占位符，由后端启动时替换成实际值（见 internal/web/handler.go）。
 *
 * 所以凡是绕过 vue-router 自行拼接的 URL —— 原生 `<a href>`、`window.location` 跳转 ——
 * 都必须走这里运行时读取，不能依赖构建期的 `import.meta.env`。
 *
 * 反过来，`<router-link :to>` 和 `router.push()` 不要用本模块：vue-router 的 history
 * 已经带了同一个 base，再拼一层会得到 `/admin/admin/...` 的双重前缀。
 */

// 必须与后端 internal/web/handler.go 的 adminBasePlaceholder 保持一致
const BASE_PLACEHOLDER = '__DJ_ADMIN_BASE__'

function resolveAdminBase(): string {
  const fromTag =
    typeof document !== 'undefined'
      ? document.querySelector('base')?.getAttribute('href')
      : null
  const raw = fromTag || import.meta.env.BASE_URL || '/'

  // 占位符没被后端替换（例如直接 vite preview 查看 fullstack 产物）时退回根路径，
  // 避免生成 /__DJ_ADMIN_BASE__/users/1 这种无效链接。
  if (raw.includes(BASE_PLACEHOLDER)) return ''

  const trimmed = raw.replace(/\/+$/, '')
  // './' 是 fullstack 构建的 Vite base，语义上等于「挂在当前根」，同样按空前缀处理。
  return trimmed === '.' ? '' : trimmed
}

/** admin 的挂载前缀，不带尾斜杠。例如 ''、'/admin'、'/dj-mgmt-7x9k2'。 */
export const ADMIN_BASE = resolveAdminBase()

/**
 * 把 admin 内部路由路径拼成可直接用于 `<a href>` / `window.location` 的完整路径。
 *
 * @example adminUrl('/users/1') // admin 挂在 /admin 时得到 '/admin/users/1'
 */
export function adminUrl(path: string): string {
  return `${ADMIN_BASE}${path.startsWith('/') ? path : `/${path}`}`
}
