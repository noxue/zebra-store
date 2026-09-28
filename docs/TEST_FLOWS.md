# 端到端测试流程清单（dot2.com 实验环境）

这份清单列出在 dot2.com 实验环境上可以**真实操作**的端到端流程，供人工测试和后续自动化补全使用。
每条流程都有编号、前置条件、步骤、期望结果、在哪个站点操作，以及已有的自动化测试。

⚠️ 表示高风险流程（涉及资金、库存或安全），每次发版都要回归。

## 0. 约定

### 站点

| 简写 | 地址 | 说明 |
|---|---|---|
| **S** | `https://store.dot2.com` | 主站 Zebra Store 前台 |
| **SA** | `https://store.dot2.com/admin/` | 主站管理后台 |
| **Z2** / **Z2A** | `https://zs2.dot2.com` / `https://zs2.dot2.com/admin/` | 第二个 Zebra Store，通过 `zebra-store` 协议从 S 进货（S 的下游） |
| **ACG** / **ACGA** | `https://acg.dot2.com` / `https://acg.dot2.com/admin` | 真实的异次元发卡 acg-faka。它是 S 的上游，同时也通过 `/shared/*` 从 S 进货 |
| **DJ** / **DJA** | `https://dujiao.dot2.com` / `https://dujiao.dot2.com/admin` | 原版 dujiao-next，S 的上游 |
| **SK** / **NE** / **MA** | `https://sakura.dot2.com` / `neon` / `matcha` | S 的三个分销商分站：同一个后端，按 `Host` 区分租户 |
| **DOC** | `https://docs.dot2.com` | 使用手册 |

### 账号

所有密码都在服务器上执行 `/opt/zebra-lab/lab.sh creds` 查看，也保存在 `build/state/credentials.txt`。

| 账号 | 用途 |
|---|---|
| `admin`（S / Z2 / DJ） | 各站超级管理员 |
| `buyer@lab.test`（S、Z2） | 普通买家，钱包里预充了 5000 |
| `zs2-supply@lab.test`（S） | Z2 在 S 上的采购账号：API 凭证已审核，钱包 5000 |
| `acg-down@lab.test`（S） | ACG 在 S 上的下游账号，持有异次元兼容密钥 |
| `reseller-sakura@lab.test` / `-neon` / `-matcha`（S） | 三个分销商 |
| `zebrasupply`（ACG 会员） | S 在 ACG 上的进货账号，余额 5000 |
| `zebra-supply@lab.test`（DJ） | S 在 DJ 上的进货账号，API 凭证已审核 |
| `acgbuyer`（ACG 会员） | 在 ACG 前台购买 S 商品的买家 |

### 测试商品

`lab-e2e-card`（S，10.00，300 张卡密，卡密前缀 `ZS-LAB-`）；ACG 来源商品（卡密前缀 `ACG-LAB-`）；
DJ 来源商品（卡密前缀 `DJ-LAB-`）；以及 `backend/scripts/seed_store.py` 生成的演示目录。

### 测试支付通道

实验环境**没有真实收款渠道**，标准测试支付路径有两条：
1. **钱包通道**（推荐）：管理员在 SA「用户详情 → 钱包调整」给用户加款，下单时勾选“使用余额”；
2. **epay 模拟通道**（只用于测试环境）：在 SA 新建一个 epay 渠道，网关地址填任意不可达域名，下单后按 epay v1 规则自己计算 MD5 签名，把回调 POST 到 `/api/v1/payments/callback`。签名方法见 `e2e/support/api.ts#sendEpayCallback`。

### “自动化覆盖”列的写法

- `api:文件::测试名`：`backend/crates/api/tests/文件.rs` 中的集成测试（内存 SQLite，真实路由）
- `e2e:NN`：`e2e/tests/NN-*.spec.ts`（Playwright，驱动真实 UI）
- `verify:x`：`deploy/provision/verify.py` 在 dot2.com 实验环境上跑的 case x（a–e）
- `unit:路径`：前端或 crate 内的单元测试
- `—`：暂时没有自动化测试，需要人工回归

### 重置环境

一条流程可能改动共享数据，比如禁用分销商或清空库存。测完后在服务器上执行 `lab.sh provision`（幂等，会补库存和配置）；如果需要彻底重置，执行 `lab.sh reset && lab.sh all`。

---

## 1. 访客 / 用户前台

### 1.1 浏览、搜索、语言、主题

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-001 | 首页展示 | 已 provision | 打开首页 | 站点名“斑马小铺”/后台配置的名称、Logo、Favicon、Banner 轮播、推荐商品、最新文章；无控制台报错 | S `/` | e2e:02 `home shows the configured site name and theme` |
| F-002 | 首页公告弹窗 | SA 开启首页公告 | 打开首页 → 分别选“本次关闭 / 今日不再 / 永久关闭” | 弹窗按所选策略不再出现（localStorage `announcement_dismiss`） | S `/` | api:content_public_config::set_02_announcement_schedule |
| F-003 | 商品列表与分类 | 至少 2 个分类 | `/products` → 点左侧分类 → 折叠父分类 | URL 变为 `/categories/:slug`，只显示该分类商品；手机上分类是抽屉 | S `/products` | api:catalog_public::visibility_follows_product_and_category_state |
| F-004 | 搜索 | — | 在商品页搜索框输入“Claude” | 300ms 防抖后过滤；无结果时显示“清除筛选” | S `/products` | — |
| F-005 | 列表模式 | SA 模板配置改为 `list` | 刷新首页 | 首页变为“分类侧栏 + 分组列表”，`/products` 与首页相同 | S | — |
| F-006 | 商品详情 | — | 打开 `lab-e2e-card` | 显示价格、库存状态、购买类型（游客/会员）、发货方式（自动/人工）、SKU 选择、详情富文本 | S `/products/:slug` | api:catalog_public::detail_shape_and_prices |
| F-007 | 库存展示模式 | SA 把商品 `stock_display_mode` 分别设为 exact/status/range/hidden | 刷新详情 | 分别显示精确数 / 充足紧张 / 区间 / 隐藏 | S | api:catalog_public::stock_is_masked_outside_exact_mode |
| F-008 | 售罄与下架 | 把某 SKU 库存清零；再把商品下架 | 查看详情与列表 | 售罄 SKU 不可选；下架商品在列表消失，直接访问返回 404 页 | S | api:catalog_public::visibility_follows_product_and_category_state |
| F-009 | 切换语言 | — | 导航栏语言切换 简体/繁體/English | 界面文案与多语言商品标题切换；刷新后保持（localStorage `locale`）；请求带 `X-Lang` | S | — |
| F-010 | 亮/暗主题 | — | 点主题切换 | `<html>` 切换 `.dark`，刷新后保持（`dujiao_theme`）；樱花动效在 `prefers-reduced-motion` 下关闭 | S | — |
| F-011 | 后台主题色生效 | SA「站点设置 → 模板配置 → 主题外观」改主色 | 刷新前台 | 主按钮、链接颜色随配置变化，未配置时回到默认樱花粉 | S | e2e:01 `site settings: name, logo, theme colors…` |
| F-012 | 博客 / 公告 / 关于 / 条款 | SA 已发布文章与公告 | 依次打开 `/blog`、`/blog/:slug`、`/notice`、`/about`、`/terms`、`/privacy` | 内容与后台一致，相关商品链接可用 | S | api:content_posts::post_crud_publication_and_public_views |
| F-013 | 404 页 | — | 打开 `/not-exist` | 二次元空状态插画、返回首页按钮 | S | — |
| F-014 | SEO 文件 | SA 设置 `brand.site_url` | 访问 `/sitemap.xml`、`/robots.txt` | sitemap 使用配置的站点地址，只含上架商品与已发布文章 | S | api:content_sitemap::set_03_sitemap_uses_configured_url_and_filters |
| F-015 | 移动端布局 | 手机或 DevTools 390px | 浏览首页、详情、购物车 | 底部导航（首页/商品/购物车/我的）；详情页滚动后出现底部购买栏 | S | — |
| F-016 | 快速购买弹窗 | — | 在商品卡片点“快速购买” | 弹窗内可选 SKU、数量，加入购物车或立即购买 | S | — |

