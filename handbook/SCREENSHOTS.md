# 手册截图清单

手册里所有 `![说明](/screenshots/…)` 图片都列在这里。应用界面的 89 张图片已经从完成验收的
dot2.com 实验环境和全新本地实例中整理到 `handbook/public/screenshots/`；重新构建即可发布。
剩余 6 张宝塔面板图片必须在单独安装的宝塔测试服务器上采集，当前正文不引用它们，避免线上出现损坏图片或用模拟图误导读者。

## 约定

- 环境：dot2.com 实验环境（`lab.sh provision` 之后的数据）；账号密码在服务器上执行 `/opt/zebra-lab/lab.sh creds` 查看；
- `S` = `https://store.dot2.com`，后台 = `https://store.dot2.com/admin`；
- 默认界面语言简体中文、亮色主题，除非另有说明；视口见最后一列，“整页”表示 fullPage 截图，“区域”表示只截对应元素；
- 格式 PNG；截图前**对密钥、连接码、对接密钥、真实邮箱打码**；不要截到浏览器地址栏以外的个人信息；
- “不保存 / 不提交”的步骤截完图后取消，避免污染实验数据；会改动数据的步骤截完后执行 `lab.sh provision` 恢复；
- 部署（宝塔）截图需要一台单独装了宝塔面板的测试服务器，不在 dot2.com 上。

共 **95** 张：**89 张已完成，6 张宝塔面板外部环境截图待采集**。

## 用户前台（22）

| 文件 | 说明 | 页面 / URL | 截图前的准备 | 账号 | 视口 | 用在 |
|---|---|---|---|---|---|---|
| `storefront/home.png` | 斑马小铺前台首页 | S/ | 首页有 Banner、推荐商品；关闭公告弹窗（先点“今日不再显示”） | 访客（未登录） | 1440×900 亮色 | guide/index.md, storefront/index.md |
| `storefront/home-mobile.png` | 手机端首页 | S/ | 同上 | 访客 | 390×844（iPhone 14） | storefront/index.md |
| `storefront/home-dark.png` | 暗色主题 | S/ | 点导航栏切换到暗色主题 | 访客 | 1440×900 暗色 | storefront/browse.md |
| `storefront/announcement.png` | 首页公告弹窗 | S/ | 后台系统设置→首页公告 开启并填标题内容；清除 localStorage `announcement_dismiss` 后刷新 | 访客 | 1440×900 | storefront/browse.md |
| `storefront/products.png` | 商品列表 | S/products | 选中一个有子分类的分类 | 访客 | 1440×900 | storefront/browse.md |
| `storefront/product-detail.png` | 商品详情页 | S/products/{多规格商品 slug} | 选择一个有活动价或批发价的多规格商品，选中一个 SKU | buyer@lab.test | 1440×900 | storefront/browse.md |
| `storefront/login.png` | 登录页 | S/auth/login | 默认状态 | 访客 | 1440×900 | storefront/account.md |
| `storefront/register.png` | 注册页 | S/auth/register | 默认状态（实验环境关闭了邮箱验证码；如需展示验证码按钮，先在 后台开启邮箱验证） | 访客 | 1440×900 | storefront/account.md |
| `storefront/security-2fa.png` | 开启两步验证 | S/me/security | 点“开启两步验证”，停在显示二维码的一步（用测试账号，截图后取消） | 新注册的测试账号 | 1440×900 | storefront/account.md |
| `storefront/cart.png` | 购物车 | S/cart | 购物车里加入 2 种商品 | 访客 | 1440×900 | storefront/checkout.md |
| `storefront/checkout.png` | 结算页 | S/checkout | 购物车结算；已登录并勾选“使用余额”；输入一个有效优惠券 | buyer@lab.test | 1440×1200 整页 | storefront/checkout.md |
| `storefront/member-level.png` | 会员等级与升级进度 | S/me | 个人中心概览，展示会员等级卡和升级进度 | buyer@lab.test | 1440×900 | storefront/discounts.md |
| `storefront/payment.png` | 支付页 | S/pay?order_no={待支付订单} | 创建一个待支付订单（需要一个 epay 测试渠道），停在选择渠道/显示二维码的状态 | buyer@lab.test | 1440×900 | storefront/payment.md |
| `storefront/me-orders.png` | 我的订单 | S/me/orders | 有若干不同状态的订单 | buyer@lab.test | 1440×900 | storefront/orders.md |
| `storefront/guest-orders.png` | 游客查单 | S/guest/orders | 用一个游客订单的邮箱和查询密码查询后 | 访客 | 1440×900 | storefront/orders.md |
| `storefront/order-detail.png` | 订单详情与卡密 | S/orders/{已交付的 lab-e2e-card 订单} | 自动发货订单，交付区显示卡密 ZS-LAB-… | buyer@lab.test | 1440×1200 整页 | storefront/orders.md |
| `storefront/order-refunds.png` | 订单详情里的退款记录 | S/orders/{部分退款订单} | 后台对该订单“退到余额”5.00 后，截退款记录区域 | buyer@lab.test | 1440×900 区域 | storefront/after-sales.md |
| `storefront/wallet.png` | 钱包页面 | S/me/wallet | 有充值和消费流水 | buyer@lab.test | 1440×900 | storefront/wallet.md |
| `storefront/gift-card.png` | 兑换礼品卡 | S/me/gift-cards | 后台生成一张礼品卡，兑换成功后的结果卡片 | buyer@lab.test | 1440×900 | storefront/wallet.md |
| `storefront/affiliate.png` | 推广面板 | S/me/affiliate | 后台开启返利；该账号已开通推广 | buyer@lab.test | 1440×900 | storefront/affiliate.md |
| `storefront/me-api.png` | API 对接页面 | S/me/api | 凭证已审核的账号，显示 Key、连接码区域 | zs2-supply@lab.test | 1440×1200 整页 | storefront/api.md |
| `storefront/me-api-compat.png` | 兼容密钥 | S/me/api | “异次元 / 萌次元 对接”区域，已生成对接密钥（截图前对密钥打码） | acg-down@lab.test | 1440×900 区域 | integration/acg-faka.md, storefront/api.md |

