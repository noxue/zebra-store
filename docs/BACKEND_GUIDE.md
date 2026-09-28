# 后端模块开发指南（给每个模块的实现者）

先读 `CLAUDE.md`。本文件说明**如何按统一模式实现一个业务模块**，以及多人（多 agent）并行时的文件归属规则。

## 1. 参考实现

分类（category）+ 管理员认证/RBAC 是完整的参考切片，照抄其结构：

| 层 | 参考文件 |
|---|---|
| domain 模型 + 端口 | `crates/domain/src/catalog/category.rs`、`crates/domain/src/identity/admin.rs` |
| app 服务 + mock 单测 | `crates/app/src/catalog/category.rs`、`crates/app/src/identity/admin_auth.rs` |
| infra 仓储 | `crates/infra/src/db/repo/catalog/category.rs`、`.../identity/admin.rs` |
| 组装 | `crates/infra/src/wire/catalog.rs`、`wire/identity.rs` |
| API 路由 | `crates/api/src/routes/catalog.rs`、`routes/identity.rs` |
| 集成测试 | `crates/api/tests/catalog_category.rs`、`tests/admin_auth.rs`（`tests/common/mod.rs` 为测试夹具） |

## 2. 模块分组与文件归属（并行开发时只改自己组的文件）

组：`identity, catalog, content, marketing, order, payment, wallet, affiliate, reseller, integration, notify, dashboard`

每组拥有：
- `crates/domain/src/<group>/**`
- `crates/app/src/<group>/**`（其中 `mod.rs` 的 `<Group>Services` 结构体由本组增字段）
- `crates/infra/src/db/repo/<group>/**`（需要时新建目录，并在 `db/repo/mod.rs` 加一行 `pub mod <group>;`）
- `crates/infra/src/wire/<group>.rs`（`build` 组装服务、`jobs` 注册后台任务）
- `crates/api/src/routes/<group>.rs`（可改为目录 `routes/<group>/mod.rs`）
- `crates/api/tests/<group>_*.rs`

**工作区必须始终可编译**：多个 agent 共用同一源码树。声明 `pub mod x;` 前先创建该文件（空 stub 也行），不要留下缺失模块或半截语法，否则会阻断所有人的构建。

**共享文件**（`lib.rs` 的 mod 列表、`services.rs`、`wire/mod.rs`、`routes/mod.rs`、`Cargo.toml`）已经预先登记好所有组；
如确需修改，只做**最小追加**（加一行），不要重排/重格式化别人的代码。
新增 workspace 依赖时在 `backend/Cargo.toml` 的 `[workspace.dependencies]` 末尾追加一行。

## 3. 数据表与 JSON 形状

- 60 张表的实体已由 `scripts/gen_entities.py` 从原项目 GORM 结构生成：`crates/infra/src/db/entity/<table>.rs`（**不要手改**）。
  列名与原项目一致（如多语言字段为 `title_json`、`name_json`）。
- 原项目每个字段的 **JSON 名**（响应形状）见 `docs/reference/schema.json`（`Fields[].JSON`，`-` 表示不输出，`omitempty` 表示空值省略）。
  domain 模型的 serde 输出必须与之一致。
- 额外表放 `crates/infra/src/db/entity/extra/`（已有 `casbin_rule`、`jobs`）。
- **迁移创建的唯一索引必须登记到 `zs_migration::custom_unique_indexes()`**：sea-orm 的 schema sync 会删除实体未声明的唯一索引（MySQL/PostgreSQL 上还会因语句错误导致二次启动失败），登记后会在 sync 前删除、sync 后由迁移重建。多数据库回归：`scripts/db_smoke.sh` 与 `ZS_TEST_PG_URL`/`ZS_TEST_MYSQL_URL` 下的 `crates/infra/tests/multi_db.rs`。

## 4. 与原项目保持一致的规则

- 路由路径、方法、请求字段、响应字段、错误 i18n key 以原项目源码为准：
  原项目源码 `…/scratchpad/dujiao-next/internal/modules/<module>/`（`transport/http/*_handler.go` 看请求/响应与错误 key，`application/*.go` 看业务规则）。
- 业务错误用 `zs_domain::Error::{bad_request,not_found,unauthorized,forbidden,too_many}(key)`，
  key 与原项目 `ginutil.RespondError(c, code, "error.xxx", …)` 完全相同；500 类错误用 `.or_internal("error.xxx_failed")` 保留原 key。
  消息表 `crates/api/src/i18n/messages.json`（原项目导出，勿删改；确需新增 key 时三种语言都加）。
