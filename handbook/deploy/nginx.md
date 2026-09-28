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