## 管理后台（47）

| 文件 | 说明 | 页面 / URL | 截图前的准备 | 账号 | 视口 | 用在 |
|---|---|---|---|---|---|---|
| `admin/login.png` | 后台登录页 | 后台/login | 默认状态 | —（未登录） | 1440×900 | admin/index.md |
| `admin/dashboard.png` | 仪表盘 | 后台/ | 范围选 7 天，有订单数据 | admin | 1440×900 | admin/dashboard.md, admin/index.md |
| `admin/compliance.png` | 合规确认对话框 | 后台/payment-channels | **全新实例**（合规未确认）首次打开时的确认对话框；dot2.com 已确认，需在本地 fresh DB 上截 | admin（本地新库） | 1440×900 | admin/index.md |
| `admin/settings-basic.png` | 站点设置 | 后台/settings | 基础配置页签 | admin | 1440×1200 整页 | admin/settings.md |
| `admin/settings-theme.png` | 主题外观设置 | 后台/settings?tab=template | 模板配置 → 主题外观，含右侧预览 | admin | 1440×1200 整页 | admin/theme.md |
| `admin/authz.png` | 权限管理 | 后台/authz | 选中角色 operations，显示其权限和权限目录 | admin | 1440×900 | admin/rbac.md |
| `admin/authz-audit.png` | 权限审计日志 | 后台/authz-audit-logs | 至少有一次角色授权操作 | admin | 1440×900 | admin/rbac.md |
| `admin/security.png` | 管理员安全设置 | 后台/security | 默认状态 | admin | 1440×900 | admin/security.md |
| `admin/notifications.png` | 通知中心 | 后台/settings/notifications | 默认状态 | admin | 1440×1200 整页 | admin/notifications.md |
| `admin/telegram-bot.png` | Telegram Bot 概览 | 后台/telegram-bot | 默认状态 | admin | 1440×900 | admin/telegram.md |
| `admin/categories.png` | 商品分类 | 后台/categories | 有父子分类 | admin | 1440×900 | admin/products.md |
| `admin/products.png` | 商品列表 | 后台/products | 默认列表 | admin | 1440×900 | admin/products.md |
| `admin/product-edit.png` | 商品编辑弹窗 | 后台/products?product_id={lab-e2e-card 的 id} | 打开编辑弹窗，滚动到 SKU 区域 | admin | 1440×1000 | admin/products.md |
| `admin/card-secret-import.png` | 卡密导入 | 后台/card-secret-imports | 选中 lab-e2e-card，在粘贴框里填几行示例卡密（不要提交） | admin | 1440×900 | admin/card-secrets.md |
| `admin/card-secrets.png` | 卡密库存 | 后台/card-secrets | 选中 lab-e2e-card，显示统计和批次 | admin | 1440×900 | admin/card-secrets.md |
| `admin/orders.png` | 订单列表 | 后台/orders | 默认列表 | admin | 1440×900 | admin/orders.md |
| `admin/order-detail.png` | 订单详情 | 后台/orders | 打开一个已交付订单的详情对话框 | admin | 1440×1000 | admin/orders.md |
| `admin/order-refunds.png` | 退款记录 | 后台/order-refunds | 至少一条退款记录 | admin | 1440×900 | admin/orders.md |
| `admin/risk-control.png` | 订单风控 | 后台/order-risk-control | 默认状态 | admin | 1440×1200 整页 | admin/risk-control.md |
| `admin/payment-channels.png` | 支付渠道列表 | 后台/payment-channels | 至少一个渠道（可建一个未启用的 epay 示例渠道） | admin | 1440×900 | admin/payments.md |
| `admin/payment-channel-edit.png` | 新建支付渠道 | 后台/payment-channels | 打开“新建渠道”，提供方选易支付，显示专属字段（不保存） | admin | 1440×1000 | admin/payments.md |
| `admin/payments.png` | 支付记录 | 后台/payments | 默认列表 | admin | 1440×900 | admin/payments.md |
| `admin/users.png` | 用户列表 | 后台/users | 默认列表 | admin | 1440×900 | admin/users.md |
| `admin/user-detail.png` | 用户详情 | 后台/users/{buyer 的 id} | buyer 的详情页，钱包卡片可见 | admin | 1440×1000 | admin/users.md |
| `admin/user-login-logs.png` | 用户登录日志 | 后台/user-login-logs | 默认列表 | admin | 1440×900 | admin/users.md |
| `admin/wallet-recharges.png` | 充值记录 | 后台/wallet-recharges | 默认列表 | admin | 1440×900 | admin/wallet.md |
| `admin/wallet-config.png` | 钱包配置 | 后台/wallet-config | 默认状态 | admin | 1440×900 | admin/wallet.md |
| `admin/member-levels.png` | 会员等级 | 后台/member-levels | 至少 2 个等级 | admin | 1440×900 | admin/member-levels.md |
| `admin/coupons.png` | 优惠券 | 后台/coupons | 至少一张券 | admin | 1440×900 | admin/marketing.md |
| `admin/promotions.png` | 活动价 | 后台/promotions | 至少一个活动 | admin | 1440×900 | admin/marketing.md |
| `admin/wholesale.png` | 批发价配置 | 后台/wholesale-prices | 打开一个商品的批发价配置弹窗，含 2 个阶梯 | admin | 1440×900 | admin/marketing.md |
| `admin/gift-cards.png` | 礼品卡 | 后台/gift-cards | 已生成若干礼品卡 | admin | 1440×900 | admin/marketing.md |
| `admin/posts.png` | 文章编辑 | 后台/posts/blog | 打开一篇文章的编辑弹窗 | admin | 1440×1000 | admin/content.md |
| `admin/banners.png` | Banner 管理 | 后台/banners | 至少一个 Banner | admin | 1440×900 | admin/content.md |
| `admin/media.png` | 素材库 | 后台/media | 有若干图片 | admin | 1440×900 | admin/content.md |
| `admin/affiliate-settings.png` | 返利设置 | 后台/affiliates/settings | 默认状态 | admin | 1440×900 | admin/affiliate.md |
| `admin/affiliate-withdraws.png` | 提现审核 | 后台/affiliates/withdraws | 至少一条提现申请 | admin | 1440×900 | admin/affiliate.md |
| `admin/site-connections.png` | 连接管理 | 后台/site-connections | 显示 acg-faka、dujiao-next 两个连接 | admin | 1440×900 | admin/integration.md |
| `admin/site-connection-edit.png` | 新建连接 | 后台/site-connections | 打开“新建连接”，协议下拉展开或选中 Zebra Store 显示连接码框（不保存） | admin | 1440×1000 | admin/integration.md |
| `admin/product-mappings.png` | 商品映射 | 后台/product-mappings | 展开一行显示 SKU 对比 | admin | 1440×900 | admin/integration.md |
| `admin/procurement-orders.png` | 采购单管理 | 后台/procurement-orders | 有 fulfilled 的采购单 | admin | 1440×900 | admin/integration.md |
| `admin/reconciliation.png` | 对账中心 | 后台/reconciliation | 运行过一次对账任务 | admin | 1440×900 | admin/integration.md |
| `admin/api-credentials.png` | API 凭证审核 | 后台/api-credentials | 显示 zs2-supply、acg-down 等已审核凭证（Key 打码） | admin | 1440×900 | admin/integration.md |
| `admin/reseller-operations.png` | 分销概览 | 后台/resellers/operations | 范围 30 天 | admin | 1440×1200 整页 | admin/resellers.md |
| `admin/reseller-profiles.png` | 分销商审核 | 后台/resellers/profiles | 三个分销商均为启用 | admin | 1440×900 | admin/resellers.md |
| `admin/reseller-site-configs.png` | 分销站点配置 | 后台/resellers/site-configs | 列表显示三个分站的名称和 Logo | admin | 1440×900 | admin/resellers.md |
| `admin/reseller-withdraws.png` | 分销提现 | 后台/resellers/withdraws | 有已打款记录 | admin | 1440×900 | admin/resellers.md |