- 金额：`zs_shared::money::Amount`（请求可接受数字或字符串，响应为 `"12.30"`）。
- 多语言：`zs_shared::i18n::LocalizedText`。
- 分页：`zs_shared::page::{PageRequest, Pagination, Page}`，响应用 `response::Paged(items, pagination)`。
- 时间：UTC `DateTime<Utc>`；原项目用 `2006-01-02T15:04:05Z07:00` 格式化的字段用 `routes::identity::rfc3339`。
- **“不存在才插入”一律用 `support::insert_if_absent`**：sea-orm 的 `OnConflict::column(..).do_nothing()` 在 MySQL 上生成非法 SQL；`do_nothing_on([pk])` 在 MySQL（sqlx 开启 FOUND_ROWS）上重复时也报告影响 1 行，幂等判断会失效。
- **并发事务重试**：可能被选为死锁牺牲者的事务（下单等）用带重试的事务包装（`support::is_transient` + `MAX_TXN_ATTEMPTS`，见 `repo/order/mod.rs` 的 `in_txn!`）。
- **原生 SQL 占位符**：sea-query 的 `Expr::cust_with_values` / `Statement::from_sql_and_values` 不转换占位符，必须按后端选择：PostgreSQL 用 `$1..$n`，MySQL/SQLite 用 `?`（原样的 `?` 会让 PostgreSQL 报语法错误）。优先用 sea-orm 查询构建器，原生 SQL 需加三方言 SQL 生成单测。
- 软删除：查询一律过滤 `deleted_at IS NULL`，删除写 `deleted_at`。

## 5. 必须吸取原项目 bug 修复的教训

原项目完整 git 历史：`/private/tmp/claude-501/-Volumes-KINGSTON-codes-rust-zebra-store/24fd5e70-eecd-446c-8cc3-d3c8ef96339a/scratchpad/dujiao-history`。
- 实现一个模块前，先执行 `git log --no-merges --format='%h %ad %s' --date=short -- internal/modules/<module>` 查看该模块历史，
  对 `fix`/`修复`/`security` 类提交用 `git show <hash>` 阅读 diff 和新增测试，把对应场景写成我们的测试。
- 汇总文档：`docs/reference/bugfix-lessons.md`（生成中/已生成时必须对照其中本模块的 "必测用例"）。
- 在测试名或注释中注明对应的清单编号（如 `// ORD-03`）。

## 6. 跨模块协作

- **消费方定义端口**：A 模块需要 B 模块的数据时，在 A 的 domain 定义所需的最小端口，在 A 的 infra 实现（可直接读 B 的表）。
  不要让 A 的 app 服务直接依赖 B 的 app 服务，除非是纯查询且 B 已完成。
- **原子性**：跨多表的写操作（下单扣库存、支付回调、退款、钱包）放进 infra 的单个事务实现（一个粗粒度端口方法），
  domain/app 负责计算"计划"（金额、状态迁移），infra 负责原子落库并用条件更新防并发。
- **事务纪律（DB-01，原项目修过 4 次）**：事务内的所有读写必须使用同一个事务句柄（`&txn`），禁止在事务中途再用 `DatabaseConnection` 查询——SQLite 单连接池会死锁；
  需要被事务复用的仓储方法接受 `&impl ConnectionTrait` 参数。集成测试一律用 `max_connections = 1` 跑，能自动暴露这类问题。
- **并发扣减**：库存、卡密、优惠券次数、余额一律"条件 UPDATE + 检查 rows_affected"，不得先读后写（ORD-03/06、DLV-01/03、PRC-01）。
- **异步副作用**：通过任务队列 `zs_domain::queue::{JobQueue, NewJob, kinds}` 解耦（例如支付成功后投递 `order:auto_fulfill`）；
  处理器实现 `JobHandler`，在 `wire/<group>.rs::jobs` 注册；周期任务用 `registry.every(kind, interval)`。
- **设置**：通过 `WireCtx.settings`（`SettingsStore` + `SettingsExt::typed`）读取自己关心的 key；
  设置的完整校验/默认值/`/public/config` 由 content 组实现。
- **加密**：`WireCtx.cipher`（与原项目 AES-GCM 兼容）；签名 `zs_shared::sign`（与原项目 HMAC 兼容，已有 Go 向量测试）。
- **当前用户**：用户鉴权中间件 `middleware::auth::require_user` 由 identity 组实现；在此之前用户路由返回 401。
  用户路由放 `RouteSet.user`，管理端放 `RouteSet.admin`（自动走 JWT + RBAC）。

## 7. 管理端路由与 RBAC

- `RouteSet.admin` 中注册的每条路由会自动进入权限目录（`App.admin_permissions`）。
- 新增管理端路由时，对照原项目 `internal/authz/bootstrap.go`，确认 `crates/domain/src/authz/builtin_roles.json` 已包含该路由（该 JSON 就是原项目角色矩阵，通常已包含）。

