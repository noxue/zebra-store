# TODO

图例：`[ ]` 未开始 · `[~]` 进行中 · `[x]` 已完成并验证（编译 + 测试通过）

## M0 基础设施
> 已完成：60 张表由 GORM schema 自动生成 sea-orm 实体（`backend/scripts/gen_entities.py`），entity-first 同步已在 SQLite 验证；分类模块作为参考实现打通 entity→repo→service→api→集成测试。
- [x] backend workspace：7 个 crate、workspace lints/deps、rustfmt/clippy 配置
- [x] zs-shared：Amount(Decimal 字符串序列化)、LocalizedText、Pagination、serial(订单号)、AES-GCM、HMAC 签名、Clock
- [x] 配置加载（config.yml + 环境变量覆盖，三密钥校验）、tracing 日志
- [x] 数据库连接（sqlite/mysql/postgres 按 URL 选择，sqlite WAL）、zs-migration 框架
- [x] zs-api：响应信封、ApiError、i18n 消息表（zh-CN/zh-TW/en-US）、X-Lang 解析、RequestId、CORS、/health
- [x] DB 任务队列：jobs 表、入队/延迟/重试、worker、定时任务调度
- [x] 缓存与限流（进程内 RateLimiter、public config / 仪表盘 / 租户缓存；Redis 可选未接入）
- [x] storefront 脚手架：Vite + Vue3 TSX + Tailwind v4 + Pinia + Router + i18n + API client
- [x] admin 脚手架：同上
- [x] 二次元设计系统（两端各一份）：theme.css token、按钮/输入/卡片/徽章/弹窗/表格/分页/Toast/Confirm/Select/Switch/Tabs

## 并行分工
- 后端组开发中：identity、content、catalog+marketing、payment(渠道/网关)、dashboard、reseller、notify。
- 已完成：identity、content、catalog+marketing、reseller、dashboard、payment(渠道/网关/回调)。全部后端组已完成（order 最后完成）。notify、wallet+affiliate、第三方登录已完成。
- 全部后端组均已启动。
- 前端：storefront、admin 均已完成（对接原 Go 后端验证）；待 Rust 后端完成后联调复测。
- [x] 原项目 bug 修复史分析 → docs/reference/bugfix-lessons.md（215 条教训、217 个必测用例，第 21 节回归清单）。