## 部署（宝塔面板）（6）

| 文件 | 说明 | 页面 / URL | 截图前的准备 | 账号 | 视口 | 用在 |
|---|---|---|---|---|---|---|
| `deploy/bt-install-nginx.png` | 宝塔软件商店安装 Nginx | 宝塔面板 → 软件商店 | 在一台装了宝塔的测试服务器上，搜索 Nginx 的安装界面 | 宝塔面板账号 | 1440×900 | deploy/bt-panel.md |
| `deploy/bt-add-site.png` | 宝塔添加站点 | 宝塔面板 → 网站 → 添加站点 | 填好域名，PHP 版本选“纯静态”（不提交） | 宝塔面板账号 | 1440×900 | deploy/bt-panel.md |
| `deploy/bt-site-files.png` | 上传后的网站目录 | 宝塔面板 → 文件 → /www/wwwroot/{域名} | 上传 storefront/dist 内容和 admin/ 目录后的文件列表 | 宝塔面板账号 | 1440×900 | deploy/bt-panel.md |
| `deploy/bt-supervisor.png` | 进程守护管理器添加 zebra-store | 宝塔面板 → 软件商店 → 进程守护管理器 → 添加守护进程 | 按手册填写的表单 | 宝塔面板账号 | 1440×900 | deploy/bt-panel.md |
| `deploy/bt-nginx-conf.png` | 宝塔站点配置文件 | 宝塔面板 → 网站 → 设置 → 配置文件 | 已粘贴 Zebra Store 配置段，滚动到该段 | 宝塔面板账号 | 1440×900 | deploy/bt-panel.md |
| `deploy/bt-ssl.png` | 宝塔申请 Let's Encrypt 证书 | 宝塔面板 → 网站 → 设置 → SSL → Let's Encrypt | 证书申请成功、强制 HTTPS 已开启 | 宝塔面板账号 | 1440×900 | deploy/bt-panel.md |