## 8. 验证（每完成一个子功能都要跑）

为避免多人同时编译互相等锁，每个 agent 使用自己的 target 目录：

```
cd backend
export CARGO_TARGET_DIR=target-<group>
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

集成测试要覆盖：成功路径的响应形状（字段名/类型/金额字符串）、每个业务错误的 status_code 与 msg、权限（未登录 401、无权限 403）、
以及 bug 修复教训中的边界与并发场景。

## 9. 新增一个对接系统（供货方适配器）

站点对接（采购方）按适配器模式实现：核心服务（同步、导入、采购、入站事件）只依赖协议无关端口，
**禁止**在适配器之外出现 `if protocol == "…"` 分支。现有适配器：`dujiao-next`（`crates/infra/src/integration/client.rs`）、
`zebra-store`（`crates/infra/src/integration/zebra_store.rs`，协议见 `docs/protocol/zebra-store-v1.md`）、
`acg-faka`（异次元发卡「店铺共享」`/shared/*`，`crates/infra/src/integration/acg_faka.rs`，协议见 `docs/protocol/third-party/acg-faka.md`）、
`mcy-shop`（萌次元 OpenApi 插件 `/plugin/open-api/*`，`crates/infra/src/integration/mcy_openapi.rs`，协议见 `docs/protocol/third-party/mcy-shop.md`）。
两个 lizhipay 系统共用 `php_form.rs`（PHP 表单编码与 `Str::generateSignature` md5 签名）。它们只声明 `categories`：
下单不可幂等且无锁价，传输失败的 `trade` 由适配器按 `upstream_result_unknown`（`procurement::RESULT_UNKNOWN`，不重试）转人工核对
（acg-faka 靠确定性 `request_no` 让重试被对方去重拒绝，萌次元直接不重试），教训见 `bugfix-lessons.md` §22（ACG-*/MCY-*）。
核心对 `upstream_result_unknown` 以及 `CreateOrderResponse.review`（已扣款但需人工，如付款后无货）一律置采购单 `manual_review`：
**不回退、不退款**，告警管理员；管理端「重试」（已知供货方单号 → 回到 accepted 重新查单；否则用同一请求号重新提交）或「取消」（= 标记失败 → 回退本地订单）。
下单同步返回的交付（acg-faka `secret`、萌次元 `contents`）放进 `CreateOrderResponse.fulfillment`，核心与供货方单号**同一事务**写入 `procurement_deliveries` 再交付，
重启后由轮询/巡检从库里补交付；适配器**不得**在内存里缓存交付。
传输失败按「请求是否可能已到达对方」分类（`client::transport` / `body_lost` / `unreadable`，中性）：连接前失败（DNS、拒绝连接、连接超时、TLS、SSRF 拦截）= `UpstreamError::Transport`，安全，重试耗尽后照旧 rejected + 回退；
发出后失败（等待超时、中途断开、响应截断/无法解析、5xx 网关、无法解读的 2xx）= `may_have_executed()`，同一请求号重试，耗尽后转 `manual_review`（错误信息带 `[result unknown] ` 前缀，跨重试保留，此后任何终态失败都只转人工、不回退）。
下单前的只读预检（目录/库存/询价）失败用 `.not_executed()` 降级，避免误判。
内容无法自判是否为交付的供货方（acg-faka 人工发货）以 `status = "unconfirmed"` 的 fulfillment 暴露：下单时的文本存为基线，之后查单内容与基线不同（且非已知提示文案）即视为已交付（`procurement::confirmed_by_change`）。

新增一个系统只需两步：

1. 在 `crates/infra/src/integration/` 新建一个模块，实现 `zs_domain::integration::adapter::SupplierAdapter`：
   - `meta()`：协议 ID（存入 `site_connections.protocol`）、三语名称/说明、配置字段表单（`ConfigField`：key、三语 label/placeholder、`text|secret|url|select`、required）、
     能力（`Capability`：`categories`、`incremental_changes`、`push_events`、`quote`、`multi_item`、`idempotency`、`encrypted_delivery`）、是否支持连接码、入站路径；
   - `open(endpoint)`：返回实现 `UpstreamClient` 的客户端。必需操作：`capabilities`、`handshake`（返回能力 ID 形式的 `features`）、`ping`、目录（分类/商品分页或游标/单品）、
     `place_order`（业务拒绝返回 `ok=false` + 错误码，由核心按 `is_retryable_error_code` 分类）、`get_order`/`cancel_order`（`OrderRef` 数字 id 或单号，`addressable` 声明用哪个）、`download`；
     可选操作（`changes`/`change_head`、`quote`、`register_events`）默认返回 `Unsupported`，只有声明了对应能力才会被调用；
   - 入站：`inbound_key`（头部与时间窗预检，返回用于找连接的 key）、`parse_inbound`（用该连接的密钥验签，解析为中性的 `InboundEvent`：订单通知 / 目录变化 / 余额告警）；
   - 可选 `parse_connection_code`。
   - 出站 HTTP 一律使用 `HttpConnector`（SSRF 安全：解析后校验公网地址、不跟随重定向、超时、响应限长，UPS-01）。