### 1.2 注册、登录、两步验证、找回密码

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-020 | 免验证码注册 | SA 注册开启、邮箱验证关闭（实验环境默认） | `/auth/register` 填邮箱、密码、勾选协议 | 注册成功并登录，进入 `/me/orders`；分配默认会员等级 | S | e2e:03 `register (no email verification) and log in`；api:identity_user_auth::registration_assigns_default_member_level |
| F-021 | 邮箱验证码注册 | SA 开启邮箱验证 + 配好 SMTP | 发送验证码 → 填码注册 | 收到带站点品牌的邮件；60 s 内不能重发；错码 5 次后作废 | S | api:identity_user_auth::register_with_email_code、verify_code_attempt_limit；api:regression_mail::ntf_02_verify_code_mail_brand_follows_the_storefront |
| F-022 | 邮箱域名白名单 | SA 开启白名单，只允许 `lab.test` | 用 `x@gmail.com` 注册 | 被拒绝；注册页出现域名下拉 | S | api:identity_user_auth::email_domain_allowlist |
| F-023 | 关闭注册 | SA 关闭注册 | 打开注册页 | 显示“注册已关闭”；接口拒绝 | S | api:identity_user_auth::registration_and_verification_switches |
| F-024 | ⚠️ 登录与限流 | 已有账号 | 连续输错密码 5 次 | 第 6 次被锁 15 分钟（`security.login_rate_limit`）；登录日志记下失败原因 | S | api:identity_user_auth::login_rate_limits、login_errors_and_logs |
| F-025 | 图形验证码 | SA「验证码配置」provider=image，场景勾选 login | 登录 | 出现验证码图片；错码被拒 | S | api:identity_user_auth::image_captcha_on_login_and_send_code |
| F-026 | ⚠️ 用户开启 2FA | 已登录 | `/me/security` → 两步验证 → 扫码 → 输入 6 位码 → 保存恢复码 | 状态变为已开启；下次登录要求输入 TOTP | S | api:identity_me::user_2fa_management_and_admin_reset；api:identity_user_auth::user_login_with_2fa |
| F-027 | ⚠️ 2FA 登录与恢复码 | F-026 | 登录 → 输入 TOTP；另一次改用恢复码 | 两种方式都能登录；恢复码用一次即失效；挑战超时后回到密码步骤 | S | api:identity_user_auth::user_login_with_2fa |
| F-028 | ⚠️ 找回密码 | 邮箱验证开启 + SMTP | `/auth/forgot` 发码 → 设新密码 | 旧密码失效，旧会话被吊销 | S | api:identity_user_auth::forgot_password_flow |
| F-029 | 修改密码 / 邮箱 | 已登录 | `/me/security` 改密码；改邮箱（旧邮箱码 + 新邮箱码） | 改密码后其他会话失效；邮箱更换成功 | S | api:identity_me::change_password_revokes_sessions、change_email_with_both_codes |
| F-030 | 登录历史 | 已登录 | `/me/security` 底部登录记录 | 显示时间、IP、来源、成功/失败 | S | api:identity_me::own_login_history |
| F-031 | 个人资料 | 已登录 | `/me/profile` 改昵称、语言 | 保存后头部卡片更新 | S | api:identity_me::profile_contract_and_update |
| F-032 | Telegram / Google 登录 | 需要真实 Bot / Google Client ID（实验环境未配） | 登录页点第三方登录 | 未配置时按钮不显示；配置后能登录/绑定/解绑 | S | api:identity_oauth_telegram::*、api:identity_oauth_google::* |
| F-033 | ⚠️ 被禁用用户 | SA 禁用某用户 | 该用户登录 / 继续使用旧 token | 登录被拒；旧 token 立即失效 | S | api:dashboard_users::password_change_and_disable_revoke_tokens |

### 1.3 购物车、下单与定价

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-040 | 购物车 | — | 加入 2 个商品 → 改数量 → 删除 → 撤销 | 数量受最小/最大购买量和库存限制；刷新后仍在（`cart_items`） | S `/cart` | api:order_member::cart_round_trip |
| F-041 | ⚠️ 游客下单（自动发货） | 商品购买类型=游客；epay 模拟渠道或无渠道 | 详情 → 立即购买 → 填邮箱和查询密码 → 选渠道 → 支付 → 模拟回调 | 跳到 `/guest/orders/:no`，状态为已交付，显示卡密 | S | e2e:02 `guest: product detail → buy now → …`；api:order_guest::guest_checkout_callback_auto_delivery_and_download |
| F-042 | 仅会员可购 | 商品 `purchase_type=member` | 未登录打开详情 | 按钮变为“登录后购买” | S | api:catalog_product::purchase_limits_and_enum_validation |
| F-043 | ⚠️ 会员下单 + 余额全额支付 | buyer 有余额 | 登录 buyer → 购买 `lab-e2e-card` → 勾选使用余额 → 提交 | 订单直接变为已支付，自动发卡；钱包扣款金额正确 | S | verify:a/b（订单部分）；api:order_member::wallet_full_payment_and_refund_on_cancel |
| F-044 | ⚠️ 余额部分支付 + 在线补差 | 余额小于订单金额 | 勾选余额 → 选在线渠道 → 模拟回调 | 余额先扣，回调补齐后订单完成；详情分别显示余额支付和在线支付金额 | S | api:order_member::wallet_partial_payment_then_callback |
| F-045 | ⚠️ 优惠券 | SA 建优惠券（满 20 减 10% 等） | 结算页输入券码 | 预览金额正确（e2e 用例 25.00 → 22.50）；超过次数或每人限额后被拒 | S | e2e:03 `cart → checkout with coupon…`；api:marketing_coupon::coupon_evaluation_through_service、ledger_enforces_limits_under_concurrency |
| F-046 | 活动价 | SA 为商品建活动价 | 查看详情和结算 | 显示原价划线和活动价，结算有“活动优惠”一行 | S | api:order_member::prc_member_pricing_with_coupon_promotion_and_wholesale |
| F-047 | 会员价 / 等级折扣 | 用户等级带折扣或商品设了会员价 | 登录后查看价格并结算 | 显示会员价；结算有“会员优惠” | S | 同上 |
| F-048 | 批发价阶梯 | SA 设“≥5 件单价 8.00” | 数量 4 → 5 | 第 5 件时单价切换为 8.00，结算有“批发优惠” | S | 同上；api:catalog_product::wholesale_prices_semantics |
| F-049 | 优惠叠加规则 | 券勾选“不可与批发价同享” | 批发数量 + 用券 | 按规则只取其一，金额与后台预期一致 | S | api:order_member::prc_member_pricing_with_coupon_promotion_and_wholesale |
| F-050 | 人工发货商品的下单表单 | 商品有 manual_form_schema（例如账号、区服） | 结算时不填必填项 / 格式错误 | 被前端和后端同时拦截；正确填写后提交成功，表单内容出现在订单详情 | S | api:bind_messages::storefront_modules_name_failed_fields |
| F-051 | ⚠️ 多商品订单（父子单） | 购物车 2 种商品 | 一次结算 | 生成父订单 + 子订单，各自交付；详情页有“子订单”分区 | S | api:integration_procurement::parent_with_children |
| F-052 | ⚠️ 超时未支付取消 | 渠道下单后不支付 | 等待 15 分钟（`order.payment_expire_minutes`） | 订单变为已过期，库存和优惠券次数退回；过期后迟到的回调不会把订单变成已支付 | S | api:order_guest::timeout_cancel_then_late_callback、ord01_paid_order_survives_timeout_job |
| F-053 | 用户主动取消 | 待支付订单 | 订单详情 → 取消 | 状态变为已取消，优惠券次数退回 | S | api:regression_order_payment::ord_01_user_cancel_returns_coupon_usage |
| F-054 | ⚠️ 游客风控 | SA 开启订单风控：同 IP 最多 2 个待支付订单 | 同一 IP 连续下 3 单不付 | 第 3 单被拒 | S | api:regression_order_payment::risk_01_guest_pending_limit_holds_under_concurrency |
| F-055 | IP 黑名单 | SA 风控把测试 IP 加入黑名单 | 下单 | 被拒 | S | — |
| F-056 | ⚠️ 最后一张卡密并发 | 库存只剩 1 张 | 两个浏览器同时下单并支付 | 只有一单拿到卡密，另一单不会发出重复卡密 | S | api:order_admin::concurrent_orders_on_last_card_secret、api:catalog_card_secret::secret_reservation_never_double_books |

### 1.4 支付

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-060 | 支付页二维码 / 跳转 | epay 渠道（交互方式=二维码或跳转） | 下单 → 支付页 | 二维码在本地生成；跳转模式有“打开支付链接”；倒计时正确 | S `/pay` | e2e:02 |
| F-061 | ⚠️ 回调验签 | epay 渠道 | 发送未签名或签名错误的回调 | 被拒，订单不变 | API | api:payment_callback::pay_14_unsigned_callback_rejected、pay_35_epay_rejections |
| F-062 | ⚠️ 重复回调只入账一次 | 同上 | 同一笔回调并发发送 3 次 | 订单只支付一次，余额/发货不重复 | API | api:payment_callback::pay_04_epay_callback_settles_once；api:regression_order_payment::pay_04_concurrent_duplicate_callbacks_settle_once |
| F-063 | ⚠️ 少付回调 | 同上 | 回调金额比应付少 | 订单不算支付成功，少付金额计入用户余额 | API | api:order_guest::pay02_underpaid_callback_does_not_pay；api:order_member::pay02_member_underpaid_credit |
| F-064 | 换支付方式 | 待支付订单 | 支付页点“更换方式”换另一渠道 | 旧支付单被取代；旧单迟到的成功回调会被标记 | S | api:regression_order_payment::pay_03_new_link_supersedes_old_and_late_success_is_flagged |
| F-065 | 手续费由买家承担 | 渠道费率 2% + SA 开启 customer_fee | 结算 | 渠道卡片显示手续费，应付金额含手续费 | S | — |
| F-066 | 只允许余额支付 | SA「钱包配置」开启 wallet_only_payment | 结算 | “使用余额”被强制勾选，不显示在线渠道 | S | api:regression_order_payment::pay_23_wallet_only_mode_is_enforced |
| F-067 | 渠道金额范围 / 角色限制 | 渠道设 min 5 / max 100，只对会员开放 | 游客下 200 元订单 | 该渠道不出现或提示超出范围 | S | api:payment_admin::channel_changes_refresh_public_config |
| F-068 | 其他网关回调（alipay/wechat/stripe/paypal/USDT 等） | 需真实商户，实验环境不测 | — | 由集成测试覆盖 | — | api:payment_callback::pay_34_alipay_notification、pay_32_wechat_notification_blind_matching、pay_25_stripe_webhook、pay_30_paypal_webhook、pay_42_epusdt_and_bepusdt_callbacks、pay_46_tokenpay_callback、pay_39_okpay_callback、pay_48_dujiaopay_blind_matching |