## 对接（8）

| 文件 | 说明 | 页面 / URL | 截图前的准备 | 账号 | 视口 | 用在 |
|---|---|---|---|---|---|---|
| `integration/zs-apply-credential.png` | 在 A 站申请 API 凭证 | S/me/api | 一个**新注册**账号点击“申请”后的“审核中”状态 | 新测试账号 | 1440×900 | integration/zebra-store.md |
| `integration/zs-connection-code.png` | 生成连接码 | S/me/api | 点击“生成连接码”后显示连接码的弹窗（截图后对连接码打码） | zs2-supply@lab.test | 1440×900 | integration/zebra-store.md |
| `integration/zs-paste-code.png` | 粘贴连接码并握手 | https://zs2.dot2.com/admin/site-connections | 新建连接，粘贴连接码并解析，显示握手成功信息（不保存；连接码打码） | admin（zs2） | 1440×1000 | integration/zebra-store.md |
| `integration/zs-procurement-fulfilled.png` | 采购单已完成 | https://zs2.dot2.com/admin/procurement-orders | verify case c 之后，有 fulfilled 采购单 | admin（zs2） | 1440×900 | integration/zebra-store.md |
| `integration/dujiao-connection.png` | 新建 dujiao-next 连接 | 后台/site-connections | 编辑 dujiao-next 连接的对话框（Secret 留空） | admin | 1440×1000 | integration/dujiao-next.md |
| `integration/acg-connection.png` | 新建异次元连接 | 后台/site-connections | 编辑 acg-faka 连接的对话框 | admin | 1440×1000 | integration/acg-faka.md |
| `integration/acg-shared-store.png` | 异次元后台添加共享店铺 | https://acg.dot2.com/admin → 店铺共享 | 列表中显示 store.dot2.com（异次元类型）已连接 | acg-faka 管理员（lab.sh creds） | 1440×900 | integration/acg-faka.md |
| `integration/procurement-manual-review.png` | 待人工核对的采购单 | 后台/procurement-orders?status=manual_review | 需要一条 manual_review 采购单：本地实例上停掉上游后下单制造；或用集成测试同样的方法 | admin | 1440×900 | integration/procurement.md |

