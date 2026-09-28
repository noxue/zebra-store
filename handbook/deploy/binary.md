# 单文件二进制部署（推荐）

这是最省事的部署方式。GitHub Release 中的 `zebra-store` 已经包含：

::: tip 服务器使用 Docker？
请直接使用 [Docker Release 单文件部署](./docker)。它下载同一个 Release 二进制并校验 SHA256，运行时也只有一个业务容器。
:::

- Rust 后端、API 和后台任务；
- 用户前台；
- `/admin/` 管理后台；
- SQLite 支持和首次启动配置生成器。

服务器不需要安装 Rust、Node.js，也不需要另外上传 `storefront/dist` 或 `admin/dist`。

## 支持的 Linux 版本

每个以 `v` 开头的 Git 标签都会自动构建并发布以下文件：

| 文件 | 适用系统 |
|---|---|
| `zebra-store-linux-x86_64-musl.tar.gz` | Intel / AMD 64 位 Linux，兼容范围最广，推荐 |
| `zebra-store-linux-aarch64-musl.tar.gz` | ARM64 Linux，兼容范围最广，推荐 |
| `zebra-store-linux-x86_64-gnu.tar.gz` | 使用 glibc 的 Intel / AMD 64 位 Linux |
| `zebra-store-linux-aarch64-gnu.tar.gz` | 使用 glibc 的 ARM64 Linux |

下载地址：[GitHub Releases](https://github.com/noxue/zebra-store/releases)。Release 同时提供
`SHA256SUMS`，安装脚本会自动校验下载文件。

## 一键安装

适用于使用 systemd 的 Debian、Ubuntu、CentOS、Rocky Linux 等发行版。用 root 执行：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

脚本会自动完成：

1. 判断服务器是 `x86_64` 还是 `arm64`；
2. 下载对应的静态 Linux 二进制并校验 SHA-256；
3. 安装到 `/usr/local/bin/zebra-store`；
4. 在 `/opt/zebra-store` 创建安全配置、SQLite 数据目录和上传目录；
5. 创建 `zebra-store` 系统用户和 systemd 服务；
6. 启动服务并检查 `http://127.0.0.1:8081/health`。

首次安装会在终端显示随机生成的管理员密码，**当场保存**。配置文件权限为 `600`，三个应用密钥和管理员密码均随机生成。

## 部署后查看或重置后台密码

后台地址是 `https://你的域名/admin/`，默认用户名是 `admin`。一键安装生成的初始密码会显示在终端，
也可以从配置文件查看：

```bash
sudo grep -E 'default_admin_(username|password)' /opt/zebra-store/config.yml
```

这只代表数据库首次初始化时使用的密码。用户在后台修改密码后，数据库只保存密码哈希，无法读取出当前密码，
配置文件中的旧值也不会把密码改回去。忘记密码时执行重置命令：

```bash
NEW_PASSWORD="Zs9-$(openssl rand -hex 10)"
sudo /usr/local/bin/zebra-store --config /opt/zebra-store/config.yml \
  admin reset-password --username admin --password "$NEW_PASSWORD"
printf '新后台密码：%s\n' "$NEW_PASSWORD"
```

重置会撤销这个管理员已有的登录会话。请立即用新密码登录并保存到密码管理器。

## 配置域名和 HTTPS

程序已经提供所有网页，Web 服务器只需把整个域名反向代理到 `127.0.0.1:8081`。

### Caddy

```caddyfile
shop.example.com {
	encode zstd gzip
	reverse_proxy 127.0.0.1:8081
}
```

```bash
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
```

Caddy 会自动申请和续期 HTTPS 证书。Cloudflare 小黄云可以保持开启；源站的 80、443 端口需要允许访问。

### Nginx / 宝塔

在对应站点的 `server { ... }` 中设置：

```nginx
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
```

不需要创建静态网站目录，也不需要分别配置 `/api/` 和 `/admin/`。

## 手动安装

不想运行安装脚本时，从 [Releases](https://github.com/noxue/zebra-store/releases) 下载对应压缩包：

```bash
mkdir -p /opt/zebra-store && cd /opt/zebra-store
tar xzf zebra-store-linux-x86_64-musl.tar.gz
chmod +x zebra-store
./zebra-store init       # 生成 config.yml，并显示初始管理员密码
./zebra-store serve      # 前台、后台和 API 都监听 8081
```

浏览器访问 `http://服务器IP:8081/` 是用户前台，`http://服务器IP:8081/admin/` 是管理后台。
生产环境请用上面的 Caddy 或 Nginx 配置 HTTPS，不要直接暴露 8081。

## 升级

先按 [备份与升级](./backup-upgrade) 备份，再次运行一键安装命令即可。脚本保留
`/opt/zebra-store/config.yml`、数据库和上传文件，只替换二进制并重启服务。

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

检查状态：

```bash
systemctl status zebra-store --no-pager
journalctl -u zebra-store -n 100 --no-pager
curl -s http://127.0.0.1:8081/health
```

## 自己发布版本

仓库的 `.github/workflows/release.yml` 监听 `v*` 标签：

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions 会构建两套前端，把静态文件嵌入 Rust 二进制，交叉编译四个 Linux 目标，并把压缩包与校验文件添加到对应 Release。

## Docker 中运行 Release 单文件

仓库的 `deploy/release/` 提供了另一种安装方式：Docker 构建阶段直接从 GitHub Release 下载并校验
musl 单文件，运行镜像不编译源码，也不包含 Node.js 或 Rust。

```bash
git clone https://github.com/noxue/zebra-store.git
cd zebra-store/deploy/release
cp ../../backend/config.example.yml config.yml
# 编辑 config.yml，设置三个 secret 和初始管理员密码
docker compose build --no-cache
docker compose up -d
curl http://127.0.0.1:18180/health
```

ARM64 服务器执行 `TARGET=aarch64 docker compose build --no-cache`。需要固定版本时使用
`RELEASE=v0.1.4 docker compose build --no-cache`。Caddy 或 Nginx 把整个域名反向代理到
`127.0.0.1:18180` 即可。
