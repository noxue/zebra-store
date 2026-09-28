# Nginx 部署

## 适合谁

服务器上已经有 Nginx（或者你习惯用 Nginx）的人。整体结构：

```text
浏览器 ──HTTPS──► Nginx ──┬── /、/admin/ ─────► 静态网页（/opt/zebra/web）
                          └── /api/ 等 ───────► zebra-store（127.0.0.1:8081，systemd 守护）
```

## 准备

| 项目 | 要求 |
|---|---|
| 服务器 | 64 位 Linux，下面以 Debian 12 / Ubuntu 22.04+ 为例，全程用 root |
| 域名 | `shop.example.com` 的 A 记录指向服务器，80/443 已放行 |
| 文件 | `zebra-store`、`storefront/dist`、`admin/dist`、`config.example.yml` |

<!--@include: ./parts/build.md-->

## 步骤

### 1. 安装 Nginx 和 certbot

```bash
apt update
apt install -y nginx certbot python3-certbot-nginx sqlite3
```

### 2. 上传文件到固定目录

在服务器上创建目录：

```bash
mkdir -p /opt/zebra/{data,uploads,web}
```

在你的电脑上上传：

```bash
scp backend/target/x86_64-unknown-linux-musl/release/zebra-store root@服务器:/opt/zebra/
scp -r storefront/dist root@服务器:/opt/zebra/web/storefront
scp -r admin/dist      root@服务器:/opt/zebra/web/admin
scp backend/config.example.yml root@服务器:/opt/zebra/config.yml
```

最终目录：

```text
/opt/zebra/
├── zebra-store          # 后端程序
├── config.yml           # 配置
├── data/                # SQLite 数据库（自动创建 zebra.db）
├── uploads/             # 上传的图片
└── web/
    ├── storefront/      # 里面直接是 index.html 和 assets/
    └── admin/
```

<!--@include: ./parts/config-min.md-->

### 3. 创建运行用户

```bash
useradd --system --home /opt/zebra --shell /usr/sbin/nologin zebra
chown -R zebra:zebra /opt/zebra
chmod 600 /opt/zebra/config.yml
chmod +x /opt/zebra/zebra-store
chmod o+rx /opt/zebra /opt/zebra/web      # 让 Nginx（www-data）能读取网页文件
```

### 4. 用 systemd 运行后端

创建 `/etc/systemd/system/zebra-store.service`：

```ini
[Unit]
Description=Zebra Store
After=network-online.target
Wants=network-online.target

[Service]
User=zebra
Group=zebra
WorkingDirectory=/opt/zebra
ExecStart=/opt/zebra/zebra-store --config /opt/zebra/config.yml serve
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=full
ReadWritePaths=/opt/zebra

[Install]
WantedBy=multi-user.target
```

```bash
systemctl daemon-reload
systemctl enable --now zebra-store
systemctl status zebra-store --no-pager        # 应显示 active (running)
curl -s http://127.0.0.1:8081/api/v1/public/config | head -c 120
```

### 5. 写 Nginx 站点配置

<!--@include: ./parts/proxy-paths.md-->

创建 `/etc/nginx/sites-available/zebra.conf`（CentOS / 宝塔以外的 RHEL 系放在 `/etc/nginx/conf.d/zebra.conf`）：

```nginx
server {
    listen 80;
    listen [::]:80;
    server_name shop.example.com;

    client_max_body_size 20m;

    # 接口、图片、SEO 文件、兼容对接协议 → 后端
    location ~ ^/(api|uploads|shared|plugin/open-api)/ {
        proxy_pass http://127.0.0.1:8081;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 120s;
    }
    location = /sitemap.xml { proxy_pass http://127.0.0.1:8081; proxy_set_header Host $host; }
    location = /robots.txt  { proxy_pass http://127.0.0.1:8081; proxy_set_header Host $host; }

    # 管理后台
    location = /admin { return 308 /admin/; }
    location /admin/ {
        alias /opt/zebra/web/admin/;
        try_files $uri $uri/ /admin/index.html;
    }

    # 用户前台
    location / {
        root /opt/zebra/web/storefront;
        try_files $uri $uri/ /index.html;
    }

    # 打包文件名带哈希，可以长期缓存
    location /assets/ {
        root /opt/zebra/web/storefront;
        expires 1y;
        add_header Cache-Control "public, immutable";
    }
}
```

启用并检查：

```bash
ln -s /etc/nginx/sites-available/zebra.conf /etc/nginx/sites-enabled/zebra.conf
rm -f /etc/nginx/sites-enabled/default        # 如果这台机器上没有别的网站
nginx -t && systemctl reload nginx
```

此时 `http://shop.example.com` 应该已经能打开。

### 6. 申请 HTTPS 证书

```bash
certbot --nginx -d shop.example.com --redirect -m 你的邮箱 --agree-tos
```

certbot 会自动在上面的配置里加上 `listen 443 ssl` 和证书路径，并把 HTTP 跳转到 HTTPS。
证书会通过 systemd 定时任务自动续期，可以用 `certbot renew --dry-run` 检查。

<!--@include: ./parts/real-ip.md-->

<!--@include: ./parts/verify.md-->

## 升级

```bash
# 1. 备份（见下方）
# 2. 上传新文件到临时位置，然后：
systemctl stop zebra-store
install -m 755 -o zebra -g zebra /tmp/zebra-store /opt/zebra/zebra-store
rm -rf /opt/zebra/web/storefront /opt/zebra/web/admin
mv /tmp/storefront-dist /opt/zebra/web/storefront
mv /tmp/admin-dist /opt/zebra/web/admin
systemctl start zebra-store
```

新版本启动时会自动补齐新表和新字段，业务数据保留。

<!--@include: ./parts/backup-short.md-->

这一页的“数据目录”是 `/opt/zebra/data`，“程序目录”是 `/opt/zebra`。

## 常见问题

**502 Bad Gateway**
后端没运行。`systemctl status zebra-store` 和 `journalctl -u zebra-store -n 50` 查看原因，最常见的是三个密钥不合格。

**403 Forbidden 或者打开网页 404**
Nginx 读不到网页文件：确认 `/opt/zebra/web/storefront/index.html` 存在，并且执行过 `chmod o+rx /opt/zebra /opt/zebra/web`。

**后台 `/admin/` 空白、控制台 JS 404**
后台打包时没加 `--base=/admin/`。重新打包上传。

**风控 / 登录日志里全是 127.0.0.1**
`proxy_set_header X-Forwarded-For` 没配，或者 Nginx 不在本机、没加进 `trusted_proxies`。

**`client intended to send too large body`**
上传的图片超过 `client_max_body_size`，调大它（后端自身上限由 `upload.max_size` 决定，默认 10 MB）。
