# Caddy 部署

Caddy 只负责 HTTPS 和反向代理。GitHub Release 的 `zebra-store` 单文件已经包含用户前台、`/admin/` 管理后台、API 和后台任务，不需要静态网站目录，不需要上传 `dist`，也不需要克隆仓库。

```text
浏览器 / Cloudflare → Caddy :443 → zebra-store 127.0.0.1:8081
```

## 1. 安装单文件

在服务器上用 root 执行：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

脚本只下载当前服务器架构对应的 Release 压缩包和 `SHA256SUMS`，校验后安装一个二进制，并创建 systemd 服务。安装完成时会显示初始后台账号和随机密码，请立即保存。

检查程序：

```bash
systemctl status zebra-store --no-pager
curl -fsS http://127.0.0.1:8081/health
curl -fsS http://127.0.0.1:8081/api/v1/public/config | head -c 200
```

## 2. 配置 Caddy

在 Caddyfile 中添加：

```caddyfile
shop.example.com {
	encode zstd gzip
	reverse_proxy 127.0.0.1:8081
}
```

然后校验并热重载：

```bash
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
```

必须反向代理整个域名。不要另外配置 `/api/`、`/admin/`、前台目录或后台目录。

### 可选：分站绑定第三方域名

只经营主站时，无需以下配置；`reseller.enabled` 默认为 `false`。开启分站后，在管理后台「分销域名管理」填写共享 CNAME 目标，也可在 `config.yml` 中预设 `reseller.cname_target: connect.example.com`。后台保存的值优先。目标可以是指向本机的接入域名，也可以是 CDN 提供的接入域名；使用 CDN 时，需在 CDN 侧配置自定义域名接入、回源及证书。代理的子域名（例如 `shop.agent.com`）CNAME 到该目标。无需为每个代理分配独立目标。启用此校验时仅接受能公开查询到 CNAME 的域名；根域名的 A/AAAA 或 ALIAS/ANAME 不满足校验。

在管理后台「分销域名管理」选择审核模式：默认「管理员审核」，提交后待审核，管理员批准时才检测 CNAME 并启用；选择「验证后自动通过」时，提交时检测 CNAME，正确则立即启用，错误或查询失败则拒绝提交。自动模式需要先配置共享目标，否则后台不允许保存。后台目标留空且配置文件也未设置目标的旧部署继续使用原有人工审核流程。模式切换只影响新提交的域名，已有待审核记录仍由管理员处理。域名启用后，Caddy 的 `ask` 才会允许其申请证书；自动启用不代表证书已签发。

本地 `hosts` 或模拟 DNS 适合验证域名路由和 CNAME 校验，不能完成公网 ACME 域名所有权验证；真实证书签发需在公网 DNS 生效并可访问后验收。

在现有主站块之外，可添加一个动态分站块（后端端口按实际部署修改）。如果 Caddyfile 已有顶部全局配置块，把 `on_demand_tls` 合并进原块，不要创建第二个全局块：

```caddyfile
{
    on_demand_tls {
        ask http://127.0.0.1:8081/api/v1/internal/reseller-domains/allow
    }
}

https:// {
    tls {
        on_demand
    }
    @internal path /api/v1/internal/*
    respond @internal 404
    reverse_proxy 127.0.0.1:8081
}
```

Caddy 会对首次访问的单个域名向 `ask` 接口发送 `?domain=...`；接口仅对有效、已审核且分站仍启用的域名返回 204。未知、待审核、禁用或主站域名不会获准签发。若后端运行在 Docker 内，`ask` 地址须改成 Caddy 可访问的后端地址。不要把 Caddy `ask` 指向公开域名，以免绕经自身反代。

主站的 Caddy 站点块也应阻止外部访问 `/api/v1/internal/*`；Caddy 仍可从本机直接调用后端的 `ask` 地址。

如果 Caddy 在 Docker 中运行，而 `zebra-store` 直接运行在宿主机，请使用宿主机可达地址；如果按 [Docker Release 单文件部署](./docker)，宿主机 Caddy 反向代理到 `127.0.0.1:18180`。

## 3. 验证

```bash
curl -I https://shop.example.com/
curl -I https://shop.example.com/admin/
curl -fsS https://shop.example.com/health
curl -fsS https://shop.example.com/api/v1/public/config | head -c 200
```

- `/` 是用户前台；
- `/admin/` 是管理后台；
- `/api/v1/` 是 API；
- `/health` 应返回 `{"status":"ok"}`。

Cloudflare 小黄云可以开启，SSL/TLS 模式建议使用“完全（严格）”。源站开放 80、443，8081 保持仅本机访问。

## 后台密码

默认用户名是 `admin`。初始密码会在安装完成时显示，也写在 `/opt/zebra-store/config.yml`：

```bash
grep -E 'default_admin_(username|password)' /opt/zebra-store/config.yml
```

后台修改密码后无法从数据库读取原密码。忘记密码时执行：

```bash
NEW_PASSWORD="Zs9-$(openssl rand -hex 10)"
zebra-store --config /opt/zebra-store/config.yml admin reset-password \
  --username admin --password "$NEW_PASSWORD"
printf '新后台密码：%s\n' "$NEW_PASSWORD"
```

## 升级

重新运行安装命令即可。配置、数据库和上传文件会保留，只替换 Release 单文件并重启服务：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```
