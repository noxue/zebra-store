# Caddy 部署

::: tip 更简单的单文件方式
推荐先按 [单文件二进制部署](./binary) 安装。Release 二进制已经包含前台和后台，Caddyfile 只需要
`reverse_proxy 127.0.0.1:8081`，不再需要上传和配置两个 `dist` 目录。
:::

## 适合谁

一台**全新的服务器**，想用最少的配置得到一个 HTTPS 网站的人。
[Caddy](https://caddyserver.com) 会自动申请和续期 Let's Encrypt 证书，整个 Web 服务器配置只有二十几行。

```text
浏览器 ──HTTPS──► Caddy ──┬── /、/admin/ ─────► 静态网页（/opt/zebra/web）
                          └── /api/ 等 ───────► zebra-store（127.0.0.1:8081，systemd 守护）
```

## 准备

| 项目 | 要求 |
|---|---|
| 服务器 | 64 位 Linux，下面以 Debian 12 / Ubuntu 22.04+ 为例，全程用 root |
| 域名 | `shop.example.com` 的 A 记录指向服务器 |
| 端口 | 80 和 443 没有被其他程序（例如 Nginx、Apache）占用，并在防火墙/安全组放行 |
| 文件 | `zebra-store`、`storefront/dist`、`admin/dist`、`config.example.yml` |

<!--@include: ./parts/build.md-->

## 步骤

### 1. 安装 Caddy

```bash
apt install -y debian-keyring debian-archive-keyring apt-transport-https curl sqlite3
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' > /etc/apt/sources.list.d/caddy-stable.list
apt update && apt install -y caddy
```

其他系统见 [Caddy 官方安装文档](https://caddyserver.com/docs/install)。

### 2. 上传文件到固定目录

```bash
mkdir -p /opt/zebra/{data,uploads,web}
```

在你的电脑上：

```bash
scp backend/target/x86_64-unknown-linux-musl/release/zebra-store root@服务器:/opt/zebra/
scp -r storefront/dist root@服务器:/opt/zebra/web/storefront
scp -r admin/dist      root@服务器:/opt/zebra/web/admin
scp backend/config.example.yml root@服务器:/opt/zebra/config.yml
```

<!--@include: ./parts/config-min.md-->

### 3. 创建运行用户

```bash
useradd --system --home /opt/zebra --shell /usr/sbin/nologin zebra
chown -R zebra:zebra /opt/zebra
chmod 600 /opt/zebra/config.yml
chmod +x /opt/zebra/zebra-store
chmod o+rx /opt/zebra /opt/zebra/web      # 让 caddy 用户能读取网页文件
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
systemctl status zebra-store --no-pager
curl -s http://127.0.0.1:8081/api/v1/public/config | head -c 120
```

### 5. 写 Caddyfile

<!--@include: ./parts/proxy-paths.md-->

把 `/etc/caddy/Caddyfile` 的内容**整个替换**为：

```text
shop.example.com {
	encode zstd gzip

	# 接口、图片、SEO 文件、兼容对接协议 → 后端
	@backend path /api/* /uploads/* /sitemap.xml /robots.txt /shared/* /plugin/open-api/*
	handle @backend {
		reverse_proxy 127.0.0.1:8081
	}

	# 管理后台
	redir /admin /admin/ 308
	handle_path /admin/* {
		root * /opt/zebra/web/admin
		try_files {path} /index.html
		file_server
	}

	# 用户前台
	handle {
		root * /opt/zebra/web/storefront
		try_files {path} /index.html
		file_server
	}

	header /assets/* Cache-Control "public, max-age=31536000, immutable"
}
```

```bash
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
```

几秒钟后 Caddy 会自动拿到证书，`https://shop.example.com` 就能访问了。
`journalctl -u caddy -f` 可以看到申请证书的过程。

<!--@include: ./parts/real-ip.md-->

Caddy 和后端在同一台机器上，保持默认即可。Caddy 会自动带上 `X-Forwarded-For` 和原始 `Host`。

<!--@include: ./parts/verify.md-->

## 升级

```bash
# 1. 备份（见下方）
# 2. 上传新文件到 /tmp，然后：
systemctl stop zebra-store
install -m 755 -o zebra -g zebra /tmp/zebra-store /opt/zebra/zebra-store
rm -rf /opt/zebra/web/storefront /opt/zebra/web/admin
mv /tmp/storefront-dist /opt/zebra/web/storefront
mv /tmp/admin-dist /opt/zebra/web/admin
systemctl start zebra-store
```

新版本启动时自动补齐表结构，业务数据保留。网页换了之后 Caddy 不需要重启。

<!--@include: ./parts/backup-short.md-->

这一页的“数据目录”是 `/opt/zebra/data`，“程序目录”是 `/opt/zebra`。

## 常见问题

**证书一直申请不下来**
- 域名是否已经解析到这台服务器（`ping shop.example.com` 看 IP）；
- Cloudflare 用户请把云朵设为灰色（仅 DNS），或者改用 Cloudflare 的源站证书；
- 80/443 是否被占用：`ss -ltnp | grep -E ':80 |:443 '`；
- 云服务商安全组是否放行 80/443。

**`permission denied` 读取网页文件**
执行 `chmod o+rx /opt/zebra /opt/zebra/web`，并确认 `web/storefront/index.html` 存在。

**502**
后端没运行，`journalctl -u zebra-store -n 50` 查看原因。

**要开分站（子域名）**
在第一行域名后面追加分站域名，例如 `shop.example.com, sakura.example.com, neon.example.com {`，
每个子域名都要先加 DNS 解析。详见 [分站](/reseller/#开启分销功能)。
