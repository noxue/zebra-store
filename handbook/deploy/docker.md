# Docker Compose 部署

## 适合谁

服务器上已经在用 Docker，希望用一个 `compose.yml` 管理斑马小铺的人。
这一页的 Compose 只包含斑马小铺自己：

| 容器 | 作用 |
|---|---|
| `zebra` | 后端程序，数据库文件和上传图片放在 Docker 卷里 |
| `web` | Caddy：提供前台和后台网页、转发接口、**自动申请 HTTPS 证书** |

镜像直接从源码构建，服务器上**不需要**安装 Rust 或 Node.js。

## 准备

| 项目 | 要求 |
|---|---|
| 服务器 | 64 位 Linux；**构建镜像时需要约 2 GB 内存**（内存小可以加 swap，或在别的机器构建后推送） |
| Docker | Docker Engine 24+，带 Compose v2（`docker compose version` 能输出版本号） |
| 域名 | `shop.example.com` 的 A 记录指向服务器；80 和 443 端口没有被其他程序占用 |
| 源码 | 斑马小铺的代码仓库 |

还没装 Docker 的话，按 [Docker 官方文档](https://docs.docker.com/engine/install/) 安装，或执行官方脚本
`curl -fsSL https://get.docker.com | sh`。

## 步骤

### 1. 准备目录和源码

```bash
mkdir -p /opt/zebra-docker && cd /opt/zebra-docker
git clone <仓库地址> src
```

最终目录结构：

```text
/opt/zebra-docker/
├── src/                  # 斑马小铺源码
├── Dockerfile.backend    # 后端镜像
├── Dockerfile.web        # 网页 + Caddy 镜像
├── compose.yml
├── config.yml            # 后端配置
└── Caddyfile             # Web 服务器配置
```

### 2. 写两个 Dockerfile

`/opt/zebra-docker/Dockerfile.backend`：

```dockerfile
# 编译阶段：用官方 Rust 镜像编译后端
FROM rust:1-bookworm AS build
WORKDIR /src
COPY backend ./backend
RUN cd backend && cargo build --release -p zs-server \
    && cp target/release/zebra-store /zebra-store

# 运行阶段：只包含程序本身
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /app zebra \
    && mkdir -p /app/data /app/uploads && chown -R zebra:zebra /app
COPY --from=build /zebra-store /usr/local/bin/zebra-store
WORKDIR /app
USER zebra
VOLUME ["/app/data", "/app/uploads"]
EXPOSE 8081
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=6 \
    CMD wget -q -O /dev/null http://127.0.0.1:8081/api/v1/public/config || exit 1
ENTRYPOINT ["/usr/local/bin/zebra-store", "--config", "/app/config.yml"]
CMD ["serve"]
```

`/opt/zebra-docker/Dockerfile.web`：

```dockerfile
# 打包前台和后台网页
FROM node:20-alpine AS web
WORKDIR /src
COPY storefront ./storefront
COPY admin ./admin
RUN cd storefront && npm ci && npm run build
RUN cd admin && npm ci && npx vite build --base=/admin/

# Caddy + 网页
FROM caddy:2.10-alpine
COPY --from=web /src/storefront/dist /srv/storefront
COPY --from=web /src/admin/dist /srv/admin
```

### 3. 写 compose.yml

```yaml
name: zebra

services:
  zebra:
    build:
      context: ./src
      dockerfile: ../Dockerfile.backend
    image: zebra-store:local
    restart: unless-stopped
    environment:
      TZ: UTC
    volumes:
      - ./config.yml:/app/config.yml:ro
      - zebra_data:/app/data
      - zebra_uploads:/app/uploads
    networks: [zebra]

  web:
    build:
      context: ./src
      dockerfile: ../Dockerfile.web
    image: zebra-web:local
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
      - "443:443/udp"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy_data:/data
      - caddy_config:/config
    depends_on: [zebra]
    networks: [zebra]

networks:
  zebra:
    ipam:
      config:
        - subnet: 172.28.0.0/24     # 固定网段，下面 trusted_proxies 要用

volumes:
  zebra_data:
  zebra_uploads:
  caddy_data:
  caddy_config:
```

::: tip 网段冲突
如果服务器上已有 Docker 网络占用了 `172.28.0.0/24`，换一个没被用的网段（例如 `172.29.0.0/24`），
并同步修改下面 `config.yml` 里的 `trusted_proxies`。
:::

### 4. 写 config.yml

```bash
cp src/backend/config.example.yml config.yml
```

然后改成下面这样（三个密钥各执行一次 `openssl rand -hex 24` 生成）：

```yaml
app:
  secret_key: 第一串随机字符
  totp_issuer: Zebra-Store
server:
  host: 0.0.0.0                   # 容器里要监听所有地址，外部访问由 web 容器转发
  port: 8081
  mode: release
  trusted_proxies: ["172.28.0.0/24"]   # 相信同一 Docker 网络里的 Caddy 转发的真实 IP
log:
  level: info
database:
  url: sqlite://data/zebra.db?mode=rwc     # 即容器里的 /app/data/zebra.db（zebra_data 卷）
jwt:
  secret: 第二串随机字符
user_jwt:
  secret: 第三串随机字符
bootstrap:
  default_admin_username: admin
  default_admin_password: 你的管理员密码
upload:
  dir: uploads                              # 即 /app/uploads（zebra_uploads 卷）
cors:
  allowed_origins: ["https://shop.example.com"]
```

### 5. 写 Caddyfile

```text
shop.example.com {
	encode zstd gzip

	@backend path /api/* /uploads/* /sitemap.xml /robots.txt /shared/* /plugin/open-api/*
	handle @backend {
		reverse_proxy zebra:8081
	}

	redir /admin /admin/ 308
	handle_path /admin/* {
		root * /srv/admin
		try_files {path} /index.html
		file_server
	}

	handle {
		root * /srv/storefront
		try_files {path} /index.html
		file_server
	}

	header /assets/* Cache-Control "public, max-age=31536000, immutable"
}
```

把 `shop.example.com` 换成你的域名。Caddy 会自动申请并续期 Let's Encrypt 证书。

<!--@include: ./parts/proxy-paths.md-->

### 6. 构建并启动

```bash
cd /opt/zebra-docker
docker compose build          # 第一次 10–20 分钟（编译 Rust）
docker compose up -d
docker compose ps             # zebra 显示 healthy、web 显示 running
docker compose logs -f zebra  # 看后端日志，Ctrl+C 退出
```

## 用 PostgreSQL 代替 SQLite（可选）

在 `compose.yml` 的 `services:` 下加一个数据库服务，在 `volumes:` 下加 `pg_data:`：

```yaml
  db:
    image: postgres:16-alpine
    restart: unless-stopped
    environment:
      POSTGRES_USER: zebra
      POSTGRES_PASSWORD: 换成一个强密码
      POSTGRES_DB: zebra
    volumes:
      - pg_data:/var/lib/postgresql/data
    networks: [zebra]
```

同时给 `zebra` 服务加上 `depends_on: [db]`，把 `config.yml` 改成：

```yaml
database:
  url: postgres://zebra:换成一个强密码@db:5432/zebra
```

然后 `docker compose up -d`。详见 [切换数据库](./database)。

<!--@include: ./parts/verify.md-->

## 升级

```bash
cd /opt/zebra-docker
# 先备份（见下方）
git -C src pull
docker compose build
docker compose up -d          # 只重建有变化的容器；新版本启动时自动补齐表结构
docker image prune -f         # 可选：清理旧镜像
```

## 备份

**不停服务的数据库备份**：镜像里没有 `sqlite3`，用程序自带的 `backup` 命令（SQLite `VACUUM INTO`）：

```bash
cd /opt/zebra-docker && mkdir -p backup
T=$(date +%F-%H%M)
docker compose exec zebra zebra-store --config /app/config.yml backup --output data/backup/zebra-$T.db
docker compose cp zebra:/app/data/backup/zebra-$T.db backup/
docker compose exec zebra rm /app/data/backup/zebra-$T.db
```

**整卷备份**（需要短暂停止后端）：数据在 Docker 卷里（卷名是“项目名_卷名”，即 `zebra_zebra_data` 和 `zebra_zebra_uploads`）：

```bash
cd /opt/zebra-docker && mkdir -p backup
docker compose stop zebra
docker run --rm -v zebra_zebra_data:/d -v "$PWD/backup":/b alpine \
  tar czf /b/data-$(date +%F).tgz -C /d .
docker run --rm -v zebra_zebra_uploads:/d -v "$PWD/backup":/b alpine \
  tar czf /b/uploads-$(date +%F).tgz -C /d .
cp config.yml backup/config-$(date +%F).yml
docker compose start zebra
```

恢复时把压缩包解回对应的卷（先 `docker compose stop zebra`）：

```bash
docker run --rm -v zebra_zebra_data:/d -v "$PWD/backup":/b alpine \
  sh -c "rm -rf /d/* && tar xzf /b/data-2026-01-01.tgz -C /d"
```

用 PostgreSQL 时再加一步：`docker compose exec db pg_dump -Fc -U zebra zebra > backup/zebra-$(date +%F).dump`
（内置的 `backup` 命令只支持 SQLite；MySQL 用 `mysqldump --single-transaction`）。

## 常见问题

**`docker compose build` 时被杀掉（Killed / exit code 137）**
内存不够编译 Rust。加 2 GB swap（`fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile && swapon /swapfile`）后重试，
或者在别的机器上构建，然后用 `docker save zebra-store:local | ssh 服务器 docker load` 传过去。

**证书申请失败**
确认域名已经解析到这台服务器、80/443 端口没有被别的程序占用（`ss -ltnp | grep -E ':80|:443'`）、云服务商安全组已放行。
`docker compose logs web` 里有详细原因。

**已经有别的反向代理占用了 80/443**
去掉 `web` 服务的 `ports`，改成 `- "127.0.0.1:8080:80"`，把 Caddyfile 第一行改成 `:80`，
再在原来的反向代理里把 `shop.example.com` 整站转发到 `127.0.0.1:8080`（保留 Host 头）。
此时 `trusted_proxies` 需要同时包含 Docker 网段。

**后端日志提示 `unable to open database file`**
卷的权限不对，通常是手动创建过同名卷。执行 `docker compose down`、`docker volume rm zebra_zebra_data`（**会删除数据**，仅限新站）后重新 `up`。