### 1.5 订单查询与交付

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-070 | 游客查单 | F-041 | `/guest/orders` 输入邮箱 + 查询密码（可选订单号） | 列出订单；能查看卡密；可以“清除已保存的凭据” | S | e2e:02 `guest order lookup…` |
| F-071 | ⚠️ 游客凭据错误 | — | 用错误密码查单 | 查不到任何订单，不泄露是否存在 | S | api:order_guest::* |
| F-072 | 会员订单列表 | 已有订单 | `/me/orders` 按状态筛选、按单号搜索；切换到“充值订单” | 统计卡片与列表一致 | S | e2e:03 `personal center pages load…` |
| F-073 | 卡密交付 | 自动发货订单 | 订单详情 → 复制 / 下载 | 卡密逐行显示；下载 txt 内容一致；内容过长时提示已截断 | S | api:order_guest::guest_checkout_callback_auto_delivery_and_download |
| F-074 | ⚠️ 人工交付 | 人工发货商品已支付 | SA 订单列表 → 发货 → 填交付内容 | 买家订单详情出现交付内容；状态变为已交付；如开启邮件则收到发货邮件 | SA → S | api:order_admin::manual_delivery_and_refunds |
| F-075 | 使用说明 | 商品设置了 instructions | 查看已交付订单 | 每个商品下显示使用说明 HTML | S | — |
| F-076 | 订单邮件 | SMTP + 订单通知开启 | 下单 → 支付 → 发货 | 按场景发送邮件，模板变量已替换 | 邮箱 | api:regression_order_payment::ntf_01_status_mail_rules |

### 1.6 售后与退款

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-080 | ⚠️ 退款到余额 | 已支付订单 | SA 订单详情 → 退款 → 退到钱包，金额 5.00 | 买家余额 +5.00；订单状态变为部分退款；订单详情出现退款记录 | SA → S | e2e:04 `refund the member order to the wallet`；api:order_admin::manual_delivery_and_refunds |
| F-081 | ⚠️ 手动退款（线下） | 已支付订单 | SA → 手动退款，勾选“手续费已退” | 只记录退款，不动余额；退款记录显示类型为手动 | SA | api:order_admin::manual_delivery_and_refunds |
| F-082 | ⚠️ 并发退款 | 同一订单 | 两个标签页同时退全额 | 只成功一次，不会重复加余额 | SA | api:order_admin::concurrent_refunds_do_not_double_credit |
| F-083 | 退款期限 | `order.max_refund_days` | 对超过期限的订单退款 | 被拒 | SA | — |
| F-084 | 联系客服 | 前台配置了 Telegram/WhatsApp | 页脚 / 关于页点联系方式 | 跳到对应链接 | S | — |

### 1.7 钱包与礼品卡

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-090 | ⚠️ 钱包充值 | SA「钱包配置」勾选了充值渠道 | `/me/wallet` → 输入金额 → 选渠道 → 支付 → 模拟回调 | 跳到 `/recharge-orders/:no` 显示成功；余额只增加一次；可能触发会员升级 | S | e2e:03 `…wallet recharge…`；api:wallet_recharge::wal_01_success_credits_once_with_hooks |
| F-091 | 充值状态 | 充值单未支付 | 等待过期 / 点“检查状态” | 过期后状态为 expired；迟到回调不入账 | S | api:wallet_recharge::wal_01_terminal_state_matrix |
| F-092 | 钱包流水 | 有充值和消费 | `/me/wallet` 流水表 | 类型、方向、金额、余额快照都正确 | S | api:wallet_ledger::* |
| F-093 | ⚠️ 礼品卡兑换 | SA 生成礼品卡 | `/me/gift-cards` 输入卡号 | 余额增加；同一张卡第二次兑换被拒；并发兑换只成功一次 | S | api:wallet_gift_card::redeem_credits_wallet_once、double_redeem_is_impossible_under_concurrency |
| F-094 | 礼品卡限流 / 验证码 | 开启 gift_card_redeem 验证码场景 | 连续输错卡号 | 触发验证码与限流 | S | api:wallet_gift_card::risk_02_redeem_rate_limit、captcha_scene_is_enforced |
| F-095 | 会员自动升级 | 等级设置了累计充值 / 消费门槛 | 充值或消费越过门槛 | 等级自动提升，不会降级或平级跳转；概览页显示升级进度 | S | api:marketing_member_level::upgrades_are_atomic_and_never_sideways |

### 1.8 推广返利

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-100 | 开通推广 | SA 开启返利、比例 10% | `/me/affiliate` → 开通 | 获得推广码和推广链接 `/?aff=CODE` | S | api:affiliate_user::open_requires_program_and_is_idempotent |
| F-101 | 点击统计 | F-100 | 用另一个浏览器访问推广链接两次 | 点击数 +1（同一访客去重），30 天内归因（`dj_affiliate_attribution`） | S | api:affiliate_user::clicks_are_tracked_and_deduplicated |
| F-102 | ⚠️ 推广订单与佣金 | F-101 的访客下单并支付 | 查看推广面板 | 生成待确认佣金，确认期过后变为可提现 | S | api:affiliate_user::commissions_follow_orders_and_confirmation |
| F-103 | ⚠️ 退款扣回佣金 | 推广订单部分退款 | SA 退款 | 佣金按退款比例扣回 | SA → S | api:affiliate_user::refund_clawback_is_proportional |
| F-104 | ⚠️ 佣金提现 | 可提现 ≥ 最低金额 | 填金额、渠道、账号提交 | 佣金冻结；SA 通过后变为已提现，驳回后退回可提现 | S → SA | api:affiliate_user::withdrawals_freeze_and_split_commissions；api:affiliate_admin::commission_and_withdraw_review |

### 1.9 API 凭证、连接码、兼容密钥（用户侧）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| F-110 | 申请 API 凭证 | 已登录 | `/me/api` → 申请 | 状态为待审核；SA 审核通过后显示 API Key 和 Secret 尾号 | S → SA | api:integration_credentials::apply_review_and_self_service |
| F-111 | 被驳回后重新申请 | SA 驳回并填写原因 | 用户查看后重新申请 | 显示驳回原因，可以重新申请 | S | api:integration_credentials::ups18_reject_delete_and_reapply |
| F-112 | ⚠️ 重新生成 Secret | 已审核 | 点“重新生成” | 新 Secret 只显示一次，旧 Secret 立即失效 | S | api:integration_credentials::apply_review_and_self_service |
| F-113 | ⚠️ 生成连接码 | 已审核 | “连接码” → 生成 → 复制 | 得到 `zsc1_…`，只显示一次；新密钥作为 secret_next，旧密钥 7 天内仍有效 | S | api:integration_zebra_store::zebra_store_end_to_end |
| F-114 | 停用凭证 | 已审核 | 关闭“启用”开关 | 下游对接请求全部被拒（403 forbidden） | S | api:integration_upstream_api::ups10_authentication_failures |
| F-115 | ⚠️ 生成异次元/萌次元兼容密钥 | 凭证已审核，`integration.acg_faka_compat=true` | “异次元 / 萌次元 对接” → 生成 | 显示站点地址、商户 ID（= 用户 ID）和 32 位对接密钥；重置后旧密钥立即失效 | S | api:integration_provider_acg_faka::prv07_credential_and_key_state |
| F-116 | 兼容访问开关与 IP 白名单 | F-115 | 关闭兼容访问；或只允许某个 IP | 对方请求返回“商户ID不存在”（不说明原因）；白名单外的 IP 被拒 | S | 同上 |
| F-117 | 本站未开启兼容协议 | `ZS__INTEGRATION__ACG_FAKA_COMPAT=false` | 打开 API 对接页；请求 `/shared/authentication/connect` | 页面显示“本站未开启”；接口返回 404 | S | api:integration_provider_acg_faka::switch_off_answers_404 |

---

## 2. 管理后台

### 2.1 登录、合规、安全

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-001 | 管理员登录 | `admin` 密码 | `/admin/login` 登录 | 进入仪表盘；错误时提示统一文案 | SA | e2e:01 `admin login and compliance acknowledgement`；api:admin_auth::login_contract_and_errors |
| B-002 | ⚠️ 合规确认 | 新站首次登录 | 进入支付相关页面 → 按提示输入 4 段确认语（禁止粘贴） | 确认后才可访问支付、钱包、分销财务等页面；非超管看到“需要超级管理员确认”页 | SA | api:identity_compliance::acknowledge_and_gate |
| B-003 | ⚠️ 管理员 2FA | — | 安全设置 → 开启两步验证 → 保存恢复码 → 退出重登 | 登录需 TOTP；5 次错误后挑战失效 | SA `/security` | api:identity_admin_2fa::status_setup_enable_and_login_with_challenge、challenge_revoked_after_five_failures |
| B-004 | ⚠️ 修改管理员密码 | — | 安全设置 → 修改密码 | 旧 token 全部失效 | SA | api:admin_auth::password_change_revokes_old_tokens |
| B-005 | ⚠️ 命令行重置 | 服务器 shell | `zebra-store admin reset-password --username admin` / `reset-2fa`（别名 `reset2fa`） | 重置成功，所有会话被吊销 | 服务器 | api:identity_admin_2fa::cli_resets_revoke_sessions；server:cli::tests::qa_a06_reset_2fa_names |