2. 在 `crates/infra/src/wire/integration.rs` 的注册处加一行 `.register(Arc::new(XxxAdapter::new(http.clone())))`。
   若需要入站路由，在 `crates/api/src/routes/integration/` 加一个薄 handler，把请求交给 `services.integration.inbound.handle("<协议ID>", &InboundRequest{…})`。

适配器专用的额外配置存连接状态表 `integration_connection_states.extra`（JSON，经 `Endpoint.extra` 传给适配器），无需改表结构。
管理端表单由 `GET /api/v1/admin/site-connections/protocols` 驱动，握手预检 `POST /api/v1/admin/site-connections/handshake` 对任何适配器返回相同的中性字段。
`crates/api/tests/integration_adapter_registry.rs` 用一个只在测试中注册的内存适配器证明核心与协议无关，新增适配器时可照此写测试。

## 10. 新增一个提供方兼容协议（别的系统把本站当上游）

与 §9 方向相反：对方系统按**它自己的**线协议来本站进货。按「核心 + 门面」实现，核心与协议无关，门面只做验签与映射
（现有：acg-faka `/shared/*`、mcy OpenApi `/plugin/open-api/*`，说明见 `docs/protocol/third-party/provider-compat.md`）。

- **核心** `crates/app/src/integration/provide/desk.rs` 的 `SupplyDesk`：站点信息、余额、按调用者定价的可售目录（`offers`/`offer`/`offer_of_sku`，
  价格走订单组的 `price_lines` = 商城同一定价引擎）、`check`/`quote`、`place`（钱包扣款、按下游单号幂等、`DeliveryPolicy::Synchronous`
  时只允许自动发货并通过 `UpstreamOrdering::deliver_now` 在请求内交付）、`resume`/`order`。**不要**在核心里出现协议分支；缺能力就给核心加协议无关的方法。
- **鉴权**：凭证体系复用 `api_credentials`（审核 + 启用 + 用户 active）。签名方式简单（md5、无时间戳）的协议用 `provide/access.rs`
  的 `CompatAccess`（`app_id` = 用户 id + 独立 `app_key`，表 `api_compat_keys`，开关 + IP 白名单）；有更强签名的协议应像 zebra-store 那样走
  `CredentialService` 的 HMAC/nonce 校验。PHP 系系统的表单解析与 md5 签名复用 `provide/form.rs`（`SignScope` 区分是否签数组字段）。
- **新增一个协议只需**：
  1. `crates/app/src/integration/provide/<proto>.rs`：一个门面结构（持有 `SupplyDesk` + 鉴权服务 + `RateLimiter`），
     `serve(endpoint, request) -> Value`：限流 → 解析 → 验签（把校验闭包交给 `CompatAccess::authenticate`）→ 调核心 → 拼装该协议的信封与文案；
     `DeskError`/`AccessDenied` → 协议文案的映射放在门面里。在 `ProvideServices` 加一个字段并在 `ProvideServices::new` 里构造。
  2. `crates/api/src/routes/integration/provide.rs`：把路径表加到 `root()`（或合适的 bucket），handler 只读原始请求（body、IP、origin、头）交给门面；
     加配置开关 `integration.<proto>_compat`（`IntegrationConfig`，手写 `Default`），关闭时返回 404。
  3. 测试 `crates/api/tests/integration_provider_<proto>.rs`：用规格里的签名向量验证测试客户端，再**按对方客户端的方式**构造请求走真实路由；
     覆盖连接、目录价格 = 调用者价、库存、下单扣款与交付、重复单号、余额不足、缺货、签名错误、凭证/密钥停用、查单，以及规格里的历史修复（编号写进测试名/注释）。
  4. 文档：在 `provider-compat.md` 补端点表、映射与限制；前台「个人中心 → API 对接」如需展示接入信息，扩展 `CompatConnectSection`。
- 需要新表时放 `crates/infra/src/db/entity/extra/`，仓储放 `crates/infra/src/db/repo/integration/`，组装在 `crates/infra/src/wire/provide.rs`；
  「不存在才插入」用 `support::insert_if_absent`，不写方言分支。
- 反向代理/前台开发代理要把新协议的根路径转发到后端（`storefront/vite.config.ts` 与 README 的代理列表）。