## 支付渠道（6）

| 文件 | 说明 | 页面 / URL | 截图前的准备 | 账号 | 视口 | 用在 |
|---|---|---|---|---|---|---|
| `payment/epay.png` | 易支付渠道配置 | 后台/payment-channels | 新建渠道：官方文档示例值的易支付配置（假数据，不保存） | admin | 1440×1000 | payment/epay.md |
| `payment/alipay.png` | 支付宝渠道配置 | 后台/payment-channels | 新建渠道：提供方“官方”、类型 alipay 的字段（假数据，不保存） | admin | 1440×1000 | payment/alipay.md |
| `payment/wechat.png` | 微信支付渠道配置 | 后台/payment-channels | 新建渠道：官方 + wechat 的字段（假数据，不保存） | admin | 1440×1000 | payment/wechat.md |
| `payment/paypal.png` | PayPal 渠道配置 | 后台/payment-channels | 新建渠道：官方 + paypal（假数据，不保存） | admin | 1440×1000 | payment/paypal.md |
| `payment/stripe.png` | Stripe 渠道配置 | 后台/payment-channels | 新建渠道：官方 + stripe（假数据，不保存） | admin | 1440×1000 | payment/stripe.md |
| `payment/crypto-pay-page.png` | 加密货币支付页 | S/pay?order_no={订单} | 需要一个 BEpusdt/epusdt 测试网关返回收款地址；没有时在本地用 mock 网关 | buyer@lab.test | 1440×900 | payment/crypto.md |

## 分站（6）

| 文件 | 说明 | 页面 / URL | 截图前的准备 | 账号 | 视口 | 用在 |
|---|---|---|---|---|---|---|
| `reseller/apply.png` | 分销商申请 | S/reseller/apply | 一个新注册、尚未申请的账号 | 新测试账号 | 1440×900 | reseller/index.md |
| `reseller/domains.png` | 分销控制台域名 | S/reseller/domains | 显示系统子域名 sakura | reseller-sakura@lab.test | 1440×900 | reseller/index.md |
| `reseller/site-config.png` | 分站配置 | S/reseller/site | 显示 Sakura 樱花小铺的配置 | reseller-sakura@lab.test | 1440×1200 整页 | reseller/index.md |
| `reseller/products.png` | 分销商品加价 | S/reseller/products | 打开 lab-e2e-card 的规则编辑，显示加价 15% 和预览价 | reseller-sakura@lab.test | 1440×1000 | reseller/index.md |
| `reseller/storefront-sakura.png` | 分站前台 | https://sakura.dot2.com/ | 分站首页，显示分站名称和 Logo | 访客 | 1440×900 | reseller/index.md |
| `reseller/withdraw.png` | 分销商提现 | S/reseller/withdraws | 有一条已打款的提现记录 | reseller-sakura@lab.test | 1440×900 | reseller/settlement.md |