### 2.2 仪表盘与报表

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-010 | KPI 与趋势 | 有订单 | 仪表盘切换 今日/7天/30天/自定义；点“立即刷新” | 订单数、GMV、成本、利润、支付成功率等与实际一致；趋势柱状图按天分组 | SA `/` | e2e:04 `dashboard numbers reflect the activity`；api:dashboard_reports::overview_kpis_funnel_and_alerts、trends_bucket_by_requested_zone |
| B-011 | 排行与漏斗 | 同上 | 查看热销商品、渠道排行、转化漏斗 | 数据与订单一致 | SA | api:dashboard_reports::rankings_products_and_channels |
| B-012 | 库存告警 | 把某 SKU 库存降到阈值以下 | 查看告警区 | 出现低库存告警并链接到商品 | SA | api:dashboard_reports::inventory_alerts_rows |
| B-013 | 退款冲减成本 | 设置“仪表盘 → 退款冲减成本” | 退款后刷新 | 成本与利润按设置变化 | SA | api:dashboard_reports::refund_reverses_cost_setting |
| B-014 | 版本与升级 | — | 顶栏“检测更新” | 显示当前版本；提示“当前部署方式不支持一键升级” | SA | api:dashboard_system::version_and_update_capability；admin:useSystemUpdateInfo.test.ts |

### 2.3 系统设置（站点、品牌、模板）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-020 | 站点名 / Logo / Favicon | — | 系统设置 → 基础配置：改站点名，上传 Logo 和 Favicon → 保存 | 前台导航、标题、Favicon、页脚立即更新（配置缓存已失效）；后台登录页同步 | SA → S | e2e:01 `site settings…`；api:content_public_config::cache_is_invalidated_on_save |
| B-021 | 货币、SEO、联系方式、页脚链接、自定义脚本 | — | 基础配置各字段 | 前台价格单位、`<title>`/meta、页脚随之变化；脚本按 head/body_end 注入 | SA → S | api:content_public_config::public_config_shape |
| B-022 | 主题外观 | — | 模板配置 → 主题外观：主色/次色/强调色、背景图、看板娘、登录背景、樱花/星光开关、默认模式 → 保存；再点“恢复默认色” | 前台和后台登录页生效；恢复后回到默认色 | SA → S | e2e:01 |
| B-023 | 模板模式 | — | 模板配置切换 card/list | 见 F-005 | SA | — |
| B-024 | 导航配置 | — | 关闭“博客”，添加一个外链菜单 | 前台导航隐藏博客，出现新菜单 | SA → S | — |
| B-025 | 关于我们 / 法律条款 / 首页公告 | — | 编辑三种语言并保存 | 前台对应页面更新；公告按起止时间显示 | SA → S | api:content_public_config::set_02_announcement_schedule |
| B-026 | SMTP 与测试邮件 | 有 SMTP 账号 | 邮件配置填写 → 发送测试邮件 | 收到测试邮件；密码回显为掩码，保存时不会被清空 | SA | api:content_settings::smtp_mask_patch_and_test |
| B-027 | 订单邮件模板 | — | 选场景 → 改三种语言的标题和正文 → 保存；再点重置 | 发出的邮件使用新模板；重置后恢复默认 | SA | api:content_settings::affiliate_templates_notifications_bot |
| B-028 | 验证码配置 | — | provider 切换 none/image/turnstile，勾选场景 | 前台对应场景出现验证码 | SA → S | api:content_settings::captcha_telegram_google_settings |
| B-029 | 注册配置 | — | 基础配置：开关注册、邮箱验证、域名白名单 | 见 F-020~F-023 | SA | api:identity_user_auth::registration_and_verification_switches |
| B-030 | 订单配置 | — | 支付超时分钟数、最大退款天数 | 新订单的过期时间按新值计算 | SA | api:content_settings::generic_settings_get_put_normalize |
| B-031 | 上游同步配置 | — | 上游同步：同步间隔、下单前库存检查、并发、分页 | 同步任务按新间隔执行 | SA | api:integration_mappings::ups06_pre_order_stock_guard |
| B-032 | ⚠️ 回调路由 | — | 支付 → 回调路由：把支付回调改成 `/api/pay/notify-x` | 保存前校验前缀、`..` 与冲突（与内置回调路径或彼此重复 → 拒绝保存）；新路径能收回调，**旧的默认路径随之隐藏（404，与原项目一致）** | SA | api:content_settings::set_01_callback_routes_normalized_via_api；api:payment_callback::custom_callback_route |

### 2.4 权限管理（RBAC）与审计

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-040 | 内置角色 | 新站 | 权限管理 → 角色列表 | 有 6 个内置角色：readonly_auditor、operations、support、integration、finance、system_admin，内置角色不可修改 | SA `/authz` | api:identity_rbac::bootstrap_resets_builtin_roles、builtin_roles_cover_every_admin_route |
| B-041 | ⚠️ 新建管理员并分配角色 | — | 新建管理员 `auditor1`（非超管）→ 分配 readonly_auditor → 用它登录 | 只能看到有权限的菜单；访问其他页面跳到 403；能看仪表盘但看不到用户列表 | SA | api:dashboard_system::readonly_auditor_sees_dashboard_but_not_users；api:admin_auth::rbac_limits_non_super_admins |
| B-042 | 自定义角色与策略 | — | 新建角色 → 从权限目录一键授予 `GET:/admin/orders` | 该角色的管理员能看订单，但不能改 | SA | api:identity_rbac::roles_and_policies、permission_catalog_lists_admin_routes |
| B-043 | ⚠️ 受保护管理员 | — | 删除 bootstrap 超管 | 被拒 | SA | api:identity_rbac::protected_admin_cannot_be_deleted |
| B-044 | 重置其他管理员 2FA | 另一个管理员已开 2FA | 权限管理 → 管理员 → 重置 2FA | 对方下次登录不再要求 TOTP | SA | api:identity_admin_2fa::super_admin_resets_other_admins_2fa |
| B-045 | 权限审计日志 | 做过 B-041/B-042 | 权限审计页按动作、角色筛选 | 每次角色创建、授权、撤权、分配都有记录（操作人、目标、请求 ID） | SA `/authz-audit-logs` | api:identity_rbac::roles_and_policies |

### 2.5 商品、SKU、卡密

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-050 | 分类增删改 | — | 新建父分类和子分类，上传图标，排序，停用 | 前台树形同步；有子分类的父分类不能改层级；停用后前台隐藏 | SA `/categories` | e2e:01 `catalog…`；api:catalog_category::category_crud_contract |
| B-051 | 新建自动发货商品（多 SKU） | — | 商品列表 → 新建：三语标题、slug、分类、2 个 SKU（价格/成本）、图片、详情、标签 → 保存 | 前台可见；SKU 的价格和库存正确 | SA → S | e2e:01；api:catalog_product::product_crud_contract、sku_sync_semantics |
| B-052 | 人工发货商品 + 下单表单 | — | 发货方式选人工，库存 20，添加表单字段（text 必填、select 选项） | 前台结算出现表单；人工库存随订单锁定/扣减 | SA → S | api:catalog_card_secret::manual_stock_movements_use_conditional_updates |
| B-053 | 列表内联编辑与批量操作 | 多个商品 | 内联改分类、排序、上下架；勾选后批量上架/下架/移动分类/删除 | 返回成功数；前台同步 | SA | api:catalog_product::admin_filters |
| B-054 | ⚠️ 删除保护 | 商品有卡密或订单 | 删除商品 / 停用有卡密的自动 SKU | 按规则阻止或级联处理，不留孤儿数据 | SA | api:catalog_product::delete_guards_and_cascade、auto_sku_with_card_secrets_cannot_be_disabled；api:regression_catalog::ord_05_clean_delete_cascades_to_dependents |
| B-055 | 批发价配置 | — | 营销 → 批发价 → 为 SKU 配阶梯 → 保存；再“清空批发价” | 前台阶梯显示和结算正确 | SA | api:catalog_product::wholesale_prices_semantics |
| B-056 | 商品限定支付渠道 | 2 个渠道 | 商品只勾选渠道 A | 结算只出现渠道 A | SA → S | api:catalog_product::payment_channels_are_filtered_on_save |
| B-057 | ⚠️ 卡密批量粘贴导入 | 自动发货商品 | 卡密导入 → 选商品/SKU → 粘贴 10 行（含重复）→ 勾选去重 | 导入 9 张，批次号可见，统计“可用”+9 | SA `/card-secret-imports` | e2e:01；api:catalog_card_secret::batch_create_resolves_sku_and_deduplicates |
| B-058 | ⚠️ CSV/TXT 文件导入 | 同上 | 上传 csv 和 txt | 两种格式都能导入 | SA | api:catalog_card_secret::import_csv_and_txt |
| B-059 | 卡密库存管理 | 有卡密 | 卡密库存页：按状态、批次筛选；批量改状态、删除；编辑单张 | 统计实时变化；已售卡密不能改回可用（单条报错，批量跳过） | SA `/card-secrets` | api:catalog_card_secret::list_filters_bulk_targets_and_batches；api:catalog_card_secret::qa_a10_a11_reimport_and_sold_secret_guards |
| B-060 | ⚠️ 卡密导出 | 有可用卡密 | 卡密导出 → 数量 5、txt、勾选“导出后删除” | 下载内容正确；导出的卡密不再可售 | SA `/card-secret-exports` | api:catalog_card_secret::export_and_export_available |