## M1 身份与权限
> identity agent 已完成（34 个集成测试，标注 AUTH/ADM/RISK/DB 教训编号）；全工作区门禁已在 M12 与 M14 复跑。Telegram/Google 登录见 M8。
- [x] 迁移：admins, users, user_oauth_identities, email_verify_codes, user_login_logs, admin_login_logs, casbin_rule, authz_audit_logs, settings
- [x] 管理员登录 / JWT(HS256, token_version) / 默认管理员引导 / 登录限流
- [x] TOTP 2FA：setup/enable/disable/恢复码/挑战 token（管理员 + 用户）
- [x] Casbin RBAC：模型、内置 6 角色种子、权限目录（由路由表生成）、/admin/authz/* 全部接口、审计日志
- [x] 用户注册/登录/忘记密码/邮件验证码（SMTP，lettre）
- [x] 验证码：图形验证码 + Turnstile，按场景开关
- [x] /me 系列：资料、改密、改邮箱、登录日志
- [x] 合规确认（compliance status/acknowledge + 中间件）
- [x] CLI：admin list-admins / reset-password / reset-2fa

## M2 设置与内容
> content agent 已完成（22 个集成测试，覆盖 SET-01~04、UPL-01/03/04/05、DB-01）；自定义回调路由和公告 HTML 前端净化已在后续阶段完成。
- [x] settings 服务 + 20 个 key 的默认值与规范化 + 专用设置接口（smtp/captcha/telegram-auth/google-auth/affiliate/order-email-template/notification-center/telegram-bot）
- [x] /public/config（含缓存、server_time、tenant、theme 扩展）
- [x] 上传（校验类型/尺寸/像素）+ media 表 + 素材库接口 + /uploads 静态服务
- [x] 商品分类（树，已完成）、文章、文章分类、文章关联商品、Banner：admin CRUD + public 接口

## M3 商品目录
> catalog agent 已完成（45 个测试，覆盖 ORD/DLV/PRC 教训；暴露 CatalogOrdering、CouponLedger、GiftCardRedeemer、会员升级钩子给 order/wallet 组）。分销租户目录覆盖和 MySQL/PostgreSQL 并发验证已在 M6/M12 完成。
- [x] 迁移：categories, products, product_skus, member_level_prices, card_secrets, card_secret_batches
- [x] 商品 admin CRUD / PATCH / 批量操作 / 批发价
- [x] 库存策略（manual/auto/upstream、展示模式 exact/status/range/hidden）
- [x] public 商品列表/详情（促销价、会员价、批发价、库存状态）
- [x] 卡密：批量添加、CSV/TXT 导入、列表、统计、批次、状态、导出、导出可用并删除

## M4 交易核心
> order agent 已完成（20 个端到端测试 + 单测；覆盖 ORD/PAY/PRC/DLV/RFD/RISK/UPS 相关教训；结算接入钱包充值、推广、分销、上游采购）。
- [x] 迁移：cart_items, orders, order_items, order_refund_records, order_risk_lock_keys, fulfillments, payment_channels, payments, coupons, coupon_usages, promotions
- [x] 定价引擎（活动价 → 批发价取低 → 会员价 → 优惠券分摊）+ 单测
- [x] 订单风控（IP 黑名单、待支付上限、限流）
- [x] 订单创建（父子单、卡密预留、库存预留、优惠券占用）、预览、列表、详情、取消、统计
- [x] 游客订单（Guest 认证头）
- [x] 支付：渠道 CRUD、可用渠道筛选、手续费策略、创建支付、钱包抵扣、capture、latest
- [x] 支付网关：epay、epusdt、bepusdt、tokenpay、okpay、dujiaopay、alipay、wechat v3、paypal、stripe（114 测试，与 Go 字节级比对，覆盖 PAY 43 条教训）
- [x] 统一回调入口 + webhook + 自定义回调路由
- [x] 支付成功分发：订单状态、自动发货任务、通知、会员升级、返利、分润
- [x] 超时取消任务（释放卡密/库存/优惠券/钱包）
- [x] 发货：自动发卡、人工发货、发货内容下载
- [x] 退款：退回钱包、手动退款、记录、订单状态
- [x] 订单邮件模板与发送任务
- [x] 优惠券、活动价 admin CRUD

## M5 用户资产
- [x] 钱包：账户、流水、充值单、充值支付、超时过期、admin 调账（网关回调入账由 order agent 接入 recharge_settlement）
- [x] 礼品卡：生成、列表、编辑、批量状态、导出、兑换（事务，并发只成功一次）
- [x] 会员等级：CRUD、默认等级、backfill、自动升级、会员价
- [x] 推广返利：开通、点击、佣金计算、确认任务、提现与审核、admin 接口

## M6 分销商
> reseller agent 已完成（41 domain + 10 app + 30 集成测试，覆盖 RSL-01~10）。下单侧（快照、分润记账、退款扣回、禁用优惠券）已由 order 组接入；MySQL 唯一索引方案已在 M12 验证。
- [x] 迁移：reseller_* 9 张表
- [x] 申请/审核/禁用/恢复、系统子域名、自定义域名验证
- [x] 租户中间件（Host 解析 + 缓存）与 public/config 覆盖
- [x] 站点配置、商品上架与加价规则、预览
- [x] 分润快照、账本、结算确认任务、余额账户、提现
- [x] 分销运营概览/财务统计

## M7 对接
> integration agent 已完成（37 集成 + 42 单元测试，覆盖 12 条高严重度 UPS 教训）。上游下单 UpstreamOrdering 与采购交付 ProcurementLifecycle 已由 order 组实现并通过联调回归。
- [x] API 凭证：申请/审核/启停/重置
- [x] 上游 API（/api/v1/upstream/*，HMAC 鉴权）
- [x] 站点连接（加密密钥、ping、加价）、商品映射导入/同步、库存同步任务
- [x] 采购单：提交、轮询、回调、重试、取消
- [x] 下游回调任务
- [x] 对账任务与明细处理

## M8 Telegram 与通知
- [x] 通知中心：邮件/Telegram/飞书、场景模板、去重、日志、测试发送、告警检查任务
- [x] Channel API（/api/v1/channel/*）+ channel_clients 管理（含订单/支付/钱包/推广渠道端点）
- [x] Telegram Bot 配置、运行状态、群发任务（与原项目一致不提供群发 DELETE）
- [x] Telegram 登录（widget/OIDC/MiniApp）与绑定；Google 登录与绑定（25 个测试，覆盖 AUTH-01~05/07，签名与 Go 向量一致）

## M9 仪表盘与杂项
- [x] 仪表盘 overview/trends/rankings/inventory-alerts
- [x] 用户管理 admin 接口（列表、详情、批量状态、优惠券使用、解绑、重置 2FA；钱包归 wallet 组）
- [x] sitemap.xml / robots.txt、系统版本接口
- [x] 广告位接口（返回空实现）

## M10 用户前台 UI（storefront）
> 已完成并先后对接原 Go 与 Rust 后端验证；Rust 端到端复测见 M12，接续审计后为 166 个前台单测。
- [x] 布局：Navbar、Footer、MobileBottomNav、BackToTop、公告弹窗、主题/语言切换、看板娘与樱花特效
- [x] 首页（Banner 轮播、精选商品、最新动态；卡片/列表模式）
- [x] 商品列表 / 分类 / 商品详情 / 快速购买
- [x] 购物车 / 结算（人工表单、优惠券、游客模式、验证码、钱包抵扣、渠道选择）
- [x] 支付页（二维码、跳转、加密货币、轮询、结果）
- [x] 订单详情 / 游客查单 / 充值单详情
- [x] 博客 / 公告 / 文章详情 / 关于 / 服务条款 / 隐私政策 / 404
- [x] 登录（含 2FA）/ 注册 / 忘记密码 / Telegram 与 Google 回调
- [x] 个人中心：概览、订单、钱包、推广、礼品卡、安全、API、资料
- [x] 分销商控制台：概览、申请、域名、站点、商品、订单、财务、流水、提现

- [x] 打磨：首页 Hero 加入看板娘（theme.mascot_image / 内置 SVG）
- [x] 修正 src/api/reseller.ts：product-settings 列表类型与 keyword/configured 过滤参数

## M11 管理后台 UI（admin）
> 已完成并先后对接原 Go 与 Rust 后端验证；Rust 端到端复测见 M12，接续审计后为 237 个后台单测。
- [x] 登录（验证码、2FA）、布局（侧边栏按权限过滤、搜索、折叠、主题、语言）、403、合规确认
- [x] 仪表盘
- [x] 商品：分类、商品（编辑弹窗、SKU、人工表单构建器、富文本）、卡密库存/导入/导出、批发价
- [x] 订单：列表、详情、发货、退款、退款记录、风控
- [x] 支付：渠道（按网关的配置表单）、支付记录、回调路由
- [x] 用户：列表、详情、钱包充值、钱包配置、登录日志、会员等级
- [x] 内容：文章、文章分类、Banner、素材库、公告
- [x] 营销：优惠券、活动价、礼品卡
- [x] 推广返利：设置、用户、佣金、提现
- [x] 分销商：概览、审核、详情、域名、站点配置、商品配置、流水、余额、提现
- [x] 对接：站点连接、商品映射、采购单、对账、API 凭证
- [x] Telegram Bot：概览、设置、帮助中心、菜单、状态、客户端、群发
- [x] 系统：站点设置 13 个标签页（含主题自定义）、通知中心、权限管理、权限审计、安全设置
- [x] 打磨：登录页语言选择框显示语言名称（简体中文/繁體中文/English）而非 zh-CN

## M12 联调与验收
- [x] 全工作区门禁：cargo fmt / clippy -D warnings 全绿；SQLite 全量 870 通过（唯一失败为测试时间竞态，已修）
- [x] bugfix-lessons.md 第 21 节回归清单：91 条“高”全部有测试并通过；中 68/85、低 24/39 覆盖（docs/reference/regression-coverage.md）；修复 3 个真 bug（示例密钥可启动、信任全网代理、导航 javascript: XSS）；中项后续补测至 77/85（剩余均为前端交互类）；参数校验提示 99 个接口与原版一致；HTTP 连接级超时（header 10s/1MB/idle 120s）
- [x] 契约比对脚本（原项目 vs 新项目）并修复差异（`backend/scripts/contract-diff/`，报告 `docs/reference/contract-diff-report.md`）
- [x] Playwright E2E（e2e/，14 个用例全过，前端对接 Rust 后端；修复 SQLite 多连接锁导致不发货、支付渠道缓存不刷新）：游客下单→回调→发卡；会员下单/充值/礼品卡；后台 CRUD
- [x] MySQL / PostgreSQL 验证：冒烟脚本三库通过；全量集成测试 SQLite 898 / PG 295 / MySQL 315 通过，复测剩余失败项均已修复（含 MySQL 风控锁 `do_nothing` 生成非法 SQL → 开启风控后游客无法下单）。修复的真 bug：sea-orm 二次同步删自定义唯一索引导致重启失败；PG 搜索 `?` 占位符语法错误；MySQL 时间列秒级精度导致改密/2FA 后新令牌被判失效（改 DATETIME(6)，按表合并 ALTER）；MySQL `ON DUPLICATE KEY` + FOUND_ROWS 导致幂等插入失效（insert_if_absent）
- [x] 新旧项目截图逐页比对：88 页全部对照（docs/reference/ui-comparison.md，新截图 docs/reference/screenshots-new/），修复 12 页信息结构差异（前台 4、后台 8），E2E 14/14 仍通过
- [x] README：部署、配置、切换数据库、构建、测试（README.md）

## M13 zebra-store 对接协议
> 协议规范 `docs/protocol/zebra-store-v1.md`（已按实现更新），请求/响应示例 `docs/protocol/zebra-store-v1-examples.md`（端到端测试捕获）。采购方按适配器模式实现（`docs/BACKEND_GUIDE.md` §9）。
- [x] 共享原语 `zs_shared::zs`：规范化签名（HMAC-SHA256 + SHA-256 摘要 + nonce，固定向量单测）、交付加密 AES-256-GCM（往返/篡改单测）、连接码 `zsc1_` 编解码
- [x] 适配器架构：协议无关端口 `SupplierAdapter` / `UpstreamClient` + 能力标志（统一能力 ID）、`AdapterRegistry`（dujiao-next / zebra-store 两个适配器各一行注册）、核心服务（同步、导入、采购、入站事件）不含协议分支；测试内注册的第三个内存适配器证明核心与协议无关
- [x] 供货方 `/api/v1/zs/*`：签名 + nonce 防重放（数据库，多实例共享）、双密钥轮换（首次使用新密钥晋升 / 7 天自动晋升，dujiao-next 接口同样生效）、握手、分类、商品游标列表/单品（含 `version`）、变更流（15 s 快照比对任务，独立端口读取商品/SKU/卡密可售库存）、Webhook 注册 + 签名事件出站与重试（30 s/2 min/10 min/1 h/6 h）、报价锁价（复用订单组定价）、多商品幂等下单（`Idempotency-Key` + `downstream_order_no`，父子订单，钱包扣款）、查单/列表/取消、加密交付、订单状态事件（IntegrationOrderEvents 钩子）、`account.balance_low`、统一错误对象与 HTTP 状态码、每 Key 120 次/分钟限流
- [x] 采购方：`ZebraStoreClient`（`protocol = "zebra-store"`）、建连/修改/测试时握手并保存能力、结算货币、同步方式、Webhook 状态（失败不阻止保存）、变更游标驱动的增量同步（无游标或 410 回退全量）、自动注册 `/api/v1/zs/events`、报价 + `Idempotency-Key = procurement:{id}` + 交付解密后走原履约、事件接收（`ZS-Key` 找连接、验签、事件 id 去重、状态机与所属校验 UPS-02/03）
- [x] 新表（`entity/extra/`）：`api_credential_rotations`、`api_request_nonces`、`zs_change_log`、`zs_catalog_snapshots`、`zs_webhooks`、`zs_webhook_events`、`zs_quotes`、`zs_order_requests`、`integration_connection_states`、`integration_processed_events`（唯一键均由实体声明，无迁移自定义索引）
- [x] 前端接口：`GET /api-credential`（`rotation_pending`/`rotation_expires_at`/`protocols`）、`POST /api-credential/connection-code`、`POST /api-credential/rotate`；管理端 `GET /admin/site-connections/protocols`、`POST …/parse-code`、`POST …/handshake`，连接列表/详情含 `protocol`/`features`/`capabilities`/`supplier_currency`/`sync_mode`/`last_change_seq`/`webhook_status`/`handshake_error`/`extra`；RBAC 种子与三语消息已补
- [x] 配置 `integration.allow_private_addresses`（默认 false，环境变量 `ZS__INTEGRATION__ALLOW_PRIVATE_ADDRESSES`，启用时启动告警；两种模式都不跟随重定向）
- [x] 测试：两个实例同进程端到端（连接码→握手→导入→改价/补货/删除→变更流+Webhook→映射更新；采购→报价→下单→自动发卡→加密交付事件→本地交付；幂等重试、402、409 quote_expired、422 idempotency_conflict、nonce 重放 401、密钥轮换与到期晋升、410 回退全量、事件重复投递去重）、SSRF 生产策略拒绝、私网开关与重定向、握手失败仍保存、第三适配器；SQLite / PostgreSQL / MySQL 均通过

## M14 接续审计（2026-09-27）
- [x] 核实历史 QA 与未覆盖回归项，修复旧购物车未知库存被当成零库存的问题，补齐价格刷新、库存、游客存储受限测试。
- [x] 补齐 regression-coverage.md 中全部缺失/部分覆盖项并按实际行为更新审计：中严重度 83/85 覆盖（2 项 N/A），低严重度 38/39 覆盖（1 项 N/A），其余无缺失或部分覆盖。
- [x] 完成最终门禁与三库回归；修复 E2E 新建数据库仍复用旧状态文件的问题，并在独立前端代理下复跑 Playwright 14/14 通过。

## M15 文档与发布（2026-09-27）
- [x] 清查源码、文档与 Git 身份，移除旧的个人姓名与邮箱；仓库提交身份设为 `noxue` 的 GitHub noreply 地址。
- [x] 将 89 张已验收的真实应用截图整理进用户手册；6 张依赖独立宝塔面板的截图明确列为外部环境采集项，正文不再产生损坏图片。
- [x] 构建并检查手册、发布到服务器，验证线上文档、实验环境与 7 条跨站订单链路。
