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