### 2.6 订单运营

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-070 | 订单列表筛选排序 | 多个订单 | 按状态、用户、单号、游客邮箱、商品关键字、时间筛选；按金额排序 | 结果正确，分页正确 | SA `/orders` | e2e:04；api:regression_order_payment::db_06_admin_order_list_keyword_status_paging_and_sort |
| B-071 | 订单详情 | — | 查看详情 | 金额分解、商品、子订单、交付内容、采购单、支付记录齐全 | SA | e2e:04 `order list and detail…` |
| B-072 | ⚠️ 手动改状态 | 待支付/已支付订单 | 状态下拉 → 改为已完成等 | 只允许合法状态迁移；已完成/已退款订单不可改 | SA | api:order_admin::admin_status_changes |
| B-073 | ⚠️ 人工发货 | 见 F-074 | — | — | SA | api:order_admin::manual_delivery_and_refunds |
| B-074 | ⚠️ 退款 | 见 F-080~F-082 | — | 退款记录页可查看，可切换“手续费已退” | SA `/order-refunds` | e2e:04 |
| B-075 | 支付记录与导出 | 有支付 | 支付记录按状态/渠道筛选 → 查看详情 → 导出 | 详情含网关单号、原始回调；导出文件内容与筛选一致 | SA `/payments` | api:payment_admin::payment_records |
| B-076 | 订单风控设置 | — | 订单风控：开关、IP 黑名单、游客/会员限额、限流；“应用推荐游客策略” | 保存后前台生效（见 F-054/F-055） | SA `/order-risk-control` | api:regression_order_payment::risk_01_guest_pending_limit_holds_under_concurrency |

### 2.7 用户、钱包、会员等级

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-080 | 用户列表 | — | 按 ID、关键字、状态、注册/登录时间筛选；按余额排序；批量禁用/启用 | 结果正确 | SA `/users` | api:dashboard_users::list_filters_sorting_and_shape、batch_status |
| B-081 | 编辑用户 | — | 改昵称、邮箱、语言、邮箱验证状态、备注、重设密码 | 保存成功；重设密码后该用户的会话失效 | SA | api:dashboard_users::update_user_fields_and_errors |
| B-082 | ⚠️ 钱包调整 | — | 用户详情 → 钱包 → 加款 100 / 扣款 50，填备注 | 余额变化正确，流水有记录；扣款超过余额被拒 | SA `/users/:id` | api:wallet_account::wal_02_admin_adjust_rules_and_audit |
| B-083 | 用户详情各页签 | 有数据 | 订单、支付、优惠券使用、钱包流水 | 数据与前台一致 | SA | api:dashboard_users::coupon_usages_with_scope_products |
| B-084 | 解绑第三方 / 重置用户 2FA | 用户已绑定 | 解绑 Telegram/Google；重置 2FA | 解绑后仍保留至少一种登录方式 | SA | api:dashboard_users::oauth_unbind_rules；api:identity_me::user_2fa_management_and_admin_reset |
| B-085 | 手动设置会员等级 | — | 用户详情 → 会员等级下拉 | 前台价格立即按新等级计算 | SA → S | api:marketing_member_level::level_prices_and_user_levels |
| B-086 | 用户登录日志 | — | 按用户、IP、失败原因筛选 | 与前台登录尝试一致 | SA `/user-login-logs` | api:identity_user_auth::login_errors_and_logs |
| B-087 | 钱包充值记录 | 有充值 | 按状态、渠道、时间筛选 | 与前台一致 | SA `/wallet-recharges` | api:wallet_recharge::create_recharge_shapes_and_queries |
| B-088 | 钱包配置 | — | 勾选充值渠道；开关“仅余额支付” | 前台充值只出现勾选的渠道 | SA `/wallet-config` | api:wallet_recharge::payment_channels_only_list_wallet_channels |
| B-089 | 会员等级 | — | 新建等级（折扣 95%、充值门槛 100）；设默认；回填默认等级 | 前台等级卡和折扣正确；回填后无等级用户获得默认等级 | SA `/member-levels` | e2e:01；api:marketing_member_level::level_crud_contract、backfill_requires_default_level |

### 2.8 营销与内容

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-090 | 优惠券 CRUD | — | 新建：百分比/固定、适用商品、门槛、最高优惠、总次数/每人次数、角色、等级、起止时间 | 见 F-045 | SA `/coupons` | e2e:01；api:marketing_coupon::coupon_crud_contract |
| B-091 | 活动价 CRUD | — | 为单个商品设活动价和时间窗口 | 见 F-046；过期后自动失效 | SA `/promotions` | api:marketing_coupon::promotion_crud_contract |
| B-092 | 礼品卡生成与导出 | — | 生成 10 张（面额 50、有效期）→ 批量停用 → 导出 txt/csv | 状态与导出内容正确；停用的卡不可兑换 | SA `/gift-cards` | e2e:01；api:marketing_gift_card::filters_batch_status_export_and_redeem |
| B-093 | Banner | — | 新建 Banner（桌面图、手机图、内链/外链、时间窗口） | 前台轮播显示，手机端用手机图；过期后隐藏 | SA `/banners` | e2e:01；api:content_posts::banners_admin_and_public_window |
| B-094 | 文章与公告 | — | 新建博客（分类、关联商品）和公告；切换发布/草稿 | 草稿前台不可见；关联商品出现在文章页 | SA `/posts/blog` | e2e:01；api:content_posts::post_crud_publication_and_public_views |
| B-095 | 文章分类 | — | 新建树形分类、停用 | 前台只显示启用分类 | SA `/posts/categories` | api:content_posts::post_categories_tree_status_and_assignment |
| B-096 | ⚠️ 素材库与上传 | — | 上传图片、改名、批量删除；尝试上传 .php / 伪造路径 | 合法图片可用；非法类型和路径穿越被拒 | SA `/media` | api:content_media::upl_03_upload_record_and_delete、upl_03_tampered_path_refused、upl_05_validation_errors |

### 2.9 推广返利（后台）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-100 | 返利设置 | — | 开关、比例、确认天数、最低提现额、提现渠道 | 前台推广面板按新配置显示 | SA `/affiliates/settings` | api:affiliate_admin::setup |
| B-101 | 返利用户 | 有推广用户 | 筛选、停用/启用（单个/批量） | 停用后该推广码不再产生佣金 | SA `/affiliates/users` | api:affiliate_admin::users_list_with_stats_and_status_changes |
| B-102 | ⚠️ 佣金与提现审核 | F-104 | 佣金记录查看；提现审核 → 打款 / 驳回（填原因） | 见 F-104 | SA | api:affiliate_admin::commission_and_withdraw_review、finance_routes_are_compliance_gated |

### 2.10 通知、邮件、Telegram

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-110 | 通知中心配置 | SMTP 或 Telegram Bot 或飞书 | 开启渠道、填收件人、勾选场景（充值成功、订单支付、待人工发货、异常告警）、编辑模板 | 保存成功 | SA `/settings/notifications` | api:notify_center::test_send_and_logs_contract |
| B-111 | 测试发送与日志 | B-110 | 点测试发送 | 收到消息；日志页出现测试记录（is_test） | SA | api:notify_center::test_send_and_logs_contract |
| B-112 | 事件通知去重 | B-110 | 连续触发同一事件 | 在去重窗口内只发一次 | — | api:notify_center::dispatch_job_delivers_and_dedupes |
| B-113 | 低库存告警 | 告警间隔已配置 | 库存跌破阈值 | 每个间隔只告警一次 | — | api:notify_center::alert_check_reports_low_stock_once_per_interval |
| B-114 | Telegram Bot 设置 | 需要 Bot 客户端 | 基础设置、欢迎语、帮助中心、菜单（排序） | 保存成功；运行状态页显示连接信息 | SA `/telegram-bot/*` | api:content_settings::affiliate_templates_notifications_bot |
| B-115 | Bot 客户端（Channel API） | — | 新建客户端 → 复制 key/secret → 重置 secret → 停用 | 停用或重置后，旧凭证调用 Channel API 统一失败 | SA `/telegram-bot/channel-clients` | api:notify_channel_clients::*、api:notify_channel_api::ntf06_channel_auth_failures_are_uniform |
| B-116 | 群发消息 | 有绑定 Telegram 的用户 | 新建群发（全部/指定用户、HTML、附件）→ 查看详情 → 删除 | 状态按 pending → running → completed 变化，成功/失败数正确 | SA | api:notify_broadcast::broadcast_lifecycle |

