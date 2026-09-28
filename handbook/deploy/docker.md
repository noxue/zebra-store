# Docker 运行 Release 单文件（推荐）

这种方式与线上 `store.noxue.com` 的实际部署一致：Docker 构建镜像时直接从
[GitHub Releases](https://github.com/noxue/zebra-store/releases) 下载并校验 Linux musl 单文件，运行时只有一个
`zebra-store-app` 容器。前台、`/admin/` 后台、API 和后台任务都在同一个 `zebra-store` 二进制中。

```text
浏览器 / Cloudflare
        │
        ▼
Caddy 或 Nginx（HTTPS）
        │  reverse_proxy 127.0.0.1:18180
        ▼
zebra-store-app（Docker，单个 Release 二进制）
        ├── /                 用户前台
        ├── /admin/           管理后台
        ├── /api/v1/          API
        └── data + uploads    Docker 卷
```

服务器不需要 Rust、Node.js、Zig，也不会在服务器上编译源码。

## 准备

- 64 位 Linux，支持 `x86_64` 或 `aarch64`；
- Docker Engine 24 或更新版本，带 Compose v2；
- 80、443 端口对外开放；
- 18180 只绑定到 `127.0.0.1`，不需要开放防火墙。

确认 Docker 可用：

```bash
docker version
docker compose version
```

## 1. 获取部署文件

```bash
sudo mkdir -p /opt/zebra-store
sudo chown "$USER":"$(id -gn)" /opt/zebra-store
cd /opt/zebra-store
BASE=https://raw.githubusercontent.com/noxue/zebra-store/main
curl -fsSLO "$BASE/deploy/release/Dockerfile"
curl -fsSLo compose.yml "$BASE/deploy/release/compose.yml"
curl -fsSLo config.yml "$BASE/backend/config.example.yml"
```

不需要克隆仓库。部署目录只有三个必要文件：

```text
/opt/zebra-store/
├── Dockerfile       # 从 GitHub Release 下载、校验并复制单文件
├── compose.yml      # 一个容器、两个数据卷、本机端口 18180
└── config.yml       # 站点配置，不提交到 Git
```

## 2. 修改配置

编辑 `config.yml`，至少替换以下四项：

```yaml
app:
  secret_key: 换成至少32位随机字符串

jwt:
  secret: 换成至少32位随机字符串

user_jwt:
  secret: 换成至少32位随机字符串

bootstrap:
  default_admin_username: admin
  default_admin_password: 换成强密码
```

可以执行 `openssl rand -hex 32` 生成密钥。Docker 内监听地址保持 `0.0.0.0:8081`，SQLite 地址保持
`sqlite://data/zebra.db?mode=rwc`，上传目录保持 `uploads`。

## 3. 构建并启动

普通 Intel / AMD 服务器：

```bash
RELEASE=v0.1.4 docker compose build --no-cache
RELEASE=v0.1.4 docker compose up -d --no-build
```

ARM64 服务器把构建命令改为：

```bash
TARGET=aarch64 RELEASE=v0.1.4 docker compose build --no-cache
RELEASE=v0.1.4 docker compose up -d --no-build
```

不写 `RELEASE` 时默认下载最新 Release。固定版本更容易确认当前运行的程序，也方便回滚。

构建日志中必须出现类似内容：

```text
zebra-store-linux-x86_64-musl.tar.gz: OK
```

这表示下载的压缩包已经通过 Release 中 `SHA256SUMS` 的校验。

## 4. 检查容器

```bash
docker compose ps
docker compose logs --tail=100 store
curl -fsS http://127.0.0.1:18180/health
curl -fsS http://127.0.0.1:18180/api/v1/public/config | head -c 200
```

正常情况下只有一个业务容器：

```text
zebra-store-app   zebra-store/release:v0.1.4   Up ... (healthy)
```

数据库与上传文件分别保存在 `zebra-store_store_data` 和 `zebra-store_store_uploads` 卷中。

## 5. 获取或重置后台密码

后台地址是 `https://你的域名/admin/`。默认用户名是 `admin`，首次密码就是启动前写入
`config.yml` 的 `bootstrap.default_admin_password`：

```bash
grep -E 'default_admin_(username|password)' config.yml
```

程序只在数据库第一次初始化时用这两项创建管理员。后台修改密码后，数据库只保存密码哈希，不能读取出原密码；
此时 `config.yml` 里的值也不会再代表当前密码。

忘记密码时，在 `deploy/release` 目录执行：

```bash
NEW_PASSWORD="Zs9-$(openssl rand -hex 10)"
docker compose exec store zebra-store --config /app/config.yml \
  admin reset-password --username admin --password "$NEW_PASSWORD"
printf '新后台密码：%s\n' "$NEW_PASSWORD"
```

命令成功后会撤销这个管理员已有的登录会话。请立即用新密码登录，再保存到密码管理器。

## 6. 配置 HTTPS

Caddy：

```caddyfile
shop.example.com {
	encode zstd gzip
	reverse_proxy 127.0.0.1:18180
}
```

```bash
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
```

Nginx 或宝塔反向代理的目标填写 `http://127.0.0.1:18180`，整个域名都转发过去，不需要分别配置
`/api/`、`/admin/` 或静态资源目录。

## 升级

刷新两个部署文件并指定新 Release。不要重新下载 `config.yml`，否则会覆盖站点密钥和管理员初始配置：

```bash
cd /opt/zebra-store
BASE=https://raw.githubusercontent.com/noxue/zebra-store/main
curl -fsSLO "$BASE/deploy/release/Dockerfile"
curl -fsSLo compose.yml "$BASE/deploy/release/compose.yml"
RELEASE=v新版本 docker compose build --no-cache
RELEASE=v新版本 docker compose up -d --no-build
curl -fsS http://127.0.0.1:18180/health
```

Compose 会复用原来的数据库卷、上传卷和 `config.yml`。确认新容器健康后，可以删除不用的旧镜像：

```bash
docker image prune -f
```

## 常用命令

```bash
docker compose ps
docker compose logs -f store
docker compose restart store
docker compose down
docker compose up -d --no-build
```

`docker compose down` 不会删除数据库和上传卷。不要执行 `docker compose down -v`，除非确定要删除全部站点数据。
