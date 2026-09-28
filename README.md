# Zebra Store

[简体中文](README.md) | [English](README.en.md)

Zebra Store 是一个可自行部署的数字商品商城，后端使用 **Rust（axum + SeaORM）**，用户前台和管理后台使用 **Vue 3 + TSX**。

- 项目地址：[github.com/noxue/zebra-store](https://github.com/noxue/zebra-store)
- 使用文档：[zebra-store-docs.noxue.com](https://zebra-store-docs.noxue.com)
- 版本下载：[GitHub Releases](https://github.com/noxue/zebra-store/releases)

## 推荐：Release 单文件部署

Linux Release 是一个已经内嵌用户前台、管理后台、API 和后台任务的可执行文件。无需安装 Node.js、Rust，也无需上传 `dist` 或克隆整个仓库。

在使用 systemd 的 x86_64 或 aarch64 Linux 服务器上，以 root 身份执行：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

安装脚本只下载当前架构对应的 Release 压缩包和 `SHA256SUMS`，校验后安装程序、生成配置并创建 systemd 服务。安装完成时会显示后台初始账号和随机密码。

随后将 Caddy、Nginx 或宝塔 Nginx 的整个域名反向代理到：

```text
http://127.0.0.1:8081
```

Web 服务器只负责 HTTPS 和反向代理，不需要分别配置前台、后台或 API 路径。完整步骤见[部署手册](https://zebra-store-docs.noxue.com/deploy/)。

如果使用 Docker，请参考 [Docker Release 单文件部署](https://zebra-store-docs.noxue.com/deploy/docker)。该方式只下载 Dockerfile、Compose 配置、示例配置和 Release 单文件。

## 自动发布

推送以 `v` 开头的 Git 标签后，GitHub Actions 会自动构建并发布：

- x86_64 Linux musl
- aarch64 Linux musl
- x86_64 Linux GNU
- aarch64 Linux GNU

每个版本同时发布 `SHA256SUMS`，下载地址见 [GitHub Releases](https://github.com/noxue/zebra-store/releases)。

## 项目组成

| 项目 | 目录 | 技术栈 | 开发端口 |
|---|---|---|---|
| 后端 API | `backend/` | Rust 2024、axum 0.8、SeaORM 2.0 | 8081 |
| 用户前台 | `storefront/` | Vue 3 + TSX、Vite、Pinia、vue-i18n、Tailwind CSS v4 | 5185 |
| 管理后台 | `admin/` | Vue 3 + TSX、Vite、Pinia、vue-i18n、Tailwind CSS v4 | 5186 |
| 用户手册 | `handbook/` | VitePress | 5190 |

API 使用统一的 `{status_code, msg, data, pagination}` 返回结构，金额使用类似 `"12.30"` 的字符串表示。供应商适配器用于对接受支持的第三方站点。

## 开发环境快速启动

需要 Rust 1.90 或更高版本、Node.js 20 或更高版本。

```bash
# 后端
cd backend
cp config.example.yml config.yml      # 修改三个密钥和管理员密码
cargo run -p zs-server                # http://localhost:8081

# 用户前台
cd ../storefront
npm install
VITE_API_TARGET=http://localhost:8081 npm run dev     # http://localhost:5185

# 管理后台
cd ../admin
npm install
VITE_API_TARGET=http://localhost:8081 npm run dev     # http://localhost:5186
```

首次启动时，服务会创建数据表、六个内置 RBAC 角色，以及配置中指定的超级管理员。

## 数据库

默认使用 SQLite，也支持 MySQL 8+ 和 PostgreSQL 14+。切换数据库只需修改 URL：

```yaml
database:
  url: sqlite://data/zebra.db?mode=rwc
  # url: mysql://user:pass@127.0.0.1:3306/zebra
  # url: postgres://user:pass@127.0.0.1:5432/zebra
```

也可以使用环境变量，例如 `ZS__DATABASE__URL=postgres://…`。程序启动时会自动创建或补齐表结构；数据库本身需要提前创建，连接用户需要 DDL 权限。

## 配置

[`backend/config.example.yml`](backend/config.example.yml) 记录了全部配置项。配置可以用 `ZS__SECTION__KEY` 形式的环境变量覆盖，例如 `ZS__SERVER__PORT=9000`。

`app.secret_key`、`jwt.secret` 和 `user_jwt.secret` 必须互不相同，且至少包含 16 个字符。站点名称、Logo、图标、主题颜色和图片可以在管理后台修改。Redis 是可选组件。

常用管理员命令：

```bash
zebra-store admin list-admins
zebra-store admin reset-password --username admin --password '新密码'
zebra-store admin reset-2fa --username admin
```

## 从源码构建

源码构建主要用于开发。需要先生成两个前端产物，Rust 构建脚本会将它们嵌入最终的 `zebra-store`：

```bash
cd storefront && npm ci && npm run build
cd ../admin && npm ci && npm run build
cd ../backend && cargo build --release -p zs-server
```

最终仍然只需运行 `backend/target/release/zebra-store`，无需单独部署前端目录。

## 测试

```bash
cd backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd ../storefront && npm run typecheck && npm run lint && npm run test && npm run build
cd ../admin && npm run typecheck && npm run lint && npm run test && npm run build
```

## 文档与架构

中文用户手册源码位于 [`handbook/`](handbook/)。工程资料见 [`CLAUDE.md`](CLAUDE.md)、[`docs/PLAN.md`](docs/PLAN.md)、[`docs/TODO.md`](docs/TODO.md)、[`docs/BACKEND_GUIDE.md`](docs/BACKEND_GUIDE.md)、[`docs/DESIGN.md`](docs/DESIGN.md) 和 [`docs/reference/`](docs/reference/)。

```text
zs-shared ← zs-domain ← zs-app ← zs-infra ← zs-api ← zs-server
                                  zs-migration ↗
```

领域模型和业务规则位于 `zs-domain`，用例位于 `zs-app`，数据库、网关、邮件和任务队列位于 `zs-infra`，HTTP 接口位于 `zs-api`。