### 2.11 支付渠道

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-120 | ⚠️ 新建 epay 渠道 | 合规已确认 | 支付渠道 → 新建：名称、图标、epay、alipay、跳转、费率、网关地址、商户 ID、密钥 | 前台结算出现该渠道 | SA `/payment-channels` | e2e:01 `payment channel: epay (v1, redirect)`；api:payment_admin::channel_crud |
| B-121 | ⚠️ 渠道校验与密钥掩码 | — | 缺字段保存；编辑已有渠道时不改密钥 | 缺字段有字段级错误；密钥显示为掩码，保存后不会被清空 | SA | api:payment_admin::channel_validation_errors；api:regression_admin::adm_01_channel_secrets_masked_and_merged |
| B-122 | 微信支付公钥测试 | 有微信商户 | 渠道编辑 → “测试公钥” | 返回校验结果 | SA | api:payment_admin::wechatpay_public_key_test |
| B-123 | 渠道停用 / 排序 / 适用范围 | — | 停用；调整排序；只对会员/指定等级开放；只用于订单或只用于充值 | 前台立即生效 | SA → S | api:payment_admin::channel_changes_refresh_public_config |
| B-124 | 手续费策略 | — | 开启“买家承担手续费” | 见 F-065 | SA | — |

### 2.12 审计与日志

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| B-130 | 权限审计 | 见 B-045 | — | — | SA | api:identity_rbac::roles_and_policies |
| B-131 | 钱包调整审计 | B-082 | 查看用户钱包流水 | 调整记录带操作人和备注 | SA | api:wallet_account::wal_02_admin_adjust_rules_and_audit |
| B-132 | 通知日志 | B-111 | 按渠道、状态、事件类型筛选 | 与实际发送一致 | SA | api:notify_center::test_send_and_logs_contract |

---

## 3. 站点对接

### 3.1 S ← ACG（S 从异次元进货，协议 `acg-faka`）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| I-001 | 建立连接 | ACG 会员 `zebrasupply` 已有 app_key 和余额 | SA 对接管理 → 连接管理 → 新建：协议“异次元发卡”，站点地址 `https://acg.dot2.com`，商户 ID（ACG 用户 ID），对接密钥 → 测试连接 → 保存 | 握手返回店名、余额、币种；能力只有 `categories`；连接变为已激活 | SA `/site-connections` | api:integration_acg_faka_adapter::acg_faka_end_to_end；provision 第 4 步 |
| I-002 | 浏览并导入上游商品 | I-001 | 商品映射 → 导入 → 选连接 → 按分类浏览 → 勾选商品 → 选本地分类 → 导入 | 种类/规格展开为 SKU；拿货价与 ACG 的 valuation 一致；导入后默认下架 | SA `/product-mappings` | api:integration_acg_faka_adapter::acg_faka_end_to_end |
| I-003 | 加价与上架 | I-002 | 连接设加价 20% → “重新应用加价” → 商品上架 | 售价 = 成本 × 1.2（按取整规则） | SA | api:integration_mappings::ups05_rate_change_reprices |
| I-004 | ⚠️ 买家购买 ACG 来源商品 | I-003，buyer 有余额 | S 前台买 ACG 来源商品，用余额支付 | 生成采购单并 fulfilled；买家拿到 `ACG-LAB-…` 卡密；ACG 侧 `zebrasupply` 余额扣款 | S → ACG | **verify:a**；api:integration_acg_faka_adapter::acg_faka_end_to_end |
| I-005 | 上游调价 / 调库存后同步 | I-002 | 在 ACGA 改价、改库存 → SA 商品映射点“同步” | 本地成本、售价、库存随之更新 | ACGA → SA | api:integration_mappings::sync_aligns_skus_prices_and_tiers |
| I-006 | ⚠️ 上游余额不足 | 把 `zebrasupply` 余额改为 0 | S 买家下单 | 采购单 `rejected`，错误“余额不足”，不自动重试；ACG 充值后在 SA 点“重试”→ fulfilled | S → SA | api:integration_acg_faka_adapter::acg_faka_error_paths |
| I-007 | ⚠️ 上游缺货 | ACG 该商品卡密清空 | 同步后查看前台 | 同步后前台显示售罄；开启“下单前库存检查”时下单被拦截，不会扣买家的钱 | S | api:integration_mappings::ups06_pre_order_stock_guard、ups06_stock_sync_job |
| I-008 | ⚠️ 网络错误 → 待人工核对 | 临时让 ACG 不可达（停止 acg 容器，或在请求处理中途断网） | 买家下单 | 结果不明的采购单进入 `manual_review`，**不会重复下单**、不会自动退款；恢复后管理员核对 ACG 订单 → 重试或取消 | S → SA | api:integration_acg_faka_adapter::acg_faka_transport_failures；api:integration_adapter_registry::synchronous_delivery_and_manual_review_are_protocol_neutral |
| I-009 | 人工发货的上游商品 | ACG 有人工发货商品 | 导入并购买 | ACG 返回的提示文案不会被当成卡密；采购单停在 `accepted`，由管理员处理 | S | api:integration_acg_faka_adapter::acg_faka_end_to_end（ACG-04） |
| I-010 | ⚠️ 上游控件正则 | ACG 商品有带正则的账号控件 | S 结算时填不合规账号 | 在 S 结算时被拦截，不扣款（ACG-06） | S | unit:zs-infra `acg_faka::acg06_widget_regex_is_copied_when_portable` |
| I-011 | 上游商品下架 / 删除 | I-002 | ACGA 下架或删除商品 → 同步 | 映射标记为上游已下架/已删除，本地商品自动下架 | SA | api:integration_mappings::ups14_unavailable_and_deleted |

### 3.2 ACG ← S（异次元从 S 进货，`/shared/*`，店铺类型“异次元”）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| I-020 | ⚠️ 生成兼容密钥 | `acg-down@lab.test` 凭证已审核 | S `/me/api` → 异次元 / 萌次元 对接 → 生成 | 见 F-115 | S | api:integration_provider_acg_faka::prv07_credential_and_key_state |
| I-021 | ACG 添加共享店铺 | I-020 | ACGA → 店铺共享 → 添加：类型“异次元(V3.1.2+)”、地址 `https://store.dot2.com`、商户 ID、密钥 → 连接 | 显示店名和余额 | ACGA | provision 第 7 步 |
| I-022 | ACG 拉取与导入商品 | I-021 | 拉取商品 → 导入 `lab-e2e-card`（加价 1） | 只列出自动发货商品（PRV-02）；多 SKU 商品的种类名为 zh-CN 规格值（PRV-08） | ACGA | api:integration_provider_acg_faka::acg_faka_downstream_end_to_end、prv08_localized_spec_value_is_one_race |
| I-023 | ⚠️ ACG 会员下单 | I-022，`acgbuyer` 有 ACG 余额 | ACG 前台购买导入的商品 | ACG 订单立即拿到 `ZS-LAB-…` 卡密；S 上 acg-down 钱包扣款正好等于调用者价 | ACG → S | **verify:e** |
| I-024 | ⚠️ 重放 trade | 抓到一次 trade 请求 | 用同一个 `request_no` 再发 | 返回原订单（`stock` 为字符串），不重复扣款（PRV-03/09） | API | api:integration_provider_acg_faka::acg_faka_downstream_end_to_end |
| I-025 | ⚠️ 签名与 app_id 校验 | — | 错误签名、`app_id[]=1`、`0e…` 形式的签名、改字段后重放 | 一律“密钥错误”或“商户ID不存在”，不出现 500 | API | api:integration_provider_acg_faka::prv01_signature_and_app_id_rules |
| I-026 | ⚠️ S 侧余额不足 / 0 元商品 | acg-down 余额清零；或把商品价格改为 0 | ACG 下单 | 分别返回“余额不足”和“商品价格异常，暂停对接”，不创建订单 | ACG | api:integration_provider_acg_faka::prv05_zero_price_refused |
| I-027 | 限流 | — | 1 分钟内请求超过 300 次 | 返回 JSON“请求过于频繁” | API | api:integration_provider_acg_faka::prv06_rate_limited |
| I-028 | 人工发货商品不开放 | S 有人工发货商品 | ACG 按 code 访问或下单 | “该商品未开放对接”，不扣款 | API | api:integration_provider_acg_faka::acg_faka_downstream_end_to_end |

### 3.3 ACG ← S（萌次元类型，`/plugin/open-api/*`）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| I-030 | 添加萌次元(V4.0)店铺 | I-020（同一个密钥） | ACGA → 店铺共享 → 添加：类型“萌次元(V4.0)”；因为 ACG 不允许同一地址保存两次，地址换一种写法（例如末尾加 `/`，或换大小写） | connect 返回站点名和余额 | ACGA | api:integration_provider_mcy::mcy_open_api_end_to_end |
| I-031 | ⚠️ 导入与下单 | I-030 | 导入单 SKU 与多 SKU 商品 → ACG 会员购买 | `contents` 同步返回卡密；S 钱包扣款正确 | ACG → S | api:integration_provider_mcy::mcy_open_api_end_to_end；docs/protocol/third-party/interop-report.md A8 |
| I-032 | S ← 萌次元商城（采购方向） | 需要一个装了 OpenApi 插件的 mcy-shop（实验环境没有） | SA 新建连接，协议“萌次元商城” | 由集成测试覆盖 | — | api:integration_mcy_adapter::mcy_shop_end_to_end、mcy_shop_error_paths |

