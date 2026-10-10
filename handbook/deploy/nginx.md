# Nginx 部署

Nginx 只负责 HTTPS 和整站反向代理。GitHub Release 的 `zebra-store` 单文件已经包含用户前台、`/admin/` 管理后台、API 和后台任务，不需要静态网站目录，不需要上传 `dist`，也不需要克隆仓库。

```text
浏览器 / Cloudflare → Nginx :443 → zebra-store 127.0.0.1:8081
```

## 1. 安装单文件

在服务器上用 root 执行：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

脚本只下载当前架构对应的 Release 压缩包和 `SHA256SUMS`，校验后安装一个二进制，并创建 systemd 服务。安装完成时会显示初始后台账号和随机密码，请立即保存。

检查程序：

```bash
systemctl status zebra-store --no-pager
curl -fsS http://127.0.0.1:8081/health
```

## 2. 配置 Nginx

在站点的 `server` 中把整个域名反向代理到单文件程序：

```nginx
server {
    listen 80;
    server_name shop.example.com;

    client_max_body_size 20m;

    location ^~ /api/v1/internal/ { return 404; }

    location / {
        proxy_pass http://127.0.0.1:8081;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 120s;
    }
}
```

检查并重载：

```bash
nginx -t
systemctl reload nginx
```

再用 Certbot、面板或现有证书配置 HTTPS。不要另外配置 `/api/`、`/admin/`、前台目录或后台目录。

### 可选：自动同步分站第三方域名

只经营主站时，无需以下配置；`reseller.enabled` 默认为 `false`。Nginx 没有与上述 Caddy `ask` 完全相同的内置回调流程；仓库提供独立的 [同步脚本](https://github.com/noxue/zebra-store/blob/main/scripts/sync-nginx-reseller-domains.py)，读取当前有效域名、用 Certbot webroot 申请缺失证书、生成 Nginx 站点块并在配置检查通过后重载。

本地 `hosts` 或模拟 DNS 可测试分站路由与同步逻辑，不能让 Certbot 完成公网 ACME 验证。脚本测试使用模拟证书文件与命令；真实签发需在公网 DNS 生效并可访问后验收。

若共享目标是 CDN，且 CDN 不把 `/.well-known/acme-challenge/` 转发到源站，下面的 Certbot 同步方式无法为客户域名完成 HTTP 验证。此时应由 CDN 管理客户域名证书，并按 CDN 文档配置回源 Host 与源站证书；同步脚本的源站 Certbot 流程适用于挑战请求能到达本机的部署。

1. 在 `config.yml` 设置 `reseller.enabled: true`、随机生成的 `reseller.domain_sync_token`。在管理后台「分销域名管理」填写共享 CNAME 目标，或用 `reseller.cname_target: connect.example.com` 预设；后台保存值优先。目标可指向本机，也可填 CDN 提供的接入域名；使用 CDN 时，还需在 CDN 配置自定义域名、回源和证书。代理的子域名 CNAME 到共享目标；根域名的 A/AAAA 或 ALIAS/ANAME 不满足 CNAME 校验。审核模式默认「管理员审核」，批准时检测 CNAME；「验证后自动通过」在提交时检测 CNAME，正确则立即启用，错误或 DNS 查询失败则拒绝提交。自动模式必须先配置共享目标；未配置目标的旧部署仍按原有人工审核流程运行。切换模式只影响新提交的域名，已有待审核记录仍由管理员处理。域名启用后还需等待同步脚本运行、申请证书并重载 Nginx，才能通过 HTTPS 访问。
2. 配置固定的 80 端口 ACME 挑战入口，并在 Nginx 的 `http` 作用域引入脚本生成的文件。先创建空的 `/etc/nginx/zebra-resellers.conf` 和 `/var/www/zebra-acme`，以便首次配置检查通过；如果已有 `default_server`，把挑战路径合并到已有的 80 端口默认站点：

```nginx
server {
    listen 80 default_server;
    server_name _;
    location ^~ /.well-known/acme-challenge/ {
        root /var/www/zebra-acme;
    }
    location / { return 308 https://$host$request_uri; }
}
include /etc/nginx/zebra-resellers.conf;
```

3. 从仓库复制同步脚本到服务器；把密钥保存到只有同步任务可读的文件（例如 `/etc/zebra-store/domain-sync-token`，权限 `0600`），然后定时执行：

```bash
python3 /opt/zebra-store/scripts/sync-nginx-reseller-domains.py \
  --token-file /etc/zebra-store/domain-sync-token \
  --email admin@example.com
```

脚本路径按实际安装位置调整。为 Certbot 续期配置 deploy hook `systemctl reload nginx`。未配置 `domain_sync_token` 时清单接口返回 404；分站功能关闭时，两个新接口均返回 404，不影响已有主站。分站域名停用后会从新配置中移除，旧证书文件由运维自行清理。
主站的 Nginx 站点也应阻止外部访问 `/api/v1/internal/`；同步脚本经本机地址直接访问后端。

如果按 [Docker Release 单文件部署](./docker)，把 `proxy_pass` 改为 `http://127.0.0.1:18180`。

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

只开放 80、443，8081 不要对公网开放。

## 后台密码

默认用户名是 `admin`。初始密码会在安装完成时显示，也写在 `/opt/zebra-store/config.yml`：

```bash
grep -E 'default_admin_(username|password)' /opt/zebra-store/config.yml
```

后台修改密码后无法读取原密码。忘记密码时执行：

```bash
NEW_PASSWORD="Zs9-$(openssl rand -hex 10)"
zebra-store --config /opt/zebra-store/config.yml admin reset-password \
  --username admin --password "$NEW_PASSWORD"
printf '新后台密码：%s\n' "$NEW_PASSWORD"
```

## 升级

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

安装脚本会保留配置、数据库和上传文件，只替换 Release 单文件并重启服务。
