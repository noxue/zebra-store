# Zebra Store

[简体中文](README.md) | [English](README.en.md)

Zebra Store 是一个可自行部署的数字商品商城，后端使用 **Rust（axum + SeaORM）**，用户前台和管理后台使用 **Vue 3 + TSX**。

- 项目地址：[github.com/noxue/zebra-store](https://github.com/noxue/zebra-store)
- 使用文档：[zebra-store-docs.noxue.com](https://zebra-store-docs.noxue.com)
- 版本下载：[GitHub Releases](https://github.com/noxue/zebra-store/releases)
- 汇付支付 SDK：[huifu-pay](https://github.com/noxue/huifu-pay)（crates.io：[`huifu-pay`](https://crates.io/crates/huifu-pay)）

Zebra Store 支持通过汇付进行支付宝、微信 H5 和 PC 收款，包括支付下单、RSA 验签回调、主动查单、原路退款和交易对账单下载。默认在 PC 支付页显示完整托管网页的二维码、移动端调起支付，也可切换为整页跳转。配置方法见[汇付支付文档](https://zebra-store-docs.noxue.com/payment/huifu)。

游客下单的邮箱和查询密码可以同时留空；商城会在浏览器 `localStorage` 中保存 UUID 身份，刷新或重新打开页面后仍能查询订单。填写邮箱和密码时也会自动保存并回填，便于继续查单。

## 界面截图

### 用户前台

|  |  |
|---|---|
| <a href="handbook/public/screenshots/storefront/home.png"><img src="handbook/public/screenshots/storefront/home.png" width="440" alt="商城首页"></a><br>商城首页 | <a href="handbook/public/screenshots/storefront/product-detail.png"><img src="handbook/public/screenshots/storefront/product-detail.png" width="440" alt="商品详情"></a><br>商品详情 |

<details>
<summary>查看全部前台页面截图（22 张）</summary>

|  |  |
|---|---|
| <a href="handbook/public/screenshots/storefront/affiliate.png"><img src="handbook/public/screenshots/storefront/affiliate.png" width="440" alt="推广中心"></a><br>推广中心 | <a href="handbook/public/screenshots/storefront/announcement.png"><img src="handbook/public/screenshots/storefront/announcement.png" width="440" alt="公告详情"></a><br>公告详情 |
| <a href="handbook/public/screenshots/storefront/cart.png"><img src="handbook/public/screenshots/storefront/cart.png" width="440" alt="购物车"></a><br>购物车 | <a href="handbook/public/screenshots/storefront/checkout.png"><img src="handbook/public/screenshots/storefront/checkout.png" width="440" alt="确认订单"></a><br>确认订单 |
| <a href="handbook/public/screenshots/storefront/gift-card.png"><img src="handbook/public/screenshots/storefront/gift-card.png" width="440" alt="礼品卡"></a><br>礼品卡 | <a href="handbook/public/screenshots/storefront/guest-orders.png"><img src="handbook/public/screenshots/storefront/guest-orders.png" width="440" alt="游客订单查询"></a><br>游客订单查询 |
| <a href="handbook/public/screenshots/storefront/home-dark.png"><img src="handbook/public/screenshots/storefront/home-dark.png" width="440" alt="深色首页"></a><br>深色首页 | <a href="handbook/public/screenshots/storefront/home-mobile.png"><img src="handbook/public/screenshots/storefront/home-mobile.png" width="440" alt="移动端首页"></a><br>移动端首页 |
| <a href="handbook/public/screenshots/storefront/home.png"><img src="handbook/public/screenshots/storefront/home.png" width="440" alt="商城首页"></a><br>商城首页 | <a href="handbook/public/screenshots/storefront/login.png"><img src="handbook/public/screenshots/storefront/login.png" width="440" alt="用户登录"></a><br>用户登录 |
| <a href="handbook/public/screenshots/storefront/me-api-compat.png"><img src="handbook/public/screenshots/storefront/me-api-compat.png" width="440" alt="兼容 API 设置"></a><br>兼容 API 设置 | <a href="handbook/public/screenshots/storefront/me-api.png"><img src="handbook/public/screenshots/storefront/me-api.png" width="440" alt="API 设置"></a><br>API 设置 |
| <a href="handbook/public/screenshots/storefront/me-orders.png"><img src="handbook/public/screenshots/storefront/me-orders.png" width="440" alt="我的订单"></a><br>我的订单 | <a href="handbook/public/screenshots/storefront/member-level.png"><img src="handbook/public/screenshots/storefront/member-level.png" width="440" alt="会员等级"></a><br>会员等级 |
| <a href="handbook/public/screenshots/storefront/order-detail.png"><img src="handbook/public/screenshots/storefront/order-detail.png" width="440" alt="订单详情"></a><br>订单详情 | <a href="handbook/public/screenshots/storefront/order-refunds.png"><img src="handbook/public/screenshots/storefront/order-refunds.png" width="440" alt="订单退款"></a><br>订单退款 |
| <a href="handbook/public/screenshots/storefront/payment.png"><img src="handbook/public/screenshots/storefront/payment.png" width="440" alt="订单支付"></a><br>订单支付 | <a href="handbook/public/screenshots/storefront/product-detail.png"><img src="handbook/public/screenshots/storefront/product-detail.png" width="440" alt="商品详情"></a><br>商品详情 |
| <a href="handbook/public/screenshots/storefront/products.png"><img src="handbook/public/screenshots/storefront/products.png" width="440" alt="商品列表"></a><br>商品列表 | <a href="handbook/public/screenshots/storefront/register.png"><img src="handbook/public/screenshots/storefront/register.png" width="440" alt="用户注册"></a><br>用户注册 |
| <a href="handbook/public/screenshots/storefront/security-2fa.png"><img src="handbook/public/screenshots/storefront/security-2fa.png" width="440" alt="两步验证"></a><br>两步验证 | <a href="handbook/public/screenshots/storefront/wallet.png"><img src="handbook/public/screenshots/storefront/wallet.png" width="440" alt="用户钱包"></a><br>用户钱包 |

</details>

### 管理后台

|  |  |
|---|---|
| <a href="handbook/public/screenshots/admin/dashboard.png"><img src="handbook/public/screenshots/admin/dashboard.png" width="440" alt="仪表盘"></a><br>仪表盘 | <a href="handbook/public/screenshots/admin/products.png"><img src="handbook/public/screenshots/admin/products.png" width="440" alt="商品管理"></a><br>商品管理 |

<details>
<summary>查看全部管理后台页面截图（47 张）</summary>

|  |  |
|---|---|
| <a href="handbook/public/screenshots/admin/affiliate-settings.png"><img src="handbook/public/screenshots/admin/affiliate-settings.png" width="440" alt="分销设置"></a><br>分销设置 | <a href="handbook/public/screenshots/admin/affiliate-withdraws.png"><img src="handbook/public/screenshots/admin/affiliate-withdraws.png" width="440" alt="分销提现"></a><br>分销提现 |
| <a href="handbook/public/screenshots/admin/api-credentials.png"><img src="handbook/public/screenshots/admin/api-credentials.png" width="440" alt="API 凭证"></a><br>API 凭证 | <a href="handbook/public/screenshots/admin/authz-audit.png"><img src="handbook/public/screenshots/admin/authz-audit.png" width="440" alt="权限审计"></a><br>权限审计 |
| <a href="handbook/public/screenshots/admin/authz.png"><img src="handbook/public/screenshots/admin/authz.png" width="440" alt="权限管理"></a><br>权限管理 | <a href="handbook/public/screenshots/admin/banners.png"><img src="handbook/public/screenshots/admin/banners.png" width="440" alt="轮播图管理"></a><br>轮播图管理 |
| <a href="handbook/public/screenshots/admin/card-secret-import.png"><img src="handbook/public/screenshots/admin/card-secret-import.png" width="440" alt="导入卡密"></a><br>导入卡密 | <a href="handbook/public/screenshots/admin/card-secrets.png"><img src="handbook/public/screenshots/admin/card-secrets.png" width="440" alt="卡密管理"></a><br>卡密管理 |
| <a href="handbook/public/screenshots/admin/categories.png"><img src="handbook/public/screenshots/admin/categories.png" width="440" alt="分类管理"></a><br>分类管理 | <a href="handbook/public/screenshots/admin/compliance.png"><img src="handbook/public/screenshots/admin/compliance.png" width="440" alt="合规设置"></a><br>合规设置 |
| <a href="handbook/public/screenshots/admin/coupons.png"><img src="handbook/public/screenshots/admin/coupons.png" width="440" alt="优惠券"></a><br>优惠券 | <a href="handbook/public/screenshots/admin/dashboard.png"><img src="handbook/public/screenshots/admin/dashboard.png" width="440" alt="仪表盘"></a><br>仪表盘 |
| <a href="handbook/public/screenshots/admin/gift-cards.png"><img src="handbook/public/screenshots/admin/gift-cards.png" width="440" alt="礼品卡管理"></a><br>礼品卡管理 | <a href="handbook/public/screenshots/admin/login.png"><img src="handbook/public/screenshots/admin/login.png" width="440" alt="管理员登录"></a><br>管理员登录 |
| <a href="handbook/public/screenshots/admin/media.png"><img src="handbook/public/screenshots/admin/media.png" width="440" alt="媒体库"></a><br>媒体库 | <a href="handbook/public/screenshots/admin/member-levels.png"><img src="handbook/public/screenshots/admin/member-levels.png" width="440" alt="会员等级管理"></a><br>会员等级管理 |
| <a href="handbook/public/screenshots/admin/notifications.png"><img src="handbook/public/screenshots/admin/notifications.png" width="440" alt="通知管理"></a><br>通知管理 | <a href="handbook/public/screenshots/admin/order-detail.png"><img src="handbook/public/screenshots/admin/order-detail.png" width="440" alt="订单详情"></a><br>订单详情 |
| <a href="handbook/public/screenshots/admin/order-refunds.png"><img src="handbook/public/screenshots/admin/order-refunds.png" width="440" alt="订单退款"></a><br>订单退款 | <a href="handbook/public/screenshots/admin/orders.png"><img src="handbook/public/screenshots/admin/orders.png" width="440" alt="订单管理"></a><br>订单管理 |
| <a href="handbook/public/screenshots/admin/payment-channel-edit.png"><img src="handbook/public/screenshots/admin/payment-channel-edit.png" width="440" alt="编辑支付渠道"></a><br>编辑支付渠道 | <a href="handbook/public/screenshots/admin/payment-channels.png"><img src="handbook/public/screenshots/admin/payment-channels.png" width="440" alt="支付渠道"></a><br>支付渠道 |
| <a href="handbook/public/screenshots/admin/payments.png"><img src="handbook/public/screenshots/admin/payments.png" width="440" alt="支付记录"></a><br>支付记录 | <a href="handbook/public/screenshots/admin/posts.png"><img src="handbook/public/screenshots/admin/posts.png" width="440" alt="文章管理"></a><br>文章管理 |
| <a href="handbook/public/screenshots/admin/procurement-orders.png"><img src="handbook/public/screenshots/admin/procurement-orders.png" width="440" alt="采购订单"></a><br>采购订单 | <a href="handbook/public/screenshots/admin/product-edit.png"><img src="handbook/public/screenshots/admin/product-edit.png" width="440" alt="编辑商品"></a><br>编辑商品 |
| <a href="handbook/public/screenshots/admin/product-mappings.png"><img src="handbook/public/screenshots/admin/product-mappings.png" width="440" alt="商品映射"></a><br>商品映射 | <a href="handbook/public/screenshots/admin/products.png"><img src="handbook/public/screenshots/admin/products.png" width="440" alt="商品管理"></a><br>商品管理 |
| <a href="handbook/public/screenshots/admin/promotions.png"><img src="handbook/public/screenshots/admin/promotions.png" width="440" alt="促销活动"></a><br>促销活动 | <a href="handbook/public/screenshots/admin/reconciliation.png"><img src="handbook/public/screenshots/admin/reconciliation.png" width="440" alt="支付对账"></a><br>支付对账 |
| <a href="handbook/public/screenshots/admin/reseller-operations.png"><img src="handbook/public/screenshots/admin/reseller-operations.png" width="440" alt="分站运营"></a><br>分站运营 | <a href="handbook/public/screenshots/admin/reseller-profiles.png"><img src="handbook/public/screenshots/admin/reseller-profiles.png" width="440" alt="分站资料"></a><br>分站资料 |
| <a href="handbook/public/screenshots/admin/reseller-site-configs.png"><img src="handbook/public/screenshots/admin/reseller-site-configs.png" width="440" alt="分站配置"></a><br>分站配置 | <a href="handbook/public/screenshots/admin/reseller-withdraws.png"><img src="handbook/public/screenshots/admin/reseller-withdraws.png" width="440" alt="分站提现"></a><br>分站提现 |
| <a href="handbook/public/screenshots/admin/risk-control.png"><img src="handbook/public/screenshots/admin/risk-control.png" width="440" alt="风险控制"></a><br>风险控制 | <a href="handbook/public/screenshots/admin/security.png"><img src="handbook/public/screenshots/admin/security.png" width="440" alt="安全设置"></a><br>安全设置 |
| <a href="handbook/public/screenshots/admin/settings-basic.png"><img src="handbook/public/screenshots/admin/settings-basic.png" width="440" alt="基础设置"></a><br>基础设置 | <a href="handbook/public/screenshots/admin/settings-theme.png"><img src="handbook/public/screenshots/admin/settings-theme.png" width="440" alt="主题设置"></a><br>主题设置 |
| <a href="handbook/public/screenshots/admin/site-connection-edit.png"><img src="handbook/public/screenshots/admin/site-connection-edit.png" width="440" alt="编辑站点连接"></a><br>编辑站点连接 | <a href="handbook/public/screenshots/admin/site-connections.png"><img src="handbook/public/screenshots/admin/site-connections.png" width="440" alt="站点连接"></a><br>站点连接 |
| <a href="handbook/public/screenshots/admin/telegram-bot.png"><img src="handbook/public/screenshots/admin/telegram-bot.png" width="440" alt="Telegram 机器人"></a><br>Telegram 机器人 | <a href="handbook/public/screenshots/admin/user-detail.png"><img src="handbook/public/screenshots/admin/user-detail.png" width="440" alt="用户详情"></a><br>用户详情 |
| <a href="handbook/public/screenshots/admin/user-login-logs.png"><img src="handbook/public/screenshots/admin/user-login-logs.png" width="440" alt="登录日志"></a><br>登录日志 | <a href="handbook/public/screenshots/admin/users.png"><img src="handbook/public/screenshots/admin/users.png" width="440" alt="用户管理"></a><br>用户管理 |
| <a href="handbook/public/screenshots/admin/wallet-config.png"><img src="handbook/public/screenshots/admin/wallet-config.png" width="440" alt="钱包设置"></a><br>钱包设置 | <a href="handbook/public/screenshots/admin/wallet-recharges.png"><img src="handbook/public/screenshots/admin/wallet-recharges.png" width="440" alt="钱包充值"></a><br>钱包充值 |
| <a href="handbook/public/screenshots/admin/wholesale.png"><img src="handbook/public/screenshots/admin/wholesale.png" width="440" alt="批发设置"></a><br>批发设置 |  |

</details>

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