### 3.4 S ← DJ（S 从原版 dujiao-next 进货，协议 `dujiao-next`）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| I-040 | DJ 上申请凭证 | DJ 用户 `zebra-supply@lab.test` | DJ 个人中心 → API → 申请 → DJA 审核 → 复制 Key/Secret | 凭证可用 | DJ / DJA | provision 第 3 步 |
| I-041 | 建立连接 | I-040 | SA 新建连接：协议 dujiao-next，地址 `https://dujiao.dot2.com`，Key、Secret、回调地址、汇率、加价 20% → Ping | Ping 成功，连接激活 | SA | api:integration_connections::ping_activates_pending_connection |
| I-042 | 导入商品 | I-041 | 商品映射 → 导入 DJ 商品 → 上架 | SKU、价格正确；没有价格的 SKU 被跳过 | SA | api:integration_mappings::import_creates_inactive_upstream_product、ups08_import_skips_unpriced_skus |
| I-043 | ⚠️ 下单与异步交付 | I-042 | buyer 购买 DJ 来源商品 | 采购单 accepted → DJ 发货后通过回调或轮询（首次轮询在 30 s 后）变为 fulfilled；买家拿到 `DJ-LAB-…` | S → DJ | **verify:b**；api:integration_procurement::poll_and_periodic_sync、supplier_callback_rules |
| I-044 | ⚠️ 回调安全 | — | 伪造回调（错误签名 / 不属于该连接的订单） | 被拒，状态不变 | API | api:integration_procurement::cancel_callback_and_bad_secret |
| I-045 | 对账 | 有采购单 | 对账中心 → 新建任务（状态/金额/全量，时间范围） → 查看不一致项 → 标记已处理 | 任务完成，统计匹配/不匹配数 | SA `/reconciliation` | api:integration_reconciliation::ups21_run_execute_resolve |
| I-046 | DJ ← S（原版把 S 当上游） | S 上有已审核凭证 | DJA 新建连接指向 `https://store.dot2.com`（dujiao-next 协议）→ 导入 → 下单 | S 通过 `/api/v1/upstream/*` 供货，DJ 收到卡密 | DJ → S | api:integration_upstream_api::ping_and_catalog、create_order_validation_and_idempotency；api:integration_downstream::signed_delivery_and_retries |

### 3.5 Z2 ← S（`zebra-store` 协议，全部特性）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| I-060 | ⚠️ 连接码 | `zs2-supply@lab.test` 凭证已审核 | S `/me/api` → 生成连接码 → 复制 `zsc1_…` | 见 F-113 | S | api:integration_zebra_store::zebra_store_end_to_end |
| I-061 | 粘贴连接码 + 握手 | I-060 | Z2A 连接管理 → 新建 → 粘贴连接码 | 自动填好地址、Key、Secret，协议为 zebra-store；握手预填站点名、货币、建议汇率；保存后 `features` 含 changes/webhooks/quote/multi_item/idempotency/encrypted_delivery | Z2A | 同上 |
| I-062 | 握手失败也能保存 | 连接码的地址不可达 | 粘贴并保存 | 记录握手错误，webhook 状态为 failed；连接可以稍后再测 | Z2A | api:integration_zebra_store::failed_handshake_does_not_block_saving |
| I-063 | Webhook 注册 | I-061 | 查看连接详情 | `webhook_status = registered`（Z2 的 `/api/v1/zs/events` 已注册到 S） | Z2A | api:integration_zebra_store::zebra_store_end_to_end |
| I-064 | 全量导入 | I-061 | Z2A 商品映射 → 导入 `lab-e2e-card`，加价 10% | 导入成功，记录 `last_change_seq` | Z2A | 同上 |
| I-065 | ⚠️ 增量同步 + 推送 | I-064 | 在 SA 修改 `lab-e2e-card` 价格或补卡 | S 在 15 s 内产生变更 → 推送 `catalog.changed` → Z2 立即拉取 `/catalog/changes` → Z2 的成本和库存更新，无需等 5 分钟轮询 | SA → Z2A | 同上 |
| I-066 | 变更游标过期 | 长时间未同步（超过 7 天保留期） | 触发同步 | 收到 410 后自动做一次全量，再从全量开始前的 `latest_seq` 继续 | Z2 | api:integration_adapter_registry::*（changes / change_head） |
| I-067 | ⚠️ 报价锁价 | I-064 | Z2 买家下单时 S 刚好涨价 | Z2 先报价锁定单价（10 分钟），按锁定价与当前价中较低的一个成交；报价过期返回 409 quote_expired | Z2 → S | api:integration_zebra_store::zebra_store_end_to_end |
| I-068 | ⚠️ 多商品订单 | Z2 导入了 2 个 S 商品 | Z2 买家一次买两种 | S 生成一个父订单 + 两个子订单；Z2 两行都交付 | Z2 → S | 同上 |
| I-069 | ⚠️ 幂等下单 | — | 同一个 `Idempotency-Key` 重发下单 | 返回同一订单的当前状态，不重复扣款；请求体不同返回 422 idempotency_conflict | API | 同上；api:integration_upstream_api::create_order_validation_and_idempotency |
| I-070 | ⚠️ 加密交付 | I-064 | Z2 买家下单 | S 返回的 `delivery` 是 AES-256-GCM 密文；Z2 解密后买家看到 `ZS-LAB-…`；S 的 zs2-supply 钱包扣款正好等于售价 | Z2 → S | **verify:c** |
| I-071 | ⚠️ 密钥轮换 | I-061 | S 上 zs2-supply 重新生成连接码 → 在 Z2A 更新连接 | 7 天内新旧密钥都能用；Z2 首次用新密钥成功签名后旧密钥立即作废；7 天后自动晋升 | S / Z2A | api:integration_zebra_store::zebra_store_end_to_end（rotation 断言） |
| I-072 | ⚠️ 签名、防重放、限流 | — | 重放同一个 nonce；时间戳偏差超过 300 s；每分钟超过 120 次 | 401 unauthorized（不说明原因）；429 带 Retry-After | API | api:integration_upstream_api::ups10_authentication_failures、ups10_rate_limit |
| I-073 | ⚠️ S 侧余额不足 | zs2-supply 余额清零 | Z2 买家下单 | 报价显示 `sufficient_balance=false`；采购单 rejected（insufficient_balance，不自动重试）；充值后 Z2A 重试成功 | Z2 → S | api:integration_procurement::submit_error_classes |
| I-074 | ⚠️ 上游缺货 | S 的 `lab-e2e-card` 卡密清空 | Z2 同步 → 下单 | 同步后 Z2 显示售罄；已付款的订单报价返回 out_of_stock，采购单 rejected，由管理员退款 | Z2 | api:integration_procurement::submit_error_classes |
| I-075 | ⚠️ 网络错误 → 待人工核对 | 下单途中 S 不可达 | Z2 买家下单 | 采购单进入 manual_review；恢复后管理员按 `downstream_order_no` 核对 S 订单，再重试（幂等键保证不会重复下单）或取消 | Z2A | api:integration_adapter_registry::synchronous_delivery_and_manual_review_are_protocol_neutral |
| I-076 | 管理员重试 / 取消采购单 | 有 failed / rejected / manual_review 采购单 | 采购单管理 → 重试 / 取消；查看详情时间线；下载上游原始数据 | 状态与时间线正确；取消后本地订单按规则处理 | SA / Z2A `/procurement-orders` | api:integration_procurement::admin_queries_and_actions |
| I-077 | ⚠️ 生产环境拒绝内网地址 | `allow_private_addresses=false` | 连接地址填 `http://127.0.0.1:8081` | 被拒（防 SSRF）；不跟随重定向 | SA | api:integration_zebra_store::ups01_production_policy_refuses_private_targets；api:integration_connections::ups01_loopback_supplier_is_refused_in_production |
| I-078 | 对接商品在本站的字段锁定 | 已导入的对接商品 | SA 编辑该商品 | 发货方式等字段被锁定（保持上游） | SA | api:regression_integration::ups_09_mapped_product_fulfillment_type_stays_upstream |

### 3.6 API 凭证管理（后台）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| I-080 | 审核 / 驳回 / 停用 / 删除 | 用户已申请 | SA API 凭证 → 通过 / 驳回（原因）/ 停用 / 删除 | 用户侧状态同步；重新审核会换新 api_key，并使兼容密钥作废 | SA `/api-credentials` | api:integration_credentials::apply_review_and_self_service、ups18_reject_delete_and_reapply |
| I-081 | 待审核凭证不能调用 | — | 用待审核的 Key 调接口 | 403 | API | api:integration_upstream_api::ups18_pending_key_is_refused |

---

## 4. 分站（分销商）

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| R-001 | 申请成为分销商 | `reseller.enabled=true`、允许自助申请 | 新用户登录 S → 个人中心 → 分销 → 申请（填理由） | 状态为待审核；分销控制台只能在主站打开 | S `/reseller/apply` | api:reseller_console::onboarding_flow_and_shapes；api:reseller_tenant::rsl03_console_only_on_main_site |
| R-002 | ⚠️ 审核通过 | R-001 | SA 分销商审核 → 通过：默认加价 10%、最高加价 50% | 状态为已启用 | SA `/resellers/profiles` | api:reseller_admin::rsl03_profile_review_transitions |
| R-003 | 驳回 / 恢复 | R-001 | 驳回（原因）→ 用户重新申请；禁用后恢复 | 状态流转正确 | SA | 同上 |
| R-004 | 分配系统子域名 | R-002 | 分销商详情 → 系统子域名填 `sakura` | `sakura.dot2.com` 可以访问，显示该分销商的站点 | SA → SK | api:reseller_admin::rsl03_system_subdomain |
| R-005 | 自定义域名 | R-002 | 分销控制台 → 域名 → 提交 `shop.example.com` → SA 审核 → 设为主域名 | 状态和校验流程正确（实验环境无真实 DNS，只验证流程） | S → SA | api:reseller_console::rsl03_custom_domains；api:reseller_admin::rsl03_domain_actions |
| R-006 | ⚠️ 未知 Host / 伪造 Host | — | 用未登记的子域名访问；伪造 `X-Forwarded-Host` | 未知站点被拒；只信任受信代理转发的 Host | SK | api:reseller_tenant::rsl06_resolves_hosts_and_rejects_unknown_sites、rsl06_forwarded_host_requires_trust |
| R-007 | 分站配置 | R-004 | 控制台 → 站点：站点名、Logo、Favicon、公告、客服、SEO、页脚链接、导航开关 | 分站前台显示自己的品牌；主题色沿用主站（与原项目一致）；主站不受影响 | S → SK/NE/MA | **verify:d**（public config）；api:reseller_tenant::rsl06_rsl08_public_config_overlay_per_tenant、rsl06_public_config_endpoint_serves_reseller_branding |
| R-008 | 三个分站的差异 | provision 已完成 | 分别打开 SK / NE / MA | 名称（Sakura 樱花小铺 / NEON 霓虹卡屋 / Matcha 抹茶杂货铺）、Logo、公告、SEO 各不相同；分站不显示优惠券输入框和“成为分销商”入口 | SK/NE/MA | verify:d |
| R-009 | ⚠️ 商品加价 | R-002 | 控制台 → 商品 → `lab-e2e-card` 选“按百分比加价 15%” → 预览 → 保存；再试固定加价、固定售价、低于底价、超过最高加价 | 预览价 = 10.00 × 1.15 = 11.50；低于基础价/成本或超过 50% 的规则被拒 | S → SK | api:reseller_console::rsl07_product_settings_pricing |
| R-010 | 隐藏商品 | R-009 | 取消“上架”某商品 | 该商品不出现在分站，主站不受影响 | SK | api:reseller_console::db_04_first_sku_setting_keeps_is_listed_false |
| R-011 | ⚠️ 分站买家下单 | R-009，buyer 有余额 | 在 SK 登录 buyer → 买 `lab-e2e-card` → 余额支付 | 实付 11.50；订单记在主站，买家在主站和分站都能看到 | SK | **verify:d** |
| R-012 | ⚠️ 利润快照 | R-011 | 分销控制台 → 订单 → 详情 | 利润 1.50；买家邮箱已脱敏；只能看到自己分站的订单 | S `/reseller/orders` | verify:d；api:reseller_console::rsl07_rsl10_orders_are_scoped_and_masked |
| R-013 | ⚠️ 结算确认期 | `settlement_confirm_days`（实验环境为 0，生产为 7） | 查看流水 | 利润先是 pending_confirm，确认期结束后的下一次结算任务把它变为 available | S `/reseller/ledger` | verify:d；api:reseller_accounting::rsl05_profit_is_idempotent_and_pending_until_confirmed |
| R-014 | ⚠️ 确认期内退款 | R-011，确认天数 = 7 | SA 给分站订单退款一半 | 流水出现 refund_deduct，按退款比例扣回利润，累计扣回不超过利润 | SA → S | api:reseller_accounting::rsl04_refund_in_confirm_window、rsl01_partial_refunds_never_over_deduct |
| R-015 | ⚠️ 自买自卖不计利润 | 分销商本人在自己分站下单 | 下单 | 不产生利润（RSL-07） | SK | api:reseller_accounting::rsl07_self_dealing_posts_nothing |
| R-016 | ⚠️ 申请提现 | 可用余额 > 0 | 控制台 → 提现：金额、币种、渠道、账号 | 余额被锁定；超过可用余额或余额为负时被拒；并发提交不会超提 | S `/reseller/withdraws` | verify:d；api:reseller_accounting::rsl02_withdraw_checks_net_available、rsl05_concurrent_withdrawals、rsl02_negative_balance_blocks_withdraw |
| R-017 | ⚠️ 管理员打款 / 驳回 | R-016 | SA 分销提现 → 打款；另一笔 → 驳回（原因） | 打款后状态 paid，锁定余额变为已提现；驳回后退回可用余额 | SA `/resellers/withdraws` | verify:d；api:reseller_accounting::rsl05_withdraw_split_and_reject |
| R-018 | 分销流水 / 余额 / 概览 | 有数据 | SA 分销流水、分销余额、分销概览 | 各项汇总与实际一致；负余额账户有标记 | SA | api:reseller_admin::operations_overview_and_finance |
| R-019 | 管理员代改分站配置与商品规则 | — | SA 分销站点配置 → 编辑 / 重置；分销商品配置 → 编辑 / 重置 | 分站前台随之变化 | SA → SK | api:reseller_admin::site_configs_admin、product_settings_admin |
| R-020 | ⚠️ 禁用分销商 | 分销商已启用 | SA → 禁用（原因） | 分站域名立即不可用；控制台只读或不可用；恢复后重新可用 | SA → SK | api:reseller_tenant::rsl03_disabled_profile_domain_is_unavailable |
| R-021 | ⚠️ 冻结结算 | — | SA 编辑分销商 → 结算状态改为 frozen | 冻结期间不能提现 | SA | api:reseller_accounting::rsl02_dashboard_and_guards |
| R-022 | 关闭分销功能 | `reseller.enabled=false` 并重启 | 访问 SK | 所有 Host 都显示主站 | 服务器 | api:reseller_tenant::feature_disabled_means_main_shop_everywhere |

---

## 5. 运维

| ID | 流程 | 前置条件 | 步骤 | 期望结果 | 站点 | 自动化覆盖 |
|---|---|---|---|---|---|---|
| O-001 | 一键部署 | 本机有 Docker、Node、zig | `deploy/deploy.sh root@<服务器>` | preflight 通过 → 构建 → 上传 → 启动 → 配置主机 Caddy → provision → verify 全部 PASS | 本机 → 服务器 | verify:a–e |
| O-002 | 状态 / 日志 | 已部署 | `lab.sh ps`；`lab.sh logs store`；`TAIL=500 lab.sh logs zs2` | 容器健康；日志是结构化 tracing 输出 | 服务器 | — |
| O-003 | ⚠️ 备份（SQLite） | 已部署 | 用 `zebra-store --config config.yml backup --output /backup/zebra-$(date +%F).db` 在线备份（Docker：`docker compose exec zebra zebra-store --config /app/config.yml backup --output data/backup/x.db` 后 `docker compose cp`），同时备份 `uploads/` 和 `config.yml` | 备份文件可作为数据库启动；有 `sqlite3` 时 `PRAGMA integrity_check` 输出 ok | 服务器 | infra:backup::qa_a16_sqlite_online_backup |
| O-004 | ⚠️ 恢复 | O-003 | 停服务 → 用备份替换 `zebra.db` 和 `uploads/` → 启动 | 数据回到备份时刻；登录、订单、卡密正常 | 服务器 | — |
| O-005 | ⚠️ 切换 PostgreSQL | 有空库 `zebra` | 改 `database.url=postgres://…` 或设 `ZS__DATABASE__URL` → 启动 | 自动建表、建内置角色和超管；**原 SQLite 数据不会自动迁移**（新库是空的） | 服务器 | scripts/db_smoke.sh；`ZS_TEST_DATABASE_URL=postgres://… cargo test -p zs-api` |
| O-006 | ⚠️ 切换 MySQL | MySQL 8+ 空库 | 同上，URL 为 `mysql://…` | 同上 | 服务器 | 同上 |
| O-007 | 升级 | 新版本二进制 / 镜像 | 备份 → 替换二进制或 `deploy.sh`（会跳过没变的镜像） → 重启 | 启动时自动增量同步表结构并执行数据迁移；业务数据保留 | 服务器 | — |
| O-008 | ⚠️ 配置校验 | — | 三个密钥留空、太短（< 16 位）或相同；`trusted_proxies` 填 `0.0.0.0/0` | 服务拒绝启动，并提示是哪个配置项 | 服务器 | unit:zs-app `config::tests` |
| O-009 | 环境变量覆盖 | — | `ZS__SERVER__PORT=9000`、`ZS__LOG__LEVEL=debug` | 生效，优先于 config.yml | 服务器 | — |
| O-010 | 重置实验环境 | — | `lab.sh reset && lab.sh all` | 全部重建并 verify PASS；`.env` 中的密钥保留 | 服务器 | verify:a–e |
| O-011 | 卸载 | — | `deploy.sh root@<服务器> --uninstall` | 只删除 `zebra-lab` 前缀的容器、卷、网络和 Caddy import 行；主机其他服务不受影响 | 服务器 | — |
| O-012 | 文档站 | handbook 已构建 | 访问 DOC | 首页、搜索、侧边栏正常；截图占位图位置正确 | DOC | `cd handbook && npm run docs:build` |

---

## 6. 统计

| 分组 | 流程数 | ⚠️ 高风险 |
|---|---|---|
| 1 前台 | 87 | 29 |
| 2 后台 | 82 | 19 |
| 3 对接 | 51 | 25 |
| 4 分站 | 22 | 12 |
| 5 运维 | 12 | 5 |
| **合计** | **254** | **90** |

> 流程数按表格行计数。编号之间故意留了空号，方便以后插入新流程。
