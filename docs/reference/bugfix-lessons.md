# dujiao-next 历史缺陷教训（Bugfix Lessons）

> 来源：dujiao-next 全量 git 历史（929 个非 merge 提交，2026-02-11 ~ 2026-09-15），逐个筛查 fix / 修复 / security / 安全 / 避免 / 防止 / revert 类提交，以及 diff 中实际修正错误行为的 feat / refactor / 优化 提交，并阅读其 diff 与同时新增的测试。
> 共整理 **215 条教训**（其中 **高** 严重度 91 条），同一问题的多个提交（含重复提交、先修后改）已合并为一条。
> 目标：Rust（axum + sea-orm，SQLite/MySQL/Postgres）+ Vue3 TSX 重写时，**不得重新引入原项目已修复过的任何缺陷**。

## 1. 简介

dujiao-next 在约 7 个月内迭代了支付（10+ 网关）、订单/库存、卡密、退款、钱包、定价（活动价/批发价/会员价/优惠券）、分销商白标站、站点间对接（上游/下游）、Telegram/Google 登录、2FA、RBAC 等模块。大量缺陷集中在以下几类：

1. **回调可信度**：未签名入口、验签可绕过（空密钥、sign_type 由请求决定、非常量时间比较）、签名正确但商户号/app_id/pid 不属于本渠道、成功状态判定错误（Stripe `payment_status`、BEpusdt 非 paid 状态）、金额/币种未在锁内复核。
2. **并发与幂等**：优惠券次数、卡密预占、手动库存扣减、充值/订单回调重复入账、分销利润与提现、会员累计金额——凡是“读后写”都出过问题；修复方式一律是“行锁 + 条件 UPDATE + rows_affected 校验 + 幂等键唯一索引”。
3. **金额口径**：汇率换算后按原币种校验、手续费承担方、固定手续费、部分退款比例冲回的舍入与超扣、0 元购、批发价门槛口径、优惠叠加顺序、仪表盘利润重复扣减。
4. **SQLite / 多方言差异**：单连接池下事务内使用非事务句柄导致自锁死锁（出现过至少 4 次）、时间 TEXT 格式比较、`json_extract` 在 PG 不存在、ORM 零值/列名映射陷阱。
5. **租户隔离与信息泄露**：分销站域名解析与缓存、return_url 回跳主站、公开 DTO 泄露成本价/内部 ID/`provider_payload`、精确库存泄露。
6. **出站请求与上游对接**：SSRF（内网地址、重定向、DNS rebinding）、上游回调归属与状态机、价格解析失败当作 0。

## 2. 如何使用本文档

- **写代码前**：实现某模块前先通读对应章节，把每条 “**对我们实现的要求**” 当作该模块的硬性设计约束（相当于验收标准）。
- **写测试时**：每条教训都有 “**必测用例**”，给出输入与期望输出；实现时必须编写对应的单元/集成测试，测试名建议带上教训 ID（如 `pay_14_unsigned_callback_rejected`），便于追踪。并发类用例需在 SQLite 与 Postgres/MySQL 上都跑（至少 SQLite + Postgres）。
- **评审时**：用第 21 节 “回归测试清单” 逐项打勾；高严重度（资金/安全/数据完整性）条目未覆盖不得合并。
- **ID 规则**：`模块前缀-序号`，模块内按严重度（高→中→低）再按时间倒序编号。前缀：ORD 订单、PAY 支付、DLV 发货、RFD 退款、WAL 钱包、PRC 定价、AFF 推广、RSL 分销、UPS 上游、AUTH 认证、ADM 管理员、RISK 风控、SET 设置、UPL 上传、NTF 通知、DB 数据库、FE 前端、MISC 其他。
- **字段说明**：“提交”为原仓库短 hash + 作者日期；“原修复方式”中出现的 Go 函数/文件名便于回查原实现（`git show <hash>`）；标注 “⚠ 演进说明” 的条目表示原项目先后改过规则，**以最新行为为准**。
- 少数条目中原作者的修复本身仍有缺口（例如 OKPay 按“当前汇率”重算应付额、会员累计金额读改写在事务外、SVG 仅黑名单过滤），“对我们实现的要求”已给出更严格的做法，应按更严格的做法实现。

### 模块统计

| 章节 | 模块 | 前缀 | 教训数 | 其中高严重度 |
|---|---|---|---|---|
| 3 | 订单/下单与库存 | `ORD` | 12 | 6 |
| 4 | 支付与回调 | `PAY` | 49 | 33 |
| 5 | 发货/卡密 | `DLV` | 10 | 4 |
| 6 | 退款 | `RFD` | 4 | 3 |
| 7 | 钱包/充值/礼品卡 | `WAL` | 3 | 1 |
| 8 | 优惠券/活动价/批发价/会员价 定价 | `PRC` | 15 | 9 |
| 9 | 推广返利 | `AFF` | 1 | 0 |
| 10 | 分销商/租户/域名 | `RSL` | 10 | 7 |
| 11 | 上游对接/采购/下游回调/对账（含 SSRF） | `UPS` | 22 | 12 |
| 12 | 用户认证/2FA/JWT/OAuth（Telegram/Google） | `AUTH` | 10 | 5 |
| 13 | 管理员/RBAC/权限 | `ADM` | 6 | 3 |
| 14 | 验证码/限流/风控 | `RISK` | 7 | 1 |
| 15 | 设置/公共配置/回调路由 | `SET` | 5 | 1 |
| 16 | 上传/素材/SVG/XSS | `UPL` | 5 | 2 |
| 17 | 通知/邮件/Telegram Bot | `NTF` | 11 | 1 |
| 18 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | `DB` | 10 | 1 |
| 19 | 前端（storefront/admin）UI/交互 | `FE` | 26 | 0 |
| 20 | 其他 | `MISC` | 9 | 2 |

### 跨模块通用硬规则（由多条教训归纳）

1. **所有支付/充值/上游回调**：必须验签（常量时间比较、空密钥直接拒绝、签名算法不由请求决定）→ 校验商户归属（app_id / pid / merchant / connection_id）→ 在事务内 `SELECT … FOR UPDATE` 锁定 payment 与 order 后复核渠道、业务单号（含 gateway_order_no）、币种、金额 → 状态机只允许合法迁移 → 首次成功才入账/发货/累计（PAY-01, PAY-04, PAY-14, PAY-34, PAY-35, UPS-02, UPS-03）。
2. **不存在未签名的“通用回调”入口**（PAY-14）；回调 body 限 1MB；回调路由限流（RISK-02）。
3. **所有库存/次数/余额类扣减**使用条件 UPDATE（`WHERE stock >= ?` / `WHERE used_count < limit`）并检查 `rows_affected`，或先行锁再复核；幂等键上唯一索引（ORD-03, ORD-06, DLV-03, PRC-01, RSL-02）。
4. **金额一律 Decimal**（前端用整数分），每一步 `round(2)`；按比例分摊采用“累计法”——最后一笔收敛到剩余值，累计不超过上限（RFD-01, RSL-01）。
5. **换汇渠道**：创建支付时快照“网关币种、网关金额、汇率、手续费策略”，回调只与快照比较，不用当前汇率重算（PAY-10, PAY-25/26, PAY-38~40）。
6. **SQLite**：事务闭包内所有读写只能用事务句柄；行锁事务中不要调用会另起连接的服务（设置读取、风控计数等）；开启 WAL + busy_timeout；时间统一 UTC 且与参数同格式（DB-01, DB-02, DB-05）。
7. **所有公开 API 返回白名单 DTO**（不暴露成本价、内部自增 ID、`provider_payload`、管理员备注、精确库存）（MISC-02, PAY-45, ORD-08）。
8. **所有由对端/管理员配置的出站 URL**：只连公网 IP（解析后校验，防 DNS rebinding）、不跟随重定向、设置超时（UPS-01, UPS-10）。
9. **所有富文本/远端 Markdown/SVG**：服务端或前端 DOMPurify 白名单净化；SVG 默认关闭、强制下载 + CSP sandbox（UPL-01, UPL-02, SET-02, DLV-02, FE-06）。
10. **所有“开关/限制”必须服务端强制**，不能只靠前端隐藏（注册开关、仅余额支付、渠道限制、购买数量、分类状态、优惠券适用范围）（AUTH-05, PAY-05, PAY-11, PAY-23, ORD-09, ORD-10）。
11. **租户（分销站）**：租户解析结果与缓存按 host 隔离、禁用分销商/域名立即失效；return_url、邮件品牌、公告、定价都要按当前租户生成（RSL-03, RSL-06, RSL-07, PAY-06, NTF-02）。
12. **启动安全**：弱/默认/重复的运行时密钥拒绝启动；初始管理员恒为超管；内置角色权限覆盖所有路由（MISC-01, ADM-02, ADM-03, ADM-06）。

---

## 3. 订单/下单与库存（ORD，12 条）


#### ORD-01 取消订单事务内加锁复核状态；用户取消回滚优惠券；游客订单密码 ≥6 位

- 提交: bb3564fb 2026-09-15
- 严重度: **高**
- 问题现象: (1) `cancelOrderWithChildren` 在事务外读到 pending_payment 后进入事务直接改 canceled 并回补库存，与并发到达的支付回调竞争：订单可能刚被标为 paid 就被取消（钱收了、订单取消、库存回补）。(2) 用户主动取消未支付订单时传 `rollbackCoupon=false`，优惠券使用次数不回滚，券被白白消耗。(3) 游客订单密码只校验非空，1 位密码可被暴力猜解查单。
- 根因: 检查-修改不在同一把锁下（TOCTOU）；取消路径参数错误；缺少密码强度。
- 原修复方式: order_service_child.go 事务开头 `orderStore.GetByIDForUpdate(order.ID)`，若 `locked.Status != order.Status` 返回 `ErrOrderCancelNotAllowed`；`CancelOrder` 改为 `cancelOrderWithChildren(order, true)` 并透传 ErrOrderCancelNotAllowed。新增 `validateGuestPassword`：trim 后空 → ErrGuestPasswordRequired，`len([]rune) < 6` → ErrGuestPasswordTooShort（error.guest_password_too_short），在 CreateGuestOrder、createOrder、buildOrderResult（预览）三处调用；前端 useCheckout 同步校验。
- **对我们实现的要求**: 任何状态变更（取消、超时关闭、支付成功、发货）都必须在事务内 `SELECT ... FOR UPDATE` 重新读取并用条件更新（`UPDATE ... WHERE id=? AND status='pending_payment'`，检查 rows_affected）；用户取消与超时取消都回滚优惠券用量；游客密码最少 6 个字符（按 Unicode 字符计数，trim 后）。
- **必测用例**: 并发执行"取消订单"与"支付回调成功"100 次，最终不存在 status=canceled 且有成功支付未入异常的情况；用户取消带券订单后 coupon.used_count 减 1；游客密码 "12345" → guest_password_too_short，"一二三四五六" → 通过。

#### ORD-02 游客订单凭据：HMAC 摘要存储 + 仅通过 Authorization: Guest 头传递

- 提交: 4e4d5bbc 2026-07-27, 9f8dc02f 2026-07-27, fd0cb609 2026-07-27
- 严重度: **高**
- 问题现象: (1) 游客订单密码明文存于 orders.guest_password，数据库泄露即可查看所有游客订单卡密。(2) 游客查单/支付接口通过 URL 查询参数 `?email=&order_password=` 或 JSON body 传凭据，进入代理访问日志、浏览器历史、Referer。(3) 回填迁移在 PostgreSQL 上 `SUBSTRING(x FROM $n)` 的整数占位符被推断为 text，pgx 编码失败，启动迁移报错。
- 根因: 凭据存储与传输不安全；PG 参数类型推断。
- 原修复方式: order gormstore `New(db, guestCredentialSecret)`（密钥为空直接 panic）；创建游客订单时 `guest_password = "hmac-sha256:" + hex(HMAC-SHA256(secret, lower(trim(email)) || 0x00 || password))`；所有游客查询条件改为比较摘要；`BackfillGuestCredentialHashes` 每批 500 条迁移历史明文（`WHERE id=? AND guest_password=<旧值>` 条件更新，rows_affected≠1 报错），候选 SQL 在 SQLite 用 GLOB、PG 用 `!~` 正则筛出非规范摘要，长度与起始位置作为字面量内联（9f8dc02f）。`ginutil.GetGuestCredentials`：只接受 `Authorization: Guest <base64url_nopad(email\npassword)>`，header 总长 ≤4096，email ≤320、password ≤256，任一为空拒绝；游客查单、游客支付创建/捕获全部改用它。
- **对我们实现的要求**: 游客密码以带密钥 HMAC 存储（密钥独立配置、缺失即拒绝启动）；验证时重算比较；凭据只从 Authorization Guest 头读取（base64url 无填充）；迁移 SQL 中不要把整数作为未指定类型的占位符传给 PG 的 SUBSTRING。
- **必测用例**: 创建游客订单后数据库中 guest_password 以 `hmac-sha256:` 开头且 64 位 hex；URL 参数传凭据查单 → 401/400；正确 Guest 头 → 成功；邮箱大小写不同 → 成功；历史明文订单迁移后仍可用原密码查单；PG 上迁移不报错。

#### ORD-03 手动库存扣减必须检查 rows_affected

- 提交: 4e4d5bbc 2026-07-27
- 严重度: **高**
- 问题现象: 手动发货商品锁库存使用条件 UPDATE（库存足够才扣），但忽略 rows_affected，库存不足时 UPDATE 影响 0 行仍视为成功 → 超卖。
- 根因: 未检查条件更新结果。
- 原修复方式: manual_stock.go `applyManualStockByItems(..., requireAffected)`：扣减（reserve/consume）路径传 true，SKU 或商品更新 affected != 1 → ErrManualStockInsufficient；回补路径 false。
- **对我们实现的要求**: 所有"条件扣减"（库存、余额、券次数、卡密占用）必须检查 rows_affected==预期值，否则回滚并报库存不足。
- **必测用例**: 手动库存 1，两个并发下单各买 1 → 1 成功 1 返回 manual_stock_insufficient，库存=0 不为负。

#### ORD-04 订单取消/过期后支付记录仍为 pending，可继续支付

- 提交: 05c3dd54 2026-05-18, 402256ef 2026-05-19
- 严重度: **高**
- 问题现象: 订单超时取消（CancelExpiredOrder/ensureOrderCanceledIfExpired）、用户取消（CancelOrder）、后台 UpdateOrderStatus→canceled、单子订单取消（cancelSingleOrderInTx）后，关联 payments 仍是 initiated/pending；前端 loadLatestPayment 会恢复旧支付，用户可能对已取消订单付款。
- 根因: 订单状态与支付记录状态未同事务联动。
- 原修复方式: PaymentRepository.ExpirePendingByOrderIDs(orderIDs, expiredAt)：`UPDATE payments SET status='expired', expired_at, updated_at WHERE order_id IN ? AND status IN ('initiated','pending')`；cancelOrderWithChildren 在同一事务对父+全部子订单调用（402256ef 去掉了开关，所有取消路径都过期支付）；cancelSingleOrderInTx 在 target==canceled 时也调用，expiredAt 取 canceled_at。错误直接返回让事务回滚。
- **对我们实现的要求**: 任何把订单置为 canceled 的路径（用户、后台、超时、创建失败回滚）都必须在同一 DB 事务里把父子订单的 initiated/pending 支付置为 expired。已 success 的支付不动。
- **必测用例**: 父订单+2 子订单各有 pending 支付，超时取消 → 3 条支付均 expired；用户取消单订单 → 其 pending 支付 expired、success 支付保持；ExpirePending 失败 → 订单取消回滚。

#### ORD-05 删除商品：有库存或成交记录禁止删除，否则在事务内级联清理

- 提交: c4b98b38 2026-04-02, 159924ae 2026-04-04, d898a22e 2026-04-02
- 严重度: **高**（高（数据完整性：订单、卡密、映射成为孤儿数据，还可能把有库存的卡密一并删掉））
- 问题现象: 旧的 `ProductService.Delete` 只软删 product，SKU、会员价、购物车项、商品映射（product_mappings + sku_mappings）、卡密和卡密批次全部残留。购物车里仍然引用已删除的商品；上游同步仍然处理这个映射；删除还有可用或预占卡密的商品会让卡密库存和进行中的订单失去归属。批量删除接口只返回 success_count，不知道哪些失败了。
- 根因: 删除没有做前置校验，也没有级联清理。
- 原修复方式: 事务外先做前置校验（避免 SQLite 单连接自锁）：`CountAvailable(product,0)>0` 或 `CountReserved>0` 时返回 `ErrProductHasStock`；`CountOrderItemsByProduct>0`（order_items.product_id）时返回 `ErrProductHasOrderRecord`。校验通过后在一个事务内依次删除 card_secrets、card_secret_batches、product_skus、member_level_prices、cart_items、sku_mappings（通过 product_mapping_id IN 子查询）、product_mappings，最后删 product。handler 把两种错误映射成 400 `error.product_has_stock` / `error.product_has_order_record`；`BatchDeleteProducts` 的返回里增加 `failed_ids`。
- **对我们实现的要求**: 删除商品前必须检查可用/预占卡密数 = 0 且没有任何 order_item 引用；有成交记录的商品只能下架（is_active=false），不能删除。级联删除放在同一个 sea-orm 事务里。批量操作逐个执行，并返回 total、success_count、failed_ids，失败原因可以带上。校验读取在 SQLite 下要避免事务外持有写锁的连接冲突（使用同一连接或先读后写）。
- **必测用例**: ①商品有 1 张 available 卡密时删除返回 400 product_has_stock，数据不变；②有 1 张 reserved 卡密时同样拒绝；③有历史 order_item 时返回 400 product_has_order_record；④干净商品删除后 skus、member_level_prices、cart_items、product_mappings、sku_mappings、card_secret_batches 全部清空；⑤批量删除 [可删, 有库存] 返回 success_count=1、failed_ids=[有库存的 id]。

#### ORD-06 手动库存语义改为"剩余库存"，-1 表示无限（旧语义 0=不限导致可超卖）

- 提交: ebb18792 2026-02-27, 6bbffe32 2026-02-27, c945ade5 2026-02-27（后台表单）
- 严重度: **高**
- 问题现象: 旧语义 `manual_stock_total` 是"总量"，`0 表示不启用库存控制`；管理员把库存改成 0（想下架售罄）时反而变成无限可售；可用量需 total−locked−sold 计算，SKU 级/商品级多处不一致；前端 `isDefaultSkuCode` 等启发式判断是否限库存。
- 根因: 用 0 同时表示"没货"和"不限"，语义冲突。
- 原修复方式: 常量 `ManualStockUnlimited=-1`；`manual_stock_total` 语义改为"当前剩余可售"。SQL：Reserve `WHERE manual_stock_total>=0 AND manual_stock_total>=? SET total=total-?, locked=locked+?`；Release `WHERE total>=0 AND locked>=? SET total=total+?, locked=locked-?`；Consume `WHERE total>=0 AND (locked>=q OR total>=(q-locked))`，锁定不足时按短缺量扣 total（兼容历史未预占订单），locked 归零，sold+=q。释放/消耗时跳过 -1 的行。一次性迁移 `migration/manual_stock_remaining_v1`（setting 标记）把旧数据 `total=max(total-locked-sold,0)`（仅 total>=0 的行）。校验 `< -1` 为非法；多 SKU 汇总时有任一 -1 则商品为 -1；公共接口按启用 SKU 汇总剩余，任一无限则 `unlimited`。单规格同步 `pickSingleModeTargetSKUIndex` 优先 DEFAULT 活跃 SKU 并停用其余。前端 cart/checkout/detail 统一 `normalizeManualStockTotal`（保留 -1，其余 floor≥0），-1 不限购。
- **对我们实现的要求**: 采用"剩余库存 + -1 无限"语义；所有扣减用条件 UPDATE + rows_affected 判断，不得读后写；-1 行绝不参与加减；迁移需幂等（标记位）。前端统一一个库存归一化函数。
- **必测用例**: total=0 下单 → 库存不足；total=-1 下单 100 件 → 成功且 total 仍 -1；total=5 预占 3 → total=2,locked=3；取消 → total=5,locked=0；locked=0 的旧订单 consume 2 且 total=5 → total=3,sold=2；total=1 两个并发预占各 1 → 仅一个成功。

#### ORD-07 游客订单邮箱大小写/空格归一化（前后端一致）

- 提交: ce0a6eb7 2026-03-31; 8751a5e9 2026-09-15
- 严重度: **中**
- 合并条目: (1) 游客订单查询邮箱大小写不一致导致查不到订单 ｜ (2) 游客凭据编码邮箱需小写，与后端一致
- 问题现象:
  - (1) 游客下单时邮箱已归一化为小写存储，但 `GetOrderByGuest`、`GetOrderByGuestOrderNo`、`ListOrdersByGuest` 直接用用户输入的 `User@Mail.com` 查询，查不到订单。
  - (2) 后端创建游客订单时邮箱转小写后计算 HMAC；前端 `encodeGuestAuthorization` 只 trim 未 lower，用户输入 `User@Mail.com` 查单提示"信息不匹配"。
- 根因:
  - (1) 写入和查询两侧的归一化不对称。
  - (2) 前后端规范化不一致。
- 原修复方式:
  - (1) 三个查询入口先做 `strings.ToLower(strings.TrimSpace(email))`。
  - (2) frontend/user/src/api/order.ts 改为 `${email.trim().toLowerCase()}\n${orderPassword.trim()}` 再 base64。
- **对我们实现的要求**:
  - (1) 邮箱统一经过同一个 `normalize_email()` 处理（trim + lowercase），用在下单、查询、交付下载、支付和登录所有入口。
  - (2) 后端验证游客凭据时自身也应对 email trim+lowercase（不依赖前端）；前端同样规范化。
- **必测用例**:
  - (1) 用 `a@b.com` 下单后，分别用 ` A@B.COM `、`a@B.com` 查询列表、详情和下载，都能找到。
  - (2) 用 `a@b.com` 下单，用 `A@B.COM ` 查单 → 成功。

#### ORD-08 模糊库存展示：公开接口不得泄露精确库存

- 提交: b4368cbf 2026-06-15, 3004e6bf 2026-06-15, b671b0a9 2026-06-15, b4368cbf 2026-06-15
- 严重度: **中**
- 问题现象: （新功能）商品可配置 stock_display_mode=exact/status/range/hidden，非 exact 时前台 API 仍返回 manual_stock_total/auto_stock_available 等字段即可推算真实库存。
- 根因: —
- 原修复方式: 公开商品/SKU 响应中非 exact 模式时对数值打码：-1 保持（无限），≤0→0，>0→1；manual_stock_sold→0；另给出 stock_status（unlimited/out_of_stock/low_stock(≤5)/in_stock）、stock_display、stock_range_min/max（1-5、6-20、21-50、51-100、100+）、stock_quantity_hidden；非法模式存入被拒（ErrProductStockDisplayInvalid→400），读取时未知值按 exact。渠道(channel) catalog 同步返回展示字段。前端购物车在 quantity_hidden 时不再用库存数做数量上限校验（交给后端）。
- **对我们实现的要求**: 在 DTO 序列化层统一打码，所有公开出口（商品列表、详情、购物车、渠道 API）一致；后端下单仍以真实库存校验。
- **必测用例**: hidden 模式真实库存 37 → auto_stock_available=1、stock_display=hidden；range 模式 37 → range_21_50/min21/max50；exact 模式原值；库存 0 → out_of_stock。

#### ORD-09 分类约束：仅叶子且启用分类可挂商品/上架；停用分类前台与 Bot 均不可见不可下单

- 提交: 273197c1 2026-05-27, 53d8e6b8 2026-05-27; 42ce9b6f 2026-03-16, 9f0b90c4 2026-03-16, a49f75b3 2026-03-16, c86176c8 2026-03-17, ef446569 2026-03-19, 645ff475 2026-03-19; e29f1eb3/037bba3f 2026-05-07
- 严重度: **中**
- 合并条目: (1) 无分类/非叶子/停用分类商品可被上架 ｜ (2) 二级分类：仅叶子分类可挂商品、父分类校验、删除保护、快捷更新也要校验 ｜ (3) 分类停用后前台与 Bot 都不能展示或下单
- 问题现象:
  - (1) 快速上架（QuickUpdate is_active=true）、批量上架不校验分类，商品 category_id=0 或指向停用/有子分类的分类也能上架，前台出现孤儿商品；上游导入自动建分类时若同 slug 分类被软删除，Create 触发 UNIQUE 冲突；批量导入每个商品都调用一次上游 ListCategories（N+1）。
  - (2) 支持二级分类后：(1) 商品可挂到有子分类的父分类上，前台浏览混乱；(2) 分类可把自己设为父、或把二级分类设为父（形成三级/环）；(3) 删除有子分类的分类会留下孤儿；(4) 编辑商品改分类后保存无效（GORM 预加载的 Category 结构体把 category_id 覆盖回旧值）；(5) Bot 渠道目录中，无直接商品但有子分类的一级分类被隐藏，二级分类因此不可见；(6) 后来新增的 `PATCH /admin/products/:id` 快捷更新可直接改 category_id，绕过叶子校验。
  - (3) 新增分类启用/停用功能。停用后要求前台列表、详情、分类列表、Bot 目录、sitemap 都不显示。
- 根因:
  - (1) 上架路径缺少分类校验；软删除与唯一索引冲突。
  - (2) 缺少层级约束；ORM 关联回写；新接口未复用校验。
  - (3) 新功能。
- 原修复方式:
  - (1) validateProductActivationCategory：categoryID>0、分类存在且 is_active、CountChildren==0，否则 ErrProductCategoryInvalid（400 error.product_category_invalid）；quickUpdateCategoryID 支持多种数字类型、拒绝负数/非整数 float；BatchUpdateProductStatus 返回 failed_items[{id,error_code,message}]。导入支持 auto_create_category；findOrCreateLocalCategory 先查 Unscoped 软删除记录并 Restore（deleted_at=NULL、刷新 name/parent/is_active）。批量导入预取一次上游分类 map。
  - (2) Category 增加 `parent_id`（0=一级，迁移将 NULL 置 0）；`validateParent`：parent 不能是自己、必须存在且为一级、有子分类的一级分类不能再被设为二级；删除时 CountChildren>0 或有商品 → ErrCategoryInUse；商品/导入映射调用 `validateProductCategoryAssignment` 仅允许叶子；`ListPublic` 对父分类展开为自身+子分类 ID 列表（`category_id IN ?`），渠道目录支持 `exact=1` 精确匹配；Update 前 `product.Category = models.Category{}`；目录可见性：一级分类有直接商品或有子分类即可见，二级分类随父可见。QuickUpdate（is_active/sort_order/category_id）直接 `Updates(fields)`，**未做叶子校验**（原实现遗留缺陷）。
  - (3) `categories.is_active`（默认 true，带索引）。公开商品查询加 `EXISTS (SELECT 1 FROM categories c WHERE c.id=products.category_id AND c.is_active=true AND c.deleted_at IS NULL)`，同时 `is_active`/`slug` 改为带表名限定（`products.is_active`），避免 JOIN 时列名歧义。`expandPublicCategoryIDs`：父分类停用时返回空列表，子分类只展开启用的。渠道 GetProductDetail 检查 `product.Category.IsActive`。新增接口 `PATCH /admin/categories/:id/active` 并补 RBAC 种子。注意：原实现**下单路径没有校验分类是否启用**（用 product_id 直接下单仍然成功）。
- **对我们实现的要求**:
  - (1) 所有“上架”入口（编辑、快速更新、批量、导入）统一调用分类校验：必须是存在、启用、叶子分类；批量接口返回逐条失败原因。sea-orm 软删除场景下按 slug 查含已删除记录并复活。
  - (2) 分类最多两级，服务端校验 parent；商品分配只允许叶子（包括全量更新、快捷更新、批量、上游导入）；删除分类需无子分类且无商品；公开列表父分类包含子分类商品。
  - (3) 在"商品可售"的统一判定函数中包含分类启用校验（包括父分类链），列表、详情、购物车、下单、渠道、上游 ListProducts 都走这个函数。SQL 里的列一律带表名限定。
- **必测用例**:
  - (1) category_id=0 的商品 quick 上架 → 400；分类有子分类 → 400；批量上架 3 个其中 1 个无分类 → success_count=2、failed_items 含该 id；导入时同 slug 分类已软删除 → 复活且表中仅 1 行。
  - (2) 创建 parent=二级分类 → 400；parent=自身 → 400；有子分类的父分类挂商品（create/update/quick PATCH/import）→ 400；删除有子分类的分类 → 400；父分类列表返回子分类下商品；编辑商品从 A 改到 B → 读取为 B。
  - (3) 停用分类后，其商品列表为空、按 slug 查详情返回 404、Bot 详情返回 404、直接 POST 下单返回 product_not_available；父分类停用后子分类下的商品也不可见。

#### ORD-10 单商品最小/最大购买数量限制（服务端强制）

- 提交: 5aad550f/fd97004c/3c59af75 2026-04-29, 11caa9f8 2026-04-29; 84976029 2026-03-10（b07c58a7 前端）
- 严重度: **中**
- 合并条目: (1) 商品最小购买数量限制 ｜ (2) 单商品单次购买上限 max_purchase_quantity
- 问题现象:
  - (1) 新增 min_purchase_quantity，需要前后端一致校验；前端 ProductQuickBuy 在 `effectiveMin` computed 定义之前就引用了它，初始化报错（TDZ），快速购买组件崩溃（11caa9f8）。
  - (2) 原仅 Telegram Bot 配置里有全局 `max_purchase_quantity`，网站/购物车/API 下单都不受限。
- 根因:
  - (1) 新功能；另一个是 Vue `<script setup>` 中声明顺序问题。
  - (2) 限购放在渠道配置而不是商品且只在客户端校验。
- 原修复方式:
  - (1) `validateProductPurchaseQuantity`：quantity<=0 → `ErrInvalidOrderItem`；min>0 且 qty<min → `ErrProductMinPurchaseNotMet`（`error.product_min_purchase_not_met`）；max>0 且 qty>max → `ErrProductMaxPurchaseExceeded`。创建或更新商品时 min>max（两者都>0）→ `ErrProductPurchaseLimitInvalid`；负数归一化为 0（不限）。购物车 upsert、buildOrderResult、渠道详情都校验或下发该字段。前端 `clampCartQuantity(q,max,min)` 把数量收敛到 [min,max]，Checkout 中 `itemMinNotMet` 时禁止提交并提示。
  - (2) `Product.MaxPurchaseQuantity`（≤0 归一为 0=不限）；`validateProductPurchaseQuantity(product, qty)`：qty<=0 → ErrInvalidOrderItem；limit>0 && qty>limit → `ErrProductMaxPurchaseExceeded`（`error.product_max_purchase_exceeded`，400）。在 `CartService.UpsertItem` 与 `buildOrderResult` 两处服务端校验；购物车接口返回该字段供前端限制。
- **对我们实现的要求**:
  - (1) Rust 下单、购物车、渠道下单共用一个数量校验函数；商品 CRUD 校验 min<=max。Vue TSX 中 computed 在使用之前声明。
  - (2) 限购在服务端下单与加购两处都校验（含游客、Channel/Bot、上游 API 下单入口），不依赖前端。
- **必测用例**:
  - (1) min=3 时下单 qty=2 返回 min_not_met、qty=3 成功；min=5 且 max=3 时创建商品返回 400；min 为负数时保存后为 0。
  - (2) limit=2 下单 3 → 400 product_max_purchase_exceeded；limit=0 下单 999（库存足）→ 成功；qty=0 → 400。

#### ORD-11 编辑 SKU：删除的 SKU 用软删会和同 sku_code 的新行撞唯一索引

- 提交: a1cd76c7 2026-03-27
- 严重度: **中**
- 问题现象: product_skus 有唯一索引 `(product_id, sku_code)`。旧逻辑对编辑后不再保留的 SKU 只设 `is_active=false`；另一条路径做了软删（deleted_at）。之后管理员再加一个同 sku_code 的 SKU 时，INSERT 撞唯一索引（软删行仍然占着这个索引），保存失败。单 SKU 模式 `syncSingleProductSKU` 会不断累积 inactive 的 SKU 行。
- 根因: 软删除 + 普通唯一索引组合，软删的行仍然占用唯一键。
- 原修复方式: 新增 `ProductSKURepository.Delete(id)`，用 `Unscoped().Delete` 硬删；新建 SKU 前调用 `PurgeSoftDeletedByProductAndCode(productID, skuCode)` 清掉 deleted_at 非空的残留行；不再保留的 SKU 直接硬删。删除之前 `ensureAutoSKUCardSecretStockSafe` 会校验：auto 发货的 SKU 由 active 变成 inactive 或被删除时，如果 `available>0` 或 `total-used>0`（有未用完或预占的卡密），返回 `ErrProductSKUHasCardSecretStock`。
- **对我们实现的要求**: 凡是带唯一键又需要"可删除后重建"的表（sku_code、slug、coupon code 等），要么硬删，要么唯一索引带上 deleted_at 或用部分索引，并且三种数据库都要兼容（MySQL 不支持部分索引）。删除或停用 auto 类型 SKU 前必须校验卡密库存。需要注意：被历史订单引用的 SKU 如果被硬删，order_item 仍然保留 sku_id 和快照，所以展示必须依赖快照，不能 join SKU。
- **必测用例**: ①创建 SKU A → 编辑时移除 A → 再次添加 code=A，保存成功；②auto 商品的 SKU 有 3 张 available 卡密时，把它移除或停用返回 ErrProductSKUHasCardSecretStock；③单 SKU 商品反复切换后数据库里只有 1 行 SKU。

#### ORD-12 后台库存状态筛选：auto/upstream 商品和低库存阈值

- 提交: d898a22e 2026-04-02, 3b1f78e3 2026-03-08, 946385cf 2026-03-08
- 严重度: **低**
- 问题现象: 最初的 `manual_stock_status` 筛选只对 manual 商品生效，auto（卡密）和 upstream 商品永远筛不出来。后来扩展成 `stock_status` 时，auto 的 "low" 定义为可用卡密 = 0，与仪表盘"低库存阈值"（默认 5）不一致：可用 3 张的商品在仪表盘告警为低库存，在商品列表却归入 normal。参数名也从 `manual_stock_status` 改成了 `stock_status`（兼容拼写错误 `stock_staus`）。
- 根因: 各发货类型的库存口径不统一，筛选没有使用系统配置的阈值。
- 原修复方式: `applyStockStatusFilter(query,status,lowStockThreshold)`：manual 为非无限且剩余 <=0 属于 low；auto 为 `0 <= available <= threshold` 属于 low、`> threshold` 属于 normal；upstream 为非无限且 sku_mappings.upstream_stock 之和 = 0 属于 low；unlimited 为 manual 存在 -1 或 upstream 存在 -1。阈值来自 `GetDashboardLowStockThreshold()`（读取失败时使用默认值），负数按 0 处理。
- **对我们实现的要求**: 库存口径（manual 剩余 = total 字段、-1 表示无限；auto 统计 status=available 且未软删的卡密；upstream 统计映射库存）集中在一个模块，筛选、仪表盘和告警共用，低库存阈值读取同一个设置。
- **必测用例**: 阈值 5，auto 商品可用 0/3/6 张 → low 包含 0 和 3，normal 只包含 6；manual 无限库存 SKU 只出现在 unlimited；upstream upstream_stock 全为 0 时属于 low。

## 4. 支付与回调（PAY，49 条）


### 支付 · 通用（跨网关）


#### PAY-01 验签常量时间比较、拒绝空密钥、sign_type 不信任请求、商户号必须匹配

- 提交: 2f870d5e 2026-09-15
- 严重度: **高**
- 问题现象: (1) 各网关用 `strings.EqualFold` 比较签名（非常量时间，可计时侧信道）。(2) 渠道密钥（bepusdt AuthToken、epay MerchantKey、epusdt SecretKey、okpay MerchantToken）为空时，攻击者可用空密钥自行计算合法签名伪造回调。(3) 支付宝 `sign_type` 从回调表单读取，攻击者可指定降级为 RSA（SHA1）。(4) OKPay 仅在回调带了 merchant_id 时才比较，省略该字段即绕过商户号校验。
- 根因: 信任回调中的元数据；未对配置缺失做防御。
- 原修复方式: 改为 `hmac.Equal(lower(expected), lower(sign))`；bepusdt/epay(v1 MD5)/epusdt/okpay 在密钥 trim 后为空时返回 ErrConfigInvalid；alipay `signType` 只取 `cfg.SignType`，为空默认 RSA2；okpay 配置了 MerchantID 时要求 `data.MerchantID == cfg.MerchantID`（不再允许为空跳过）。
- **对我们实现的要求**: Rust 中所有签名比较用 `subtle::ConstantTimeEq`（大小写归一后）；每个网关 verify 开头检查密钥非空；算法/sign_type 只来自渠道配置；商户号配置了就必须严格相等。
- **必测用例**: 渠道密钥为空时，用空密钥签名的回调 → 拒绝；支付宝回调 sign_type=RSA 而配置 RSA2 → 按 RSA2 验证失败；OKPay 回调缺失 merchant_id 字段且配置有商户号 → 拒绝；签名大小写不同但值相同 → 通过。

#### PAY-02 金额守恒：支付只有覆盖订单当前在线应付额才可履约；欠付入余额；余额全额支付作废遗留在线链接；余额分配轮次幂等键

- 提交: f57247b4 2026-08-27
- 严重度: **高**
- 问题现象: (1) 混合支付：余额 5 + 渠道 A 在线 10，之后切到渠道 B，余额被退回、在线应付额升到 15，但旧 A 链接在网关侧仍可付；用户用旧链接付 10 就能履约整单 15。(2) 余额覆盖全额支付时未作废订单遗留的 pending 在线支付，用户可能再付一次。(3) 余额"用 → 退回 → 再用"时，幂等键 `order:<id>:order_pay` 命中第一轮已退回的流水，第二轮被判为重复跳过：订单标记已用余额但钱包没扣钱。
- 根因: 回调未校验金额是否覆盖当前应付；幂等键无轮次。
- 原修复方式: payment_service_callback.go `applyPaymentUpdate` 在锁内计算 `required = TotalAmount - WalletPaidAmount`（round 2），`covered = paymentCoveredOrderAmount(lockedPayment)`（用户承担手续费/legacy 空策略且 fee>0 时扣除 fee）；成功回调且订单 open 且 covered<required → 不履约，ExceptionCode=`underpaid_payment_succeeded`，调用 `creditUnderpaidToWallet` 以 reference `payment:<id>:underpaid_credit` 幂等入账（类型 `order_underpaid_credit`），游客订单(UserID=0)只打异常码待人工。payment_service_create.go 余额全额支付后 `SupersedePendingByOrderID(orderID, paymentID, paidAt)`。wallet `orderAllocationReference`：统计 `wallet_transactions WHERE order_id=? AND type=?` 数量 count，count=0 用 `order:<id>:<action>`，否则 `order:<id>:<action>:<count+1>`；reference 唯一索引兜底。
- **对我们实现的要求**: 支付成功回调在事务+行锁中比较"本次支付可覆盖额"与"订单当前在线应付额"，不足则不履约并按幂等键把钱转入余额；余额全额支付必须作废同订单所有 pending 在线支付；钱包流水 reference 带轮次序号且有唯一索引。
- **必测用例**: 余额5+A在线10 → 切 B（应付15）→ A 回调 10 成功：订单仍待支付、异常码 underpaid、余额 +10（合计 15），重复回调不重复入账，再用余额补齐可完成；游客同场景只标异常；足额回调正常履约；余额用→退→再用，钱包余额真实扣减两次、退回一次。

#### PAY-03 手续费承担策略快照（none/merchant_absorbed/customer_surcharge/legacy）+ 新链接作废旧链接

- 提交: ab60e138 2026-08-11
- 严重度: **高**
- 问题现象: 原逻辑总是 `payable = online + fee`（向用户加收手续费），与文档"默认商户承担"不符；配置变化会重新解释历史支付；同一订单切换渠道后旧支付链接仍 pending 可付，造成重复支付/迟到成功无从审计。
- 根因: 手续费策略没有按笔快照；旧链接未作废。
- 原修复方式: payments 新增 `fee_policy varchar(32) not null default 'none'`、`exception_code`、`superseded_at`、`superseded_by_payment_id`。`calculatePaymentAmounts(base, feeRate, fixedFee, customerFeeEnabled)`：fee = round2(fixedFee + base*rate/100)；fee=0 → (base, 0, none)；启用加收 → (base+fee, fee, customer_surcharge)；否则 (base, fee, merchant_absorbed)。设置 `payment_config.customer_fee_enabled`（默认 false）、`reuse_legacy_order_fee_payment`（默认 false，旧版加收链接默认不复用而是新建）。迁移 `migration/payment_fee_policy_v1`：fee_amount>0 且策略空/none → legacy_customer_surcharge；fee=0 且空 → none。创建新支付后 `SupersedePendingByOrderID(orderID, newID, now)`：同订单其它 initiated/pending 支付置 expired 并记录 superseded_*。可复用 pending 查询加 `superseded_at IS NULL AND fee_policy IN (none, merchant_absorbed, customer_surcharge)`。回调成功时：已 superseded 的记 `superseded_payment_succeeded`，订单已付记 `duplicate_payment_succeeded`，订单已关闭记 `closed_order_payment_succeeded`（不履约，供人工处理）；成功后 `ExpirePendingByOrderIDs`。渠道列表仅在启用加收时下发 fee_rate/fixed_fee。
- **对我们实现的要求**: 支付表保留 fee_policy 快照与 superseded 字段；每次创建新支付必须作废同订单其他 pending 支付；迟到/重复成功支付不得再次履约，只打异常码；钱包充值同样使用 calculatePaymentAmounts。
- **必测用例**: 未开启加收：订单 100、费率 3% → amount=100、fee=3、merchant_absorbed；开启 → amount=103、customer_surcharge；fee=0 → none。先建 A 渠道支付再建 B → A 状态 expired 且 superseded_by=B；A 迟到回调成功 → exception_code=superseded_payment_succeeded，订单只因 B 履约一次。

#### PAY-04 回调事实校验（渠道/业务单号/币种/金额）在锁内复核 + 成功回调必须带币种与正金额

- 提交: 4e4d5bbc 2026-07-27
- 严重度: **高**
- 问题现象: (1) 回调处理先在锁外读取支付与订单再在事务中整行 Update，并发的两次回调（或回调与取消）会互相覆盖、重复 markOrderPaid/重复发货。(2) 成功回调未携带币种或金额为 0 时跳过比较直接判成功。(3) 回调中的 channel_id、order_no 未与支付记录核对，可用 A 渠道的合法回调推进 B 支付。(4) 钱包充值回调同样问题，重复回调重复加余额/重复触发会员等级。(5) 回调原文完整写日志，可能泄露敏感信息；回调 body 无大小限制。
- 根因: 缺少锁内复核和严格的事实校验。
- 原修复方式: `validateCallbackPaymentFacts(payment, businessOrderNo, status, input)`：`input.ChannelID!=0 && != payment.ChannelID` → invalid；`matchesBusinessOrderNo(input.OrderNo, order.OrderNo/recharge_no, payment)`（也匹配 gateway_order_no）；status=success 时 currency 为空 → CurrencyMismatch、金额非正 → AmountMismatch；币种不等（不区分大小写）→ mismatch；金额非零且与 payment.Amount 不等 → mismatch。`applyPaymentUpdate` 改为事务内 `GetByIDForUpdate(payment)` + `GetByIDForUpdateWithChildren(order)` 后再次校验；已 success 或状态相同 → 只更新元信息（provider_ref、callback_at）；钱包充值 `canApplyWalletRechargeCallback` 状态机 + `newlySucceeded` 标记只在首次成功时入账和调用 `memberLevelSvc.OnRechargeCompleted`。DujiaoPay 仅对"验签入口标记、升级前无法币快照"的 pending 支付允许采纳网关签名币种（`verifiedLegacyDujiaoPayCurrency` 私有字段，普通调用方无法开启）。DujiaoPay webhook 必须带 event_id/order_id/merchant_order_id，交易号优先 tx_id 回退 tx_hash。回调 handler `http.MaxBytesReader(1<<20)`，删除原始 body/form 日志。
- **对我们实现的要求**: 所有网关回调统一走一个 `validate_callback_facts`，在事务中锁定 payment 与 order 后再次调用；成功状态必须同时有币种与正金额且精确相等；幂等：已 success 不回退、不重复履约、不重复入账；axum 回调路由 body 限 1MB；不记录回调原文中的敏感字段。
- **必测用例**: 成功回调缺 currency → 拒绝；金额 0.00 → 拒绝；回调 channel_id 与支付记录不同 → 拒绝；同一成功回调并发 10 次 → 订单只 paid 一次、钱包充值只入账一次、会员等级累计只加一次；body 1MB+1 → 413/拒绝。

#### PAY-05 支付渠道金额区间/角色/会员等级/付款类型限制：服务端强制，写路径（创建支付时）复核

- 提交: 49a5e488/71bc1c01 2026-04-06, dc1a0bc9/781fd767 2026-04-06, b13b85c6 2026-04-07; 4e4d5bbc 2026-07-27
- 严重度: **高**（高；中）
- 合并条目: (1) 支付渠道金额区间/角色/会员等级/付款类型限制必须在服务端强制校验 ｜ (2) 写路径复核支付渠道可用性（角色/会员等级/支付类型）
- 问题现象:
  - (1) 新增渠道字段 `min_amount/max_amount`（decimal(20,2)，0=不限）、`hide_amount_out_range`、`payment_roles`(guest/member)、`member_levels`、`payment_types`(wallet 充值/order 订单)。原实现中金额区间在 `CreatePayment` 里通过 `validatePaymentAmountForChannel` 强制校验（`ErrPaymentAmountTooSmall/TooLarge`，>=1e18 视为溢出）；但角色、会员等级、付款类型**只在 `GetAvailableChannels` 列表中过滤**，`CreatePayment` 没有校验（全仓 grep 不到 matchesChannelRole 在支付创建中的使用）→ 游客或低等级用户手动传 channel_id 就能使用受限渠道。
  - (2) 渠道列表接口会按用户角色、会员等级、付款类型（订单/钱包充值）过滤，但创建支付/充值接口不复核，客户端直接提交隐藏渠道 ID 即可使用（例如仅 VIP 可用的低费率渠道、仅充值可用的渠道）。
- 根因:
  - (1) 只做了 UI 层过滤。
  - (2) 只在读路径过滤。
- 原修复方式:
  - (1) 列表过滤函数 `matchesChannelAmount`（只在 hide_amount_out_range=true 时按金额隐藏）、`matchesChannelRole`（设置了 member_levels 时游客或未分级用户不可见）、`matchesChannelPaymentType`。渠道配置校验：min、max 不能为负、不能溢出，max>0 时要求 min<=max，max=0 允许（不限）。
  - (2) payment_service_channel_rules.go `validateOrderChannelEligibility(channel, order)`：用订单快照中的 UserID/MemberLevelID 构造用户，校验 `matchesChannelRole`、`matchesChannelMemberLevel`、`matchesChannelPaymentType(order)`，失败 ErrPaymentChannelNotAllowedForProduct；`validateWalletChannelEligibility(channel, user)` 用数据库中的用户校验，失败 ErrPaymentChannelNotAllowedForRecharge。CreatePayment 锁单后、CreateWalletRechargePayment 中调用。
- **对我们实现的要求**:
  - (1) Rust 中写一个 `channel_allowed(channel, user, amount, payment_type, order_items)` 函数，列表接口和创建支付（订单支付、钱包充值）**都调用它**；创建支付时不满足就返回 403/400。商品级渠道白名单（`GetAllowedChannelIDsForOrder`）同样要在创建时校验。
  - (2) 创建支付/充值时重新执行与列表一致的渠道可见性规则，以订单快照/数据库用户为准，不信任客户端提交的等级/角色。
- **必测用例**:
  - (1) 渠道 payment_roles=[member] 时游客直接 POST 创建支付返回拒绝；member_levels=[2] 时等级 1 用户被拒绝；payment_types=[order] 时用于钱包充值被拒绝；min=10 时 9.99 被拒绝；max=0 时 1e6 放行。
  - (2) 普通会员直接用 VIP 专属渠道 ID 创建支付 → 拒绝；仅 wallet 类型渠道用于订单支付 → 拒绝。

#### PAY-06 分销站/自定义域名下单，支付完成回跳到主站（return_url 未按 tenant 域名生成）

- 提交: 1dfb7821 2026-07-18
- 严重度: **高**（高（游客订单回跳主站后因 tenant 隔离查不到订单，用户以为没付成功 → 重复支付/客诉））
- 问题现象: 在分销站 `shop.example.com` 下单（CreateOrderAndPay / CreatePayment / CreateGuestOrderAndPay / CreateGuestPayment / RechargeWallet 5 个入口），网关同步回跳地址取的是渠道配置里固定的 `return_url`/`success_url`（`https://main.example.com/pay`），支付完成后跳回主站；主站按 tenant 隔离查询游客订单查不到。
- 根因: adapter 只从渠道 config 读 ReturnURL，没有感知当前请求的 tenant。
- 原修复方式: `payment_service_provider.go` 新增 `resolveTenantReturnURL(ctx, scheme, channel)`：主站 / 无 tenant / tenant.Unavailable → 返回空串（让 adapter 回落到 config 的 return_url/success_url）；分销 tenant → `scheme://{tenant.Host 或 PrimaryDomain}{path}`。path 由 `tenantReturnPath` 取渠道配置 URL 的 path+query（先 `return_url` 后 `success_url`，path 为空或 `/` 时跳过），都缺失时默认 `/pay`。scheme 由 `requestSchemeFromContext` 取 `X-Forwarded-Proto` 第一个值（只接受 http/https），否则看 TLS，非法/空值默认 https。NotifyURL 始终用 config（服务器回调不随 tenant 变）。钱包充值同样传 RequestScheme。
- **对我们实现的要求**: 所有创建支付的入口（订单、游客订单、钱包充值）都要把 tenant + scheme 传进支付服务；仅 ReturnURL（同步回跳）按分销域名重写，NotifyURL 不可随 Host 变化（防止攻击者操纵回调地址）。Host 只能来自已验证的 tenant 解析结果，而不是原始 Host 头。
- **必测用例**: ①主站 tenant、无 tenant、nil ctx → ""；②分销 tenant host=shop.example.com，config return_url=https://main.example.com/pay → `https://shop.example.com/pay`；③Host 为空回落 PrimaryDomain；④scheme "http"→http，""/"ftp"→https；⑤Unavailable tenant → ""；⑥config `https://main/checkout/result?from=gateway` → path `/checkout/result?from=gateway`；只有 success_url 时用它；两者都有时 return_url 优先；config 为 `https://main/` → `/pay`。

#### PAY-07 Webhook URL 无 channel_id 时回调全部失败

- 提交: 69857b88 2026-05-27, db29446e 2026-05-27
- 严重度: **高**
- 问题现象: 微信 V3 规范禁止 notify_url 携带 query，`channel_id` 缺失 → handleWebhookViaRegistry 直接 ErrPaymentInvalid，所有微信支付回调失败（订单不自动完成）。
- 根因: 渠道识别强依赖 URL query。
- 原修复方式: channel_id==0 且 channel_type 为 wechat/stripe（supportsBlindWebhookCandidateMatching）时，列出同 provider_type+channel_type 的 active 渠道，逐个 tryParseWebhookWithChannel（微信 api_v3_key AES-GCM 解密 / Stripe endpoint_secret HMAC），首个验签成功者为目标渠道再 commitVerifiedWebhook；全部失败返回 lastErr。PayPal 因 webhook_id 无自识别能力，仍强制 channel_id。验签成功后的 DB/业务错误不再尝试下一个渠道。
- **对我们实现的要求**: Rust webhook handler 支持“盲匹配”模式：仅限验签失败即可证明不属于该渠道的算法（微信 AES-GCM、Stripe HMAC、DujiaoPay HMAC）；PayPal 必须 channel_id。解析成功后按 (channel_id, gateway_order_no/provider_ref) 反查 payment，确保 payment.channel_id 与匹配渠道一致。
- **必测用例**: 两个 active 微信渠道，回调用第二个渠道密钥加密且无 channel_id → 命中第二个并入账；密钥均不匹配 → 返回签名错误且不入账；PayPal 无 channel_id → 拒绝。

#### PAY-08 Provider 重构引入的参数错位：capture/查单/webhook 被 interaction_mode 校验阻断；跨币种金额未回写

- 提交: 6797d977 2026-05-16, d62f84ee 2026-05-16, 5f75cdec 2026-05-16, 457d4d25 2026-05-16
- 严重度: **高**
- 问题现象: captureViaRegistry 把 channel.ChannelType 当作 ValidateConfig 第二参数（应为 interactionMode），stripe/wechat adapter 拒绝非法 mode → 所有 Stripe/微信 capture 返回 ErrConfigInvalid。重构中还出现：alipay ValidateConfig 丢弃 interactionMode 传空导致永远失败；wechat QueryPayment/ParseWebhook 因空 interaction_mode 被 parseConfig 拦截；跨币种（10 USD→72 CNY，rate 7.2；OKPay 88 CNY×7.0=616 USDT）时 payment.Amount 未更新为实际发送金额，回调金额比对失败。
- 根因: 接口签名 `ValidateConfig(config, interactionMode)` 与 channelType 同为 string，易传错；配置校验被错误复用到非创建路径。
- 原修复方式: 传 channel.InteractionMode；空 mode 在 wrapper 内默认 QR；Query/Webhook 只 ParseConfig 不做 mode 校验；CreatePayment 返回 AmountSent/CurrencySent，service 回写 payment.Amount/Currency，并在 payload 记录 exchange_rate/original_amount/original_currency。
- **对我们实现的要求**: Rust 用 newtype/enum（InteractionMode、ChannelType）防止参数错位；只有 create/admin 保存时校验 interaction_mode；查单与回调不校验 mode。跨币种时 payment 存实际发送金额币种，并保留原金额审计字段，回调比对用实际发送值。
- **必测用例**: interaction_mode=qr 的 stripe 渠道 capture 不返回配置错误；interaction_mode 为空的 wechat 渠道 query/webhook 返回网络/签名错误而非配置错误；OKPay 88 CNY rate 7 → payment.amount=616, currency=USDT, payload.original_amount=88。

#### PAY-09 每笔支付使用独立 gateway_order_no：避免重复发起冲突、不泄露内部 payment_id

- 提交: ef5c9513 2026-03-11, f20b0d66 2026-03-12（08414f9b 为同名前端提交，仅 TG 页面）; 0c1f2aaf 2026-04-06, f00dfbdd 2026-04-06
- 严重度: **高**
- 合并条目: (1) 同一业务单重复发起支付导致网关订单号冲突 + 网关单号泄露内部支付ID ｜ (2) 各网关统一用每笔支付独立的 gateway_order_no，不再透传 payment_id
- 问题现象:
  - (1) 用户对同一订单（或钱包充值单）第二次点"去支付"/换渠道重新支付时，系统把同一个 `order.OrderNo`（充值为 `RechargeNo`）作为 `out_trade_no`/`order_id`/`OutOrderID` 再次发给 epay/epusdt/tokenpay，网关报"订单号已存在"，用户无法完成支付。首版修复用 `DJP{payment.ID}` 作为网关单号，又把自增支付主键暴露给外部（可枚举业务量）。
  - (2) 早期 epay 回调通过 `param` 透传 payment 自增 ID 查找支付记录；支付宝用 `passback_params`，TokenPay 用 `PassThroughInfo=payment_id=N`，微信用 attach，PayPal 用 custom_id。这会暴露自增 ID，回调查找依赖可被篡改的附加字段；官方渠道用 order.OrderNo 作为商户单号，同一订单重新发起支付时单号重复。
- 根因:
  - (1) 业务单号与"支付尝试"是一对多，但第三方聚合网关要求单号全局唯一；且用自增 ID 拼单号泄露内部信息。
  - (2) 回调定位支付记录的依据不统一。
- 原修复方式:
  - (1) `models.Payment` 新增 `GatewayOrderNo`（`index;size:64`）。`shouldUseGatewayOrderNo(channel)` 仅对 provider_type ∈ {epay, epusdt, tokenpay} 生效；`resolveGatewayOrderNo` 若 payment 已有值则复用，否则 `generateSerialNo("DJP")` 随机生成（f20b0d66 去掉 `DJP%d` 形式）；`applyProviderPayment` 用 `resolveProviderOrderNo` 把网关单号传给三家网关。回调侧 `matchesBusinessOrderNo(callbackOrderNo, businessOrderNo, payment)`：回调单号为空、等于业务单号、或等于 `payment.GatewayOrderNo` 均视为匹配（订单与充值回调都改了）。官方渠道（alipay/wechat/stripe/paypal）不变。
  - (2) `shouldUseGatewayOrderNo` 对所有渠道返回 true，每笔 payment 生成 `DJP` 前缀的流水号 `gateway_order_no`，作为 out_trade_no / invoice_id / OutOrderID 发给网关。回调统一用 `PaymentRepo.GetByGatewayOrderNo(out_trade_no)` 查找（取 id desc 最新一条），降级时用网关流水号 `GetLatestByProviderRef(trade_no)` 查找并要求 `payment.ChannelID` 匹配。epay 回调特征改为必须同时有 `pid + out_trade_no + trade_status`。PayPal webhook 先用 `purchase_units[0].invoice_id` 查找，再用 related order id。查到支付记录后，仍然用该记录所属渠道的配置验签。
- **对我们实现的要求**:
  - (1) payments 表必须有 `gateway_order_no`（唯一或至少索引），每个支付尝试生成独立的、不可推导的随机网关单号（前缀+时间+随机，不得含自增 ID）；回调校验单号时同时接受业务单号与该 payment 的 gateway_order_no，但要先按 payment_id/gateway_order_no 定位到具体 payment，不能只凭业务单号找"最新 payment"。
  - (2) Rust 中 payments 表 `gateway_order_no` 设唯一索引；所有网关下单都用它；回调只按 gateway_order_no（或 provider_ref + channel 匹配）定位，不信任任何透传的 ID；验签使用查到的 payment 所属渠道的密钥，并校验渠道 provider_type 与回调类型一致。
- **必测用例**:
  - (1) ①同一订单连续创建两次 epusdt 支付 → 两次发给网关的 order_id 不同，均以 "DJP" 开头且不等于 `DJP{payment_id}`；②payment 已有 gateway_order_no="CUSTOM-1" 再次 resolve → 复用 CUSTOM-1；③回调 order_no=payment.gateway_order_no → 订单支付成功；④充值回调 order_no=gateway_order_no → 充值成功；⑤回调 order_no 为其它任意值 → ErrPaymentInvalid；⑥alipay 官方渠道 → gateway_order_no 为空，仍用业务单号。
  - (2) 同一订单发起两次支付，gateway_order_no 不同；回调中 out_trade_no 不存在返回 fail；伪造 param=其他 payment_id 不影响定位；A 渠道的回调携带 B 渠道 payment 的单号，验签失败（用 B 的密钥）或渠道类型不匹配时被拒绝。

#### PAY-10 渠道汇率转换：payment 记录网关实际金额，回调按转换后的金额和币种校验

- 提交: 5879e2a1 2026-04-01, 6dcc1a22 2026-04-01
- 严重度: **高**（高（金额校验））
- 问题现象: 站点币种为 USD，渠道（支付宝/微信/易支付）只收 CNY。新增 `target_currency` + `exchange_rate` 渠道配置后，如果 payment.Amount 仍记原币金额（10 USD），网关回调传来的是 72 CNY，金额校验就会失败；如果放宽校验，又会给篡改留下口子。Alipay/Wechat 之前硬编码 `payment.Currency="CNY"`，但金额没有换算（把 10 USD 当成 10 CNY 收款，少收了钱）。
- 根因: 发给网关的金额/币种与 payment 记录不一致，并且缺少换算。
- 原修复方式: `common.ExchangeRateConfig{target_currency, exchange_rate}` 嵌入各渠道的 Config，`NormalizeExchangeRate` 把币种转大写并 trim。`ConvertAmount`：rate<=0 或非法时报 ErrPaymentChannelConfigInvalid，结果为 `amount×rate` Round(2)。创建支付时用转换后的 payAmount/payCurrency 请求网关，`appendExchangeInfo` 把 `payment.Amount` 更新为转换后金额、`payment.Currency` 更新为目标币种，并把原始金额、原币种和汇率写入 ProviderPayload 供审计。不需要转换时 Alipay/Wechat 仍默认 CNY。测试：`TestCallbackMatchesConvertedAmount`（10 USD→72 CNY 回调 72 CNY 通过）、`TestCallbackRejectsOriginalAmountWhenConverted`（回调 10 被拒）、`RejectsCurrencyMismatchAfterConversion`、`IdempotentSuccessWithConvertedAmount`、`SkipsAmountCheckWhenZero` / `SkipsCurrencyCheckWhenEmpty`（网关没带金额或币种时跳过对应校验）。
- **对我们实现的要求**: payment 表记录"网关实际收款金额与币种"，另存 original_amount/original_currency/exchange_rate。回调校验以 payment 记录为准，严格比较金额（Decimal 精确到 2 位）和币种。订单入账仍按原币 order.total。汇率配置非法时拒绝创建支付，不能静默回退成 1:1。回调缺少金额或币种时是否跳过校验要逐个网关评估：能验签且网关一定带金额的，缺失时应视为失败。Stripe 零小数币种按 minor unit 规则处理。
- **必测用例**: ①USD 10、rate 7.2、target CNY → 请求网关 72.00 CNY，payment.amount=72.00、currency=CNY，payload 中 original_amount=10.00；②回调 amount=10.00 被拒；③回调 currency=USD 被拒；④rate="0" 或 "abc" 时创建失败；⑤已成功的 payment 重复回调幂等返回成功；⑥10.005×1 的舍入按 Round(2) 结果一致。

#### PAY-11 商品/钱包充值限定支付渠道：服务端按商品取交集校验

- 提交: 90d292bb 2026-03-31, 966910a7 2026-03-31, d19a9584 2026-03-31
- 严重度: **高**（高（绕过渠道限制，例如某商品只允许 Stripe））
- 问题现象: 新增 product.payment_channel_ids（JSON 数组，空表示不限制）和钱包配置 `recharge_channel_ids`。如果只在前端过滤，用户直接传别的 channel_id 就能绕过。订单包含多个商品（多个子订单）时，需要对所有商品的限制取交集。
- 根因: 需要在服务端强制执行，并正确处理"无限制"与"交集为空"的区别。
- 原修复方式: `computeProductChannelIntersection`：不限制的商品跳过；没有任何商品有限制时返回 nil（不限制）；有限制则求交集，交集为空时返回空切片（没有可用渠道）。`validateProductPaymentChannel(items(含 children 的 items), channelID, tx)` 在 CreatePayment 事务内使用 tx 版 repo 调用，不允许时返回 `ErrPaymentChannelNotAllowedForProduct`。`CreateWalletRechargePayment` 调用 `validateWalletRechargeChannel`，不允许时返回 ErrPaymentChannelNotAllowedForRecharge。订单详情返回 `allowed_payment_channel_ids`；channel 接口支持 `context=recharge` / `order_no` 过滤渠道列表。`DecodeChannelIDs` 容错非法 JSON（视为不限制），过滤掉 id<=0。
- **对我们实现的要求**: 用 `Option<Vec<i64>>` 区分"不限制"（None）和"交集为空"（Some(vec![])）。交集为空的订单不能用任何在线渠道支付（只能用余额），前端也要显示"无可用渠道"。原前端 Payment.vue 在 allowed 为空数组时当作不限制，这是 bug，不要照抄。校验在创建支付的事务内完成，子订单商品要一起计入。
- **必测用例**: ①商品 A 限制 [1,2]、商品 B 限制 [2,3]，同一订单 channel=2 成功，channel=1 返回 not_allowed_for_product；②A 限制 [1]、B 限制 [3]，任何渠道都被拒，订单详情 allowed_payment_channel_ids=[]；③A 不限制、B 限制 [3]，只有 3 可用；④充值配置 [5] 时用 channel=4 充值被拒；⑤payment_channel_ids="garbage" 视为不限制。

#### PAY-12 渠道固定手续费（fixed_fee）：范围校验与计算口径

- 提交: 893ca1ad 2026-03-12, 48252e54 2026-03-12, 3b4ab088 2026-03-13; 6a3de4ad 2026-03-12
- 严重度: **高**（高；中）
- 合并条目: (1) 渠道固定手续费：范围校验与计算口径 ｜ (2) 支付渠道固定手续费（fixed_fee）计算规则
- 问题现象:
  - (1) 新增 `fixed_fee`（decimal(6,2)）后，负数固定费会被静默当 0（订单支付路径）或直接参与计算（充值路径），超过 9999.99 写库溢出/报错。
  - (2) 原先只支持比例手续费；新增固定手续费后需保证订单支付与钱包充值两处算法一致、钱包余额支付不收费。
- 根因:
  - (1) 仅在 `GreaterThan(0)` 时采用，未在渠道保存与充值下单时做范围校验。
  - (2) 功能新增（金额计算规则）。
- 原修复方式:
  - (1) `ValidateChannel` 与 `CreateWalletRechargePayment` 均校验 `fixedFee := FixedFee.Round(2)`，`<0 || >=10000` → `ErrPaymentChannelConfigInvalid`。计算：`fee = fixed + round2(online × rate / 100)`（实际为 `(fixed + online×rate/100).Round(2)`），`payable = online + fee`；payment 记录 fee_rate/fixed_fee/fee_amount；纯钱包支付的 payment fixed_fee=0。前端支付页展示 "x% + ¥y"，无费率且无固定费显示"免手续费"。
  - (2) `PaymentChannel.FixedFee`、`Payment.FixedFee` 均为 `decimal(6,2) not null default 0`（上限 9999.99）。`CreatePayment` 与 `CreateWalletRechargePayment`：`fixedFee = channel.FixedFee>0 ? round2 : 0`；`feeAmount = round2(fixedFee + onlineAmount*feeRate/100)`；`payable = round2(online + fee)`；钱包余额支付单 FeeRate/FixedFee/FeeAmount 全 0。公共配置与 channel API 输出 `fixed_fee`（StringFixed(2)）。
- **对我们实现的要求**:
  - (1) 渠道保存时校验 fee_rate∈[0,100]、fixed_fee∈[0,9999.99]；手续费只加在"在线支付部分"(total - wallet_paid)，钱包全额抵扣时不收；所有金额 Decimal 两位四舍五入；payment 快照 fee_rate/fixed_fee/fee_amount，回调金额与 payable 比较。
  - (2) 手续费 = 固定费 + 比例费，只对"在线支付部分"（总额 − 钱包抵扣）计算，四舍五入 2 位后再加总；校验 0 ≤ fixed_fee ≤ 9999.99、0 ≤ fee_rate ≤ 100（原 diff 未显式校验 fixed_fee 上下限，我们要补上，负数必须拒绝）；payment 行快照 fee_rate/fixed_fee/fee_amount。
- **必测用例**:
  - (1) online=100, rate=2.5, fixed=1 → fee=3.50, payable=103.50；fixed=-1 保存渠道 → 400；fixed=10000 → 400；钱包全额支付 → fee=0；钱包部分抵扣 30、total 100 → 手续费基于 70 计算。
  - (2) 在线 100.00、fee_rate 2.5、fixed 1.00 → fee 3.50、payable 103.50；fixed_fee=-1 → 400；订单 100 钱包抵扣 60 → 仅对 40 计费；钱包全额支付 → fee 0。

#### PAY-13 出站网关请求绑定在入站 HTTP 请求上下文，客户端断开即取消创建/查询

- 提交: 7423cf94 2026-03-11, fa8f9fbf 2026-03-11
- 严重度: **高**
- 问题现象: 浏览器跳转/刷新、Telegram Bot 客户端超时、第三方 webhook 连接中断时，`input.Context`（gin request ctx）被取消，导致正在进行的 epay/epusdt/tokenpay/paypal/alipay/wechat/stripe `CreatePayment`、PayPal `CaptureOrder`、Wechat/Stripe 查询、PayPal webhook 验签、微信 webhook 解密 以及 Telegram 测试通知被中途取消——网关侧可能已建单/已扣款，本地却记为失败。
- 根因: 直接把请求 ctx 透传给外部 HTTP 调用。
- 原修复方式: 新增 `detachOutboundRequestContext(parent)` = `WithDefaultTimeout(context.WithoutCancel(parent))`（保留 trace 值，忽略父取消，自带默认超时），在 `applyProviderPayment`、`capture*Payment`、`HandlePaypalWebhook`、`HandleWechatWebhook`、`NotificationService.SendTest(telegram)` 统一使用。测试 `TestDetachOutboundRequestContextIgnoresParentCancel`。
- **对我们实现的要求**: axum/hyper 在客户端断开时会 drop handler future，比 Go 更激进。所有"调用外部网关 + 落库结果"的关键段必须 `tokio::spawn` 出去（或用不可取消的任务）并 `tokio::time::timeout` 包裹（约 10–15s），handler 只 await JoinHandle；DB 状态更新必须在同一个被 spawn 的任务里完成，保证网关成功时本地一定落库。
- **必测用例**: 模拟网关延迟 2s，客户端在 0.5s 断开 → 网关调用仍完成，payment 记录 pay_url/provider_ref 并为 pending（非 failed）；网关超过超时 → payment 标记 failed 并释放；webhook 处理中断开连接 → 验签与状态更新照常完成。

#### PAY-14 删除未签名的通用支付回调入口（任何人可伪造支付成功）

- 提交: e31a89fa 2026-02-26
- 严重度: **高**
- 问题现象: `POST /api/v1/payments/callback` 在依次尝试 Wechat/Alipay/Epay/Epusdt 专用处理器都不匹配后，会兜底 `c.ShouldBind(&PaymentCallbackRequest)`，直接用请求体里的 `payment_id/order_no/channel_id/status/amount/currency/provider_ref/paid_at` 调用 `PaymentService.HandleCallback`。攻击者发送 `{"payment_id":1,"status":"success","amount":"xx","currency":"CNY"}` 且无任何签名即可把支付标记成功、触发发货。
- 根因: 开发期的"测试回调"代码残留在生产路由，走了无签名校验的通用路径。
- 原修复方式: `internal/http/handlers/public/payment.go` 删除 `PaymentCallbackRequest` 及兜底逻辑；所有专用处理器都未识别时记录 `payment_callback_unrecognized` 日志并返回 `400` + `constants.EpayCallbackFail`（纯文本 "fail"）。新增 `payment_callback_test.go::TestPaymentCallbackRejectUnknownPayload`。
- **对我们实现的要求**: 回调路由只能进入"按网关验签"的处理器；不存在任何接受明文 status/amount 的通用回调或 debug 回调接口（包括 feature flag/debug 模式下）。未识别的回调一律 400 + "fail"，且不得调用 HandleCallback。
- **必测用例**: POST `/api/v1/payments/callback` Content-Type: application/json, body `{"payment_id":1,"status":"success"}` → HTTP 400，body == "fail"，数据库中 payment/order 状态不变。

#### PAY-15 按站点币种校验渠道：官方微信/支付宝只能 CNY

- 提交: 72fc0ae1 2026-02-22, 13df6b8d 2026-02-22, 5b86ec61 2026-02-22
- 严重度: **高**
- 问题现象: 原来商品有 `price_currency` 列，购物车/订单按商品币种，混币种会 `ErrOrderCurrencyMismatch`；站点币种改为 USD 等时，官方微信/支付宝渠道仍会以 CNY 下单，造成金额按错误币种收款。
- 根因: 币种来源分散（商品级），支付创建时未校验渠道支持的币种。
- 原修复方式: 删除 `products.price_currency` 列（`models/db.go` 迁移后 DropColumn）；新增 `SettingService.GetSiteCurrency`、`normalizeSiteCurrency`（`^[A-Z]{3}$`，非法回退 `CNY`）；订单/购物车统一用站点币种；`validatePaymentCurrencyForChannel`：币种非 3 位大写 → `ErrPaymentCurrencyMismatch`；`provider_type==official` 且 `channel_type` 为 wechat/alipay 时币种必须为 CNY；在 CreatePayment 锁单后与钱包充值中都调用；钱包充值/管理员调账未传币种时取站点币种。
- **对我们实现的要求**: 全站唯一币种来自 site_config.currency（校验 3 位大写字母，非法回退 CNY）；订单创建时写入该币种；创建支付（订单和充值）必须校验渠道-币种兼容。
- **必测用例**: site currency=USD，用官方支付宝渠道支付 → ErrPaymentCurrencyMismatch；settings 保存 currency="usd" → 存储为 "USD"；"US" → "CNY"；钱包充值不传 currency → 使用站点币种。

#### PAY-16 同步回跳（return_url）必须携带订单/网关标记参数，返回页对所有网关触发查单

- 提交: 3e8f79c6 2026-03-06, 3e7de050 2026-03-06; f028b729 2026-06-10
- 严重度: **中**
- 合并条目: (1) 支付返回页只识别 epay_return，其他网关同步回跳不触发查单；BEpusdt return_url 缺少标记参数 ｜ (2) 同步跳转 return_url 丢失订单参数
- 问题现象:
  - (1) 从支付宝/微信/BEpusdt/TokenPay 支付完成跳回 `/payment?order_no=...&xxx_return=1` 后，前端 `syncEpayReturnIfNeeded` 只认 `epay_return=1`，页面不主动同步状态，用户看到"未支付"；epusdt 的 return_url 未拼 `buildOrderReturnQuery(order,"epusdt_return","")`。
  - (2) USDT 类 adapter 直接用 input.ReturnURL，未追加 input.ReturnURLQuery（如 order_no、`xxx_return=1` marker），也不回退渠道配置 cfg.ReturnURL，用户支付后回到前端找不到订单/不触发支付结果检查。
- 根因:
  - (1) 回跳标记与前端识别列表未统一。
  - (2) 各 adapter 实现不一致。
- 原修复方式:
  - (1) 后端 epusdt 分支 `returnURL = appendURLQuery(returnURL, buildOrderReturnQuery(order, "epusdt_return", ""))`；前端改为 `syncPaymentReturnIfNeeded`，识别 `['epay_return','alipay_return','wechat_return','epusdt_return','tokenpay_return']` 任一为 '1' 即调用同步。
  - (2) returnURL = trim(input.ReturnURL) 为空则 trim(cfg.ReturnURL)，再 appendQueryParams(returnURL, input.ReturnURLQuery)。
- **对我们实现的要求**:
  - (1) 每个网关的 return_url 都必须追加 `order_no` 与 `<gateway>_return=1`；前端支付页对全部标记做统一处理（维护同一份常量表），回跳后主动调用查单/同步接口，而不仅等待异步回调。
  - (2) 统一 build_return_url(input, cfg) 工具函数，所有网关复用；追加参数需正确处理已有 `?`。
- **必测用例**:
  - (1) 分别以五种 marker 回跳 → 前端都调用一次同步接口；epusdt 创建支付时发出的 redirect_url 含 `order_no=` 与 `epusdt_return=1`。
  - (2) cfg.return_url="https://shop/pay?x=1"、input 为空、query={order_no:"A1",bepusdt_return:"1"} → "https://shop/pay?x=1&order_no=A1&bepusdt_return=1"（或等价编码）。

#### PAY-17 商品绑定的支付渠道已停用/删除时残留

- 提交: 78e34e2b 2026-05-30
- 严重度: **中**
- 问题现象: 商品 payment_channel_ids 中包含已停用或已删除渠道 ID，下单时可选渠道为空或引用失效渠道。
- 根因: 保存时未过滤。
- 原修复方式: ProductService.filterAvailablePaymentChannelIDs：去重、去 0，按 ListByIDs 查询仅保留 IsActive 的渠道（软删除的查不到即剔除），全部失效则存 nil（=不限制）。Create/Update 都调用。
- **对我们实现的要求**: 保存商品时过滤无效渠道 ID；下单获取可用渠道时同样再过滤 active。注意“全部失效 → 不限制”的语义与原版一致。
- **必测用例**: 传 [活跃1, 停用2, 已删3, 1, 0] → 存 [1]；全部无效 → 存空（不限制）。

#### PAY-18 回调 query 使用 `;` 分隔或 `&amp;` 转义时无法解析

- 提交: 0ad0eb79 2026-05-12
- 严重度: **中**
- 问题现象: 部分网关/中间层把回调 query 写成 `?pid=2026;out_trade_no=ORDER-1;...` 或 `&amp;` 分隔；Go 1.17+ ParseForm 报 "invalid semicolon separator"，或参数名变成 `amp;out_trade_no`，回调匹配失败（issue 170）。
- 根因: 标准库拒绝非标准分隔符。
- 原修复方式: PaymentCallback 入口先 normalizeCallbackRequestQuery：RawQuery 中 `&amp;`→`&`，`;`→`&`，再 parseCallbackForm。
- **对我们实现的要求**: axum 回调入口在解析前对 RawQuery 做同样的规范化（注意只对 query，不改 body；签名按规范化后的键值对计算）。
- **必测用例**: `/payments/callback?pid=2026;out_trade_no=ORDER-1;trade_status=TRADE_SUCCESS;sign=abc` 与 `&amp;` 版本均解析出 out_trade_no=ORDER-1、sign=abc。

#### PAY-19 管理端编辑渠道配置时，清空的字段必须真正删除（汇率残留）

- 提交: 2714cc97 2026-04-20
- 严重度: **中**
- 问题现象: 管理员清空 epay/paypal/stripe/alipay/wechat 渠道的 `target_currency` 或 `exchange_rate` 后保存，旧值仍然存在，继续按旧汇率换算支付金额。
- 根因: 提交时用 `{...configJson(旧), ...buildXxxConfig()}` 合并，而 build 函数 `setIfNotEmpty` 会跳过空值，于是旧 key 保留下来。
- 原修复方式: 合并前 `delete configJson.target_currency / exchange_rate`（okpay 只删 exchange_rate）。
- **对我们实现的要求**: 后端更新渠道配置时采用"整体替换已知字段"，或显式允许传 null 删除字段，不做静默的浅合并；前端表单为空时发送 null 或省略，并且后端按替换语义处理。敏感字段（密钥）保留"不回显、空值表示不修改"的约定，要与普通字段区分对待。
- **必测用例**: 渠道原来 exchange_rate=7.2，编辑时清空并保存，GET 返回中不再有 exchange_rate，支付金额按 1:1 计算。

#### PAY-20 支付返回链接区分订单/充值（biz_type、recharge_no），兼容旧链接

- 提交: 63515716 2026-03-23, b0011b9b 2026-03-23, 30a0cb2c 2026-03-24
- 严重度: **中**
- 问题现象: 钱包充值走在线支付，返回链接只带 `order_no=WR...`（充值单号）且无 guest 标记区分，前端 /pay 页当成普通订单查询 → 404/显示"订单不存在"，充值状态无法刷新。
- 根因: `buildOrderReturnQuery` 只知道订单，充值用虚拟 order 调用。
- 原修复方式: 后端改为 `buildPaymentReturnQuery(input, order, marker, sessionID)`：输出 `biz_type`（order/recharge）；recharge 时输出 `recharge_no` 且不输出 order_no/guest；order 时 `order_no`，userID==0 输出 `guest=1`；另附 `{marker}=1` 与 `session_id`；所有网关（epay/epusdt/okpay/tokenpay/paypal/alipay/wechat/stripe）统一使用。前端兼容旧链接：无 recharge_no 但 `order_no` 以 `WR`（不区分大小写）开头即视为充值。
- **对我们实现的要求**: 返回 URL 构造集中为一个函数，参数含 biz_type/业务号/guest/网关 marker；前端 /pay 页先判断 biz_type=recharge 或 WR 前缀 → 走充值查询接口。
- **必测用例**: 游客订单 → query 含 biz_type=order&order_no=DJ..&guest=1&epay_return=1；充值 → biz_type=recharge&recharge_no=WR..，无 order_no/guest；前端访问 `/pay?order_no=WR123&epay_return=1` → 调用充值查询。

#### PAY-21 回调全链路可追溯日志（校验失败原因必须可见）

- 提交: e73abf8b 2026-02-26
- 严重度: **中**
- 问题现象: 回调被拒（渠道不匹配、订单号不匹配、币种/金额不匹配、验签失败）时无日志，线上无法排查"已付款未到账"。
- 根因: 各处理分支静默失败。
- 原修复方式: `payment.go` 每个网关记录 received（client_ip、body_size、签名头 `Paypal-Transmission-Id/Time/Sig/Auth-Algo/Cert-Url`、`Stripe-Signature`、`Wechatpay-Signature/Timestamp/Nonce/Serial`、原始 body/form，单值截断 4096 字符并加 "...(truncated)"）、各失败分支 warn、processed info；`payment_service.HandleCallback` 对 `payment_callback_channel_mismatch/order_no_mismatch/currency_mismatch/amount_mismatch/idempotent_success/idempotent_same_status`，钱包充值回调同样记录。
- **对我们实现的要求**: 用 tracing 为每个回调记录结构化字段；各拒绝原因用独立事件名；原始 body 截断到 4KB。同时说明 HandleCallback 必须做的校验：channel_id、order_no（或 recharge_no）、currency、amount 均与存储一致；已成功再次回调幂等返回成功。
- **必测用例**: 金额不一致的回调 → 拒绝且日志包含 stored_amount/callback_amount；同一成功回调重复两次 → 第二次幂等成功，不重复发货/加余额。

#### PAY-22 网关展示商品名：统一使用订单号；支付宝公共参数放 URL 防乱码

- 提交: 8fdacca0 2026-08-09, 27e42b49 2026-08-14, d18ee179 2026-08-14, d0f0dc7e 2026-08-11, 21c7cdda 2026-08-09
- 严重度: **低**
- 问题现象: (1) `buildOrderSubject` 先是只看第一个 item（父订单无 items 时直接回退订单号），之后改为遍历 items/children；最终决定所有网关（GlobePay/支付宝/微信/Stripe/PayPal）的 subject 统一为 `order.OrderNo`（trim），方便对账、避免泄露商品名。(2) 支付宝 POST 网关时把 charset 等公共参数放在表单 body，AOP 网关在解析 body 前读不到 charset，中文商品名乱码。
- 根因: subject 取值策略不统一；支付宝协议要求公共参数在 URL query。
- 原修复方式: payment_service_rules.go `buildOrderSubject` 只返回 `strings.TrimSpace(order.OrderNo)`；alipay.go `postGateway`：除 `biz_content` 外的公共参数通过 `buildGatewayPayURL` 放 URL，biz_content 放表单 body，Content-Type 带 `; charset=utf-8`。
- **对我们实现的要求**: 所有网关下单 subject/description 统一用订单号；支付宝 OpenAPI POST 请求：公共参数（app_id、method、charset、sign_type、sign、timestamp、version、notify_url 等）进 URL query，仅 biz_content 进 body。
- **必测用例**: 创建含中文商品的订单调用支付宝预下单，抓取请求：URL 含 charset=utf-8 与 sign，body 只有 biz_content；各网关 subject == 订单号。

### 支付 · 钱包余额支付


#### PAY-23 “仅钱包余额支付”模式服务端强制 + 下单前预校验余额；读设置不放在行锁事务里

- 提交: 5a433cf3 2026-03-31, c5a9c6ba 2026-03-31, 16d01122 2026-03-31, ea26759f 2026-04-01; 2bade0f3 2026-04-08
- 严重度: **高**（高（支付方式约束）；中）
- 合并条目: (1) "仅钱包余额支付"模式必须由服务端强制；读设置不要放在行锁事务里 ｜ (2) 仅钱包支付模式下单前预校验余额（含 Bot）
- 问题现象:
  - (1) 开启 wallet_only_payment 后，前端隐藏了在线渠道，但直接调用 `POST /payments` 带上 channel_id 仍能在线支付；`/channel/payment-channels` 等接口仍然返回渠道。第一版把 `GetWalletOnlyPayment()` 放在 `SELECT ... FOR UPDATE` 锁订单的事务内部调用，SQLite 单连接池下设置查询要再拿一个连接，结果死锁（自锁）。
  - (2) 开启 wallet_only_payment 后，Bot 渠道下单仍会创建订单并锁库存，之后才发现余额不足；游客在该模式下也能下单。
- 根因:
  - (1) 约束只做在 UI 上；在事务内用了非事务连接读数据。
  - (2) 余额校验只在 Web 前端做了。
- 原修复方式:
  - (1) CreatePayment 在事务外先读取 walletOnly：开启时强制 `input.UseBalance=true`，`ChannelID!=0` 直接返回 `ErrWalletOnlyPaymentRequired`。事务内扣完余额后如果还需要在线支付（channel==nil），返回 ErrWalletOnlyPaymentRequired，事务回滚，已扣的余额随之恢复。`GetAllowedChannelsForProducts` 在该模式下返回空列表。public config 下发 `wallet_only_payment`。前端 Payment 页勾选并禁用余额选项，余额不足时显示提示并禁止提交。
  - (2) `OrderService` 在锁库存之前检查：wallet_only 且 `UserID==0` → `ErrWalletOnlyPaymentRequired`；余额 < `result.TotalAmount` → `ErrWalletInsufficientBalance`。渠道支付渠道列表返回 `wallet_only_payment:true`。
- **对我们实现的要求**:
  - (1) 在 sea-orm 事务闭包内，所有读取都必须使用同一个 txn（`&txn`），不能调用内部自己取连接的 service（SQLite 下连接池大小为 1 时会自锁）。配置在事务前读取并传入。wallet_only 下余额不足必须整体回滚，不能留下部分扣款。所有返回支付渠道的接口（public、channel、Telegram bot）都要遵守这个开关。
  - (2) 在 Rust 下单服务中、锁库存之前做这个校验（最终扣款仍以支付时的原子扣减为准）。
- **必测用例**:
  - (1) ①wallet_only 开启，带 channel_id=1 创建支付返回 wallet_only_payment_required；②余额 5、订单 10，返回错误，余额仍为 5，order.wallet_paid_amount=0；③余额 20、订单 10，订单变为 paid，余额 10，生成 provider=wallet 的 payment；④SQLite 下以上流程不死锁（设置连接池为 1 跑测试）。
  - (2) wallet_only 模式下游客下单被拒绝；余额 5、订单 10 时被拒绝且库存不变；余额充足时成功。

#### PAY-24 钱包全额支付也要生成支付记录，但统计需排除

- 提交: db150df5 2026-02-26
- 严重度: **中**
- 问题现象: 订单完全用余额抵扣时不生成 payment 行，后台支付流水/通知模板中渠道为空、无法对账；而若生成后直接计入仪表盘，又会把余额支付重复算进"在线支付成功额/成功率/Top 渠道"。
- 根因: 余额支付路径缺少支付记录；统计未区分 provider。
- 原修复方式: `CreatePayment` 在 `onlineAmount<=0` 分支创建 `Payment{ChannelID:0, ProviderType:"wallet", ChannelType:"balance", InteractionMode:"balance", Amount:walletPaidAmount, Status:success, PaidAt:now}` 后 `markOrderPaid`；dashboard 的 `onlinePaymentBase` 及 `GetTopChannels` 统一加 `provider_type <> 'wallet'`。测试 `TestCreatePaymentWalletFullAmountCreatesPaymentRecord`、`TestPaymentStatsExcludeWalletProvider`。
- **对我们实现的要求**: 余额全额支付在同一事务中写 payment(provider=wallet) + 订单置 paid；所有"在线支付"统计 SQL 过滤 provider_type='wallet'。
- **必测用例**: 余额足额下单 → 生成 1 条 wallet/success payment，订单 paid；仪表盘支付总数/成功金额不包含该条。

### 支付 · Stripe


#### PAY-25 checkout.session.completed 以 payment_status 判定成功

- 提交: 2f870d5e 2026-09-15
- 严重度: **高**
- 问题现象: 延迟到账支付方式（如 SEPA、Boleto、银行转账）下 Stripe 会在 `payment_status=unpaid` 时就发送 `checkout.session.completed`；原代码按事件类型直接映射为 success，未付款即发货。
- 根因: 用事件类型而非 session 的 payment_status 判定。
- 原修复方式: stripe.go `fillWebhookResult`：事件类型为 `checkout.session.completed` 时不走 `mapEventTypeStatus`，改用 `mapCheckoutSessionStatus(payment_status, status)`（paid 才算成功）；其余事件（如 async_payment_succeeded）仍按类型映射。
- **对我们实现的要求**: Stripe webhook 处理 `checkout.session.completed` 必须检查 `payment_status == "paid"`（或 no_payment_required 视业务），unpaid 视为 pending；同时处理 `checkout.session.async_payment_succeeded/failed`。
- **必测用例**: completed 事件 + payment_status=unpaid → 订单保持待支付；completed + paid → 成功；async_payment_succeeded → 成功。

#### PAY-26 换汇渠道回调金额守恒误判 & 应付金额币种展示

- 提交: ae428601 2026-08-28, 0c6a50a7 2026-08-29
- 严重度: **高**
- 问题现象: Stripe 配置 target_currency=GBP、exchange_rate=0.11，订单 1 CNY 下单 0.11 GBP，payment.Amount/Currency 回写为 0.11 GBP；回调金额守恒校验拿 0.11 与订单 1 CNY 比较 → 误判欠付、不发货（并转入余额）。另外 `payable_amount` 返回结算币种数值但无币种字段，前端显示成 "0.11 CNY"；随后修复又把手续费误标为 GBP（手续费 FeeAmount 始终是订单币种，如 4.27 CNY 显示成 4.27 GBP）。
- 根因: 不同币种数值直接比较；响应缺少币种字段。
- 原修复方式: payment_service_rules.go 新增 `paymentExchangeRate(payment)` 读取 `ProviderPayload["exchange_rate"]`（正数才有效）；`paymentCoveredOrderAmount(payment, callbackAmount)`：有汇率时 covered = callbackAmount(为 0 则用 payment.Amount) / rate；无汇率但有 `original_amount` 快照时用原价兜底；再按手续费策略扣除用户承担手续费并 Round(2)。回调本身仍由 validateCallbackPaymentFacts 要求回调金额精确等于 payment.Amount（少付 0.05 GBP 直接拒绝）。presenter 的 CreatePaymentResp/LatestPaymentResp 增加 `currency`=payment.Currency；前端 payableAmountDisplay 用 currency，customerFeeAmountDisplay 用 order.currency。
- **对我们实现的要求**: 支付记录必须保存创建时的汇率快照与原始订单币种金额；金额守恒一律换算回订单币种后比较（注意除法精度，结果 round 到 2 位）；API 返回 payable_amount 时同时返回 currency；手续费展示用订单币种。
- **必测用例**: 订单 1 CNY、rate 0.11、回调 0.11 GBP → 足额，订单履约；回调 0.05 GBP → 验证失败拒绝；无换汇渠道行为不变；latest payment 接口返回 currency=GBP。

#### PAY-27 Checkout 启用 wechat_pay 时未声明 client，Stripe 返回 400

- 提交: a5bfa3d5 2026-07-02
- 严重度: **中**（中（渠道完全不可用））
- 问题现象: 渠道 payment_method_types 含 `wechat_pay`，创建 `/v1/checkout/sessions` 返回 400。
- 根因: Stripe 要求 Web Checkout 场景 WeChat Pay 必须带 `payment_method_options[wechat_pay][client]=web`。
- 原修复方式: 遍历 payment_method_types 时，遇到 `wechat_pay` 追加 form 字段 `payment_method_options[wechat_pay][client]=web`。
- **对我们实现的要求**: Stripe 建单时按支付方式追加必要的 method options；仅 wechat_pay 时追加。
- **必测用例**: mock Stripe server：types=[card,wechat_pay] → form 含 client=web；types=[card] → 不含该字段。

### 支付 · PayPal


#### PAY-28 Webhook 验签必须嵌入原始事件字节

- 提交: 32656b42 2026-07-26, cf57b8be 2026-07-29, def7b43c 2026-07-29
- 严重度: **高**
- 问题现象: 调用 `/v1/notifications/verify-webhook-signature` 时把解析后的 map 重新 `json.Marshal` 作为 webhook_event，字段顺序/数字格式/转义（Go 默认转义 `<>&`）变化导致 PayPal 返回 FAILURE，所有 webhook 验签失败（订单无法自动确认）。
- 根因: 签名基于原始字节，重新序列化破坏了一致性。
- 原修复方式: paypal.go `VerifyWebhookSignature(ctx, cfg, headers, rawEvent []byte)`：webhook_id 必填；rawEvent 非空且 `json.Valid`；从 header 取 `Paypal-Transmission-Id/Time`、`Paypal-Cert-Url`、`Paypal-Auth-Algo`、`Paypal-Transmission-Sig`，任一为空拒绝；`marshalWebhookVerifyRequest`：先 marshal 元数据对象，去掉末尾 `}`，拼接 `,"webhook_event":` + 原始字节 + `}`；响应 verification_status 大写后需为 SUCCESS。
- **对我们实现的要求**: Rust 中用 `serde_json::value::RawValue`/手工拼接保证 webhook_event 为原始请求体字节，不得 `serde_json::to_string(&Value)` 重新序列化。
- **必测用例**: 含中文、`<`、浮点 `10.50` 的事件体 → 发给 PayPal 的 verify 请求体中 webhook_event 与原始字节逐字节相同；缺任一 header → 直接失败不请求 PayPal。

#### PAY-29 目标货币 + 汇率换算

- 提交: 0c7bc551 2026-03-27, f7eb296a 2026-03-27
- 严重度: **高**
- 问题现象: 站点币种 CNY，PayPal 不支持 CNY 收款，需要按汇率换成 USD 下单。
- 根因: 原来直接用 payment.Amount/payment.Currency 创建 PayPal 订单。
- 原修复方式: paypal Config 增加 `target_currency`、`exchange_rate`；`NeedsCurrencyConversion()` = 两者都非空；`ConvertAmount` = amount×rate Round(2)，rate<=0 报 ConfigInvalid；创建订单用换算后金额与目标币种，ProviderPayload 记录 `converted_amount/converted_currency/exchange_rate/original_amount/original_currency`。
- **对我们实现的要求**: PayPal capture/webhook 校验金额与币种时必须使用持久化的 converted_amount/converted_currency（而不是 payment.amount/CNY），否则要么全部误拒要么被迫关掉金额校验；target_currency 需大写规范化；rate 必须 >0。
- **必测用例**: 订单 72.00 CNY、target=USD、rate=0.1389 → PayPal 下单 10.00 USD；capture 返回 10.00 USD → 成功；capture 返回 9.99 USD 或币种 CNY → 拒绝；未配置 target_currency 时按原币种原金额下单。

#### PAY-30 Webhook 必须强制验签且成功事件必须带合法金额

- 提交: 170099e9 2026-02-26
- 严重度: **高**
- 问题现象: (1) 渠道配置 `webhook_id` 为空时 `HandlePaypalWebhook` 直接跳过 `paypal.VerifyWebhookSignature`，任何人可 POST 伪造 `PAYMENT.CAPTURE.COMPLETED` 事件。(2) 金额解析失败/缺失时 amount 为 0，被当作"无金额"继续处理成功回调；`CHECKOUT.ORDER.COMPLETED` 事件金额在 `purchase_units[0].amount` 或 `purchase_units[0].payments.captures[0].amount`，旧代码只读 `resource.amount`，读不到。
- 根因: 验签是可选的（配置缺失即降级为不验）；金额缺失未被视为错误。
- 原修复方式: `paypal.ValidateConfig` 增加 `webhook_id is required`；`HandlePaypalWebhook` 无条件调用 `VerifyWebhookSignature`；`WebhookEvent.CaptureAmount()` 依次回退 `resource.amount` → `purchase_units.0.amount` → `purchase_units.0.payments.captures.0.amount`；新增 `buildPaypalCallbackAmount(event,status)`: status==success 时 value/currency 任一为空→`ErrPaymentGatewayResponseInvalid`；value 非数字或 <=0 → 错误；有 value 无 currency → 错误；pending 允许两者都空；currency 转大写。
- **对我们实现的要求**: PayPal 渠道保存配置时 webhook_id 必填；webhook 处理器永远验签（签名验证失败或配置缺失时拒绝，不降级）。成功状态回调必须解析出 >0 金额与 3 位币种，再与 payment 记录比对金额/币种，不一致拒绝。金额提取实现三级回退。
- **必测用例**: ① 配置无 webhook_id → 保存配置报错；② 成功事件 resource 仅 `{"status":"COMPLETED"}` → 拒绝；③ amount.value="invalid" → 拒绝；④ pending 事件无金额 → 允许（金额 0，币种空）；⑤ `purchase_units[0].amount={value:"88.66",currency_code:"USD"}` → 解析出 88.66/USD；⑥ `amount.currency_code:"usd"` → USD。

#### PAY-31 webhook_id 必填导致无法保存渠道；但验签时必须有

- 提交: 7031b506 2026-05-26, 993ba8d6 2026-05-26
- 严重度: **中**
- 问题现象: ValidateConfig 要求 webhook_id，新建渠道时尚未在 PayPal 后台创建 webhook（需要先有回调地址），无法保存渠道。
- 根因: 把运行时依赖提前到配置校验。
- 原修复方式: ValidateConfig 删除 webhook_id 必填；VerifyWebhookSignature 中 webhook_id 为空返回 ErrConfigInvalid（测试 TestVerifyWebhookSignatureRequiresWebhookID）。
- **对我们实现的要求**: PayPal 渠道允许 webhook_id 为空保存；webhook 到达时若 webhook_id 为空则拒绝（不能放行未验签的回调），依赖主动 capture 完成支付。
- **必测用例**: 无 webhook_id 配置保存成功；收到 webhook 时返回配置错误且订单状态不变。

### 支付 · 微信支付


#### PAY-32 微信支付 API 应答必须验签；支持平台证书与微信支付公钥两种模式

- 提交: c6eeb271 2026-08-07, 1ff3b3a6 2026-08-07
- 严重度: **高**
- 问题现象: 原 `createAPIClient` 使用 `option.WithoutValidator()`，调用微信支付 API（下单、查单）的应答不做签名校验，可被中间人伪造查单结果；新商户只提供"微信支付公钥"（无平台证书）时无法验签回调。
- 根因: 客户端关闭了应答校验；只支持平台证书下载器。
- 原修复方式: wechatpay/verification.go：配置新增 `verification_mode`、`wechatpay_public_key_id`、`wechatpay_public_key`；`createWechatPayVerifier` 按模式选择平台证书（自动下载器）或公钥 verifier，API client 与回调统一用同一 verifier；`responseSignatureValidator` 把应答验签失败映射为 ErrSignatureInvalid（区分网络错误）；`validateVerificationConfig` 校验配置完整；后台新增"测试公钥"接口。
- **对我们实现的要求**: Rust 微信支付 v3 客户端对所有 API 应答校验 `Wechatpay-Signature`/`Wechatpay-Serial`/`Wechatpay-Timestamp`/`Wechatpay-Nonce`；同时支持平台证书与公钥（按 serial/公钥ID 匹配）模式；验签失败不得视作支付成功或失败，而是报错重试。
- **必测用例**: 伪造应答（错误签名）查单 → ErrSignatureInvalid；公钥模式下正确签名的回调 → 通过；serial 不匹配 → 拒绝。

#### PAY-33 微信主动查单用业务订单号导致查不到；前端需主动 capture 微信支付

- 提交: ad9e4af0 2026-05-27, 55ca9640 2026-05-27
- 严重度: **高**
- 问题现象: 微信 CreatePayment 阶段 adapter 返回空 ProviderRef，applyProviderPayment 兜底写 `payment.ProviderRef = order.OrderNo`，但实际提交给微信的是带 DJP 前缀的网关订单号（providerOrderNo / GatewayOrderNo），capture/查单时用 OrderNo 查询永远“订单不存在”，回调丢失时无法补单。前端 Payment.vue 原来只对 PayPal/Stripe 做 capture，微信扫码后页面一直待支付。
- 根因: 业务单号与网关单号混用。
- 原修复方式: 兜底改为 `payment.ProviderRef = providerOrderNo`（测试 TestApplyProviderPaymentFallsBackToWechatGatewayOrderNoWhenProviderRefEmpty 断言 GatewayOrderNo 保留 DJP 前缀）。前端新增 shouldCaptureCurrentPayment()：provider_type==='official' && channel_type==='wechat' && 订单 pending_payment 时，在加载最新支付、轮询、恢复缓存时静默调用 capture（游客走 guestOrderAPI.capturePayment 带 email+order_password）。
- **对我们实现的要求**: payment 表必须同时存 gateway_order_no（实际提交网关的商户单号）与 provider_ref（网关流水号）；查单/关单一律用 gateway_order_no；ProviderRef 为空时以 gateway_order_no 兜底，绝不用 order_no。前端轮询期间对 official wechat 静默 capture。
- **必测用例**: 微信 mock 返回空 provider_ref → 落库 provider_ref == gateway_order_no（DJP 前缀）而非 order_no；capture 接口用该值查单；游客无授权时静默 capture 不报错。

### 支付 · 支付宝


#### PAY-34 回调需校验 app_id 归属，防跨商户回调注入

- 提交: e2e71d82 2026-02-26
- 严重度: **高**
- 问题现象: 支付宝回调仅做 RSA 验签。攻击者用自己的另一个支付宝应用（同样由支付宝平台公钥签名）对相同 `out_trade_no` 下单付款，把该应用的真实签名回调转发到本站，签名能过，从而用别的商户（甚至极小金额另行处理）的付款标记本站订单已支付。
- 根因: 未检查回调 `app_id` 是否等于本渠道配置的 AppID。
- 原修复方式: `alipay.VerifyCallbackOwnership(cfg, form)`：取 `app_id`（兼容 `appid`），为空或与 `cfg.AppID` 不相等（EqualFold）→ `ErrSignatureInvalid`；`HandleAlipayCallback` 在 `VerifyCallback` 之后调用，失败返回 200 + `constants.AlipayCallbackFail`。
- **对我们实现的要求**: 支付宝回调处理顺序：解析表单 → 按 out_trade_no 找 payment → 取渠道配置 → 验签 → 校验 app_id == 配置 AppID → 再解析金额/状态并比对。任何一步失败返回 "fail"。其他网关同理校验商户号（mchid/pid/merchant id）。
- **必测用例**: form 只有有效签名但 `app_id=2026999999999999`（与配置不同）→ 返回 "fail"，订单不变；缺失 app_id → fail；app_id 与配置相同 → 通过。

### 支付 · 易支付（Epay）


#### PAY-35 易支付回调必须校验 pid 归属本渠道商户（防同平台其他商户注入）

- 提交: 46697a45 2026-03-23; f54ee787 2026-06-01
- 严重度: **高**
- 合并条目: (1) 回调必须校验 pid 归属本渠道商户，防同平台其它商户注入 ｜ (2) 回调 pid 未校验，其他商户号的合法签名回调可被接受
- 问题现象:
  - (1) 易支付 V2 用平台 RSA 公钥验签，V1 部分平台共享签名逻辑；同一易支付平台上的另一个商户（攻击者自己的 pid=1002）可以构造一笔自己真实支付的小额订单回调（签名合法），把 out_trade_no/param 改成本站的订单号/payment_id 打到本站 notify_url，验签通过 → 本站订单被标记已支付。
  - (2) epay VerifyCallback 只验签，未校验回调中的 `pid` 是否等于渠道配置 merchant_id；若多个商户共用同一 key 或聚合平台签名规则相同，别的商户号的回调可被当作本渠道入账。
- 根因:
  - (1) `HandleEpayCallback` 只做了 `epay.VerifyCallback`（签名），没有校验回调中的 `pid` 是否等于本渠道配置的 `MerchantID`。
  - (2) 缺少“回调归属”校验。
- 原修复方式:
  - (1) 新增 `epay.VerifyCallbackOwnership(cfg, form)`：`pid` 为空或 `pid != cfg.MerchantID`（均 TrimSpace）一律 `ErrSignatureInvalid`；在 handler 中验签之后、`parseEpayCallback` 之前调用，失败则 enqueue `epay_ownership_invalid` 异常告警并返回 `fail`。
  - (2) epay adapter 在验签后调用 `epay.VerifyCallbackOwnership(cfg, form)`：pid 为空或 != cfg.MerchantID → ErrSignatureInvalid。测试 TestEpayAdapter_VerifyCallbackRejectsMerchantMismatch（pid=1002, 配置 1001，签名正确仍拒绝）。同批还把回调币种比较改为 strings.EqualFold。
- **对我们实现的要求**:
  - (1) 易支付（V1 MD5 / V2 RSA）回调处理顺序固定为：定位 payment→channel → 验签 → **pid==channel.merchant_id（缺失即拒绝）** → 金额/订单号匹配 → 状态流转。任何"平台级公钥"验签的网关（OKPay、易支付 V2 等）都要额外校验商户号字段，且字段缺失时拒绝而不是跳过。
  - (2) 所有带商户号的网关回调（epay pid、alipay app_id/seller_id、okpay 等）验签后必须校验商户号等于渠道配置；币种比较大小写不敏感。
- **必测用例**:
  - (1) (1) V1 合法签名 + pid=1001（配置 1001）→ 成功；(2) pid 缺失 → 拒绝，订单保持 pending；(3) 合法签名但 pid=1002 → 拒绝并记告警；(4) V2 RSA 同样三组。
  - (2) 签名合法但 pid 不同 → 拒绝且 payment 状态不变；pid 缺失 → 拒绝；currency "cny" vs 存储 "CNY" → 通过。

#### PAY-36 下单响应兼容"双重 JSON 编码"与压缩

- 提交: cce035a7 2026-03-22
- 严重度: **中**
- 问题现象: 某些易支付网关 mapi.php / api/pay/create 返回的 body 是顶层 JSON 字符串 `"{\"code\":1,...,\"payurl\":\"...\"}"`，`json.Unmarshal` 到 map 失败被忽略 → 拿不到 payurl/trade_no，下单失败；另有网关返回 gzip 导致解析异常。
- 根因: 只按对象解析响应；请求头未声明 `Accept-Encoding: identity`。
- 原修复方式: `postForm` 增加 `Accept-Encoding: identity`；新增 `normalizeResponseBody`：TrimSpace 后首字符为 `"` 时先解一次得到内层字符串再作为 JSON 解析；createV1/createV2 均调用。
- **对我们实现的要求**: reqwest 调用易支付时显式 `Accept-Encoding: identity`（或启用自动解压）；响应解析先 trim，若是 JSON string 则二次解析；解析失败返回明确的 `ResponseInvalid` 而不是静默当成空对象。
- **必测用例**: mock 网关返回 `"{\"code\":1,\"trade_no\":\"T1\",\"payurl\":\"https://x\"}"` → 得到 trade_no=T1、pay_url；V2 返回双编码 `code:0,pay_type:qrcode,pay_info` → 得到二维码；断言请求头 Accept-Encoding=identity。

#### PAY-37 跳转模式（submit.php）与交互模式校验

- 提交: 777eabdd 2026-03-20, 7a06965d 2026-03-20
- 严重度: **中**
- 问题现象: 部分易支付平台不开放 mapi 接口，只能页面跳转；且渠道 interaction_mode 随便填也能保存，下单时走错分支。
- 根因: 只实现了 API 下单模式；`ValidateChannel` 不校验 epay 的 interaction_mode。
- 原修复方式: `BuildRedirectURL`：V1 → `{gateway}/submit.php`，参数 pid/type/out_trade_no/notify_url/return_url/name/money(/param) MD5 签名；V2 → `/api/pay/submit` 额外 timestamp + RSA 签名；空值参数不进入 query；`payment.PayURL` 设为跳转链接、QRCode 清空、ProviderPayload 记录 `mode/endpoint/params`。`applyProviderPayment` 中 mode==redirect 走跳转，mode 非空且非 qr 报配置错误；`ValidateChannel` 对 epay 要求 mode ∈ {qr, redirect}。
- **对我们实现的要求**: 易支付渠道 interaction_mode 仅允许 qr/redirect，保存时校验；redirect 模式本地生成签名 URL（不请求网关），签名串与回调验签共用同一 `build_sign_content`（按 key 排序、排除 sign/sign_type/空值）。
- **必测用例**: redirect+V1 生成 URL 路径 `/submit.php`、sign=md5(sorted+key)；redirect+V2 含 timestamp 且 RSA 可被公钥验证；mode="foo" 保存渠道 → 400。

### 支付 · OKPay


#### PAY-38 OKPay 协议升级 HMAC-SHA256 + timestamp/nonce，回调仅接受 JSON，嵌套键用点号

- 提交: ad9b7e2d 2026-08-29, 5e0bc35c 2026-08-29
- 严重度: **高**
- 问题现象: OKPay 官方协议由 `MD5(query&token=xxx)` 改为 HMAC-SHA256，旧实现签名全部失败；旧代码还保留 form-encoded 回调解析与"排序后再验一次"的兜底验签，扩大了攻击面。
- 根因: 上游协议变更。
- 原修复方式: okpay.go `SignPayload` 追加 `id`(商户号)、`timestamp`(Unix 秒)、`nonce`(16 字节随机 hex) 后按 key ASCII 升序拼 `k=v&...`（值不 URL 编码，空值跳过），`HMAC-SHA256(key=merchantToken)` 转大写 hex。`ParseCallback` 只接受 `{` 开头的 JSON，嵌套字段展开为 `data.order_id`、`data.amount`、`data.coin`、`data.status` 等（原 `data[order_id]`）；`VerifyCallback` 去掉 sign 后排序只验一次。
- **对我们实现的要求**: 按新协议实现；JSON 解析需保留数字原始字符串（UseNumber，不重新格式化）以参与签名；只验一种签名方式，不做兜底。建议额外校验回调 timestamp 在允许窗口内（原实现未做，可增强）。
- **必测用例**: 用官方示例参数+token 计算签名与期望值一致；form-encoded 回调 → 拒绝；amount 为 JSON 数字 `10.50` 时参与签名的字符串保持 `10.50`。

#### PAY-39 JSON 回调无法识别 + 汇率为 1 时 currency 与网关币种不一致导致回调被拒

- 提交: 41f64f42 2026-06-18
- 严重度: **高**（高（真实已支付订单无法入账））
- 问题现象: ①OKPay 回调可能是 JSON body（`{"code":200,"data":{"order_id":..,"unique_id":..,"amount":"616.00000000","coin":"USDT",..},"sign":..}`），原探测只用 `url.ParseQuery` 解析 form，JSON 回调被判为不匹配。②店铺订单币种 USD、渠道 coin=USDT、exchange_rate=1 时，payment 记录的 CurrencySent 仍为 USD，回调 coin=USDT → 币种不匹配被拒。
- 根因: 回调解析只支持一种编码；CurrencySent 只在“发生了汇率换算”时才改成 coin。
- 原修复方式: 探测改为 `okpay.ParseCallback(body)`（兼容 form 与 JSON），签名基串仍按 `code=..&data[order_id]=..&...&status=success&token=` 的扁平形式 MD5 大写。adapter：`currencySent` 默认取 `cfg.Coin`（非空时），与是否换算无关。回调仍校验签名、金额（`616.00000000` vs `615` 拒绝）、币种（TRX vs USDT 拒绝），失败时 HTTP 200 + OkpayCallbackFail 让网关重试，订单保持 pending_payment。
- **对我们实现的要求**: 回调解析按 Content-Type/首字符同时支持 form 与 JSON，签名串构造与编码无关；payment 表记录的 amount/currency 必须与实际发给网关的一致（网关 coin 优先）。验签、金额、币种三者任一不符都不能改状态。
- **必测用例**: JSON 回调正确签名 → 支付成功、订单 paid；sign 前加 "BAD" → 返回 fail 且状态不变；金额 615 → fail；coin=TRX → fail；USD 订单 + coin=USDT + rate=1 → CurrencySent=USDT、AmountSent 原值、payload 不含 exchange_rate。

#### PAY-40 汇率换算下单后，回调金额必须按换算后金额校验

- 提交: 60127456 2026-03-21, 636f1d0c 2026-03-21, 40fbe689 2026-03-21, 6ee7ae6b 2026-03-21, d32fd87e 2026-03-21
- 严重度: **高**
- 问题现象: OKPay 以 USDT/TRX 收款，渠道配置 `exchange_rate`（默认 1）。下单金额 = 订单金额 × 汇率（8 位小数）。原来回调里直接把 `data.Amount`（币数量）当成 payment 金额传给通用回调，和 CNY 金额比较必然不一致（或者汇率=1 时恰好相等被误放行）。
- 根因: 通用回调服务按 payment.Amount 比对，但网关侧金额是换算后的币数量。
- 原修复方式: okpay 新增 `ParseExchangeRate`（空→1，<=0 报错）与 `ConvertAmountByRate(amount, rate) = amount×rate Round(8)`；`CreatePayment` 发 `convertedAmount.StringFixed(8)`；ProviderPayload 记录 `converted_amount/exchange_rate`；回调新增 `verifyOkpayCallbackAmount`：按当前渠道汇率重算期望值，与回调金额 `Cmp != 0` 即拒绝并告警；随后传给通用回调的 Amount 置空（跳过通用比较）。签名为 md5(按原始字段顺序 k=v&...&token=)大写，失败时回退按 key 排序再算；商户号仅在回调带 merchant_id 时校验。
- **对我们实现的要求**: (1) 换算后的应付金额、币种、汇率在创建支付时**持久化**到 payment（provider_payload 或独立字段），回调时与持久化值比较，不要用"当前配置汇率"重算（管理员中途改汇率会导致误拒/误放）；(2) 回调金额字段缺失时应视为校验失败而不是跳过（原实现 `callbackAmountRaw==""` 直接 return nil，是漏洞）；(3) 商户号字段缺失也应拒绝；(4) 金额比较用 Decimal，精度 8 位。
- **必测用例**: 订单 100.00、汇率 0.1389 → 下单 amount="13.89000000"；回调 amount=13.89 → 成功；回调 13.88 → 拒绝订单仍 pending；回调缺 amount → 拒绝；下单后改汇率为 0.2，再来 13.89 的回调 → 仍应成功（按持久化值）。

### 支付 · epusdt / BEpusdt


#### PAY-41 epusdt 签名改为 HMAC-SHA256，数值格式与服务端一致

- 提交: a7ba2c1a 2026-07-24
- 严重度: **高**
- 问题现象: epusdt(GMPay) 服务端升级为 HMAC-SHA256 且不再接受 MD5，创建交易返回 401 signature verification failed。
- 根因: 上游协议变更。
- 原修复方式: epusdt.go `Sign`：剔除 signature 与空值，key 升序，`k=v&...` 拼接，`HMAC-SHA256(key=secret_key)` 小写 hex；`formatSignValue`：float64 用 `FormatFloat(v,'f',-1,64)`（去尾零，100.0→"100"），string 原样，其它 `%v`。回调验签同算法（且见上：常量时间比较、空密钥拒绝）。
- **对我们实现的要求**: 按新算法实现；浮点数签名格式必须与 epusdt 的 MapToParams 一致（最短表示、无尾零、不用科学计数法）。
- **必测用例**: 参数 amount=100、currency=cny、network=tron、notify_url、order_id=ORD-1、pid=1000、token=usdt，secret sk-test → 签名等于 HMAC-SHA256("amount=100&currency=cny&network=tron&notify_url=https://example.com/notify&order_id=ORD-1&pid=1000&token=usdt","sk-test") 小写 hex；amount=12.50 → 串中为 12.5。

#### PAY-42 真 epusdt（GMPay）与 BEpusdt 拆分：共用回调入口的特征识别、响应体、历史数据迁移

- 提交: 81d2861f 2026-05-10
- 严重度: **高**
- 问题现象: 原来的 `epusdt` provider 实际对接的是 BEpusdt。接入真 epusdt 后两者共用 `/api/v1/payments/callback` 入口，JSON 字段相似（trade_id/order_id/status/signature），很容易被错误的 handler 接管；两者成功响应也不同（epusdt 返回 `ok`，BEpusdt 返回 `success`）。
- 根因: 回调入口是责任链，特征判定太宽。
- 原修复方式: 新增 provider `bepusdt`，旧代码移到 `internal/payment/bepusdt`。`HandleEpusdtCallback` 必须**同时**有 `pid + trade_id + order_id` 才接管（pid 是强区分特征），否则恢复请求体后返回 false，交给后面的 `HandleBepusdtCallback`。验签：去掉 signature 和空值，key 升序排列成 `k=v&...` 后拼接 secret，做 md5 小写；参与签名字段为 pid/trade_id/order_id/amount/actual_amount/receive_address/token/block_transaction_id/status；status 不是成功时直接拒绝。amount 兼容 float 和 string 两种 JSON 类型。查找支付记录时 order_id 走 gateway_order_no，降级走 trade_id；同时校验渠道 provider_type==epusdt。一次性迁移 `ensurePaymentProviderBepusdtRenameMigration`：`UPDATE payment_channels SET provider_type='bepusdt' WHERE provider_type='epusdt'`，用 settings 标记保证幂等，避免之后新建的真 epusdt 渠道被误改。
- **对我们实现的要求**: Rust 回调分发不要用"逐个尝试"的模糊匹配，优先用独立回调路由 `/callback/{provider}/{channel_id}`；如果保留统一入口，每个 provider 的特征判定必须互斥，并有测试覆盖。改名类数据迁移必须幂等（有迁移版本表或 marker）。签名中数字的格式化方式（如 10 与 10.00）需与网关一致，用网关文档中的样例做测试。
- **必测用例**: 没有 pid 的 body 不被 epusdt handler 接管而交给 BEpusdt；空 body 被放行给下一个 handler；签名错误返回 fail；status 非成功返回 fail；成功时响应体为 "ok"；迁移执行两次结果不变，且不影响新建的 epusdt 渠道。

#### PAY-43 非 paid 状态的回调不得被处理为成功

- 提交: d9b7518f 2026-02-26
- 严重度: **高**
- 问题现象: BEpusdt 会对过期/未支付等状态同样发送带签名的回调；`epusdt.VerifyCallback` 只验签不看 `status`，导致非成功状态回调被当作支付成功（紧急修复）。
- 根因: 验签通过即视为成功，未校验 `data.Status == StatusSuccess`。
- 原修复方式: `internal/payment/epusdt/epusdt.go::VerifyCallback` 在验签前加 `if data.Status != StatusSuccess { return ErrResponseInvalid }`。
- **对我们实现的要求**: 所有网关回调都要把网关状态显式映射（success/pending/failed/expired），只有明确的成功状态才能推进订单为 paid；未知状态不得默认为成功。epusdt/BEpusdt 中 status 成功值为 2（StatusSuccess），其余一律不置成功。
- **必测用例**: 签名正确但 status=1(等待)/3(过期) 的 epusdt 回调 → 不修改 payment/order；status=2 且金额匹配 → 标记成功。

#### PAY-44 BEpusdt：旧 channel_type 迁移、收银台模式与 trade_type/QR 互斥、回调不得覆盖 display_channel_type

- 提交: 3d600625 2026-07-10, 71796452 2026-07-12, 484c94c7 2026-07-15, d64799fb 2026-07-15, d3b3c5ca 2026-07-16; 148e813e 2026-07-03, 94fb9d64 2026-07-03, 3d600625 2026-07-10, 0282d765 2026-07-10, 97852f23 2026-07-12, 935ffe5f 2026-07-12, 484c94c7 2026-07-15, d4aa99a6 2026-07-15
- 严重度: **中**（中（展示/对账数据丢失，通知与后台显示错误币种网络）；中（配置错误会导致按错误币种建单））
- 合并条目: (1) 回调覆盖 provider_payload 导致创建阶段写入的 display_channel_type 丢失 ｜ (2) 旧 channel_type 数据迁移 + 收银台模式与 trade_type/QR 互斥
- 问题现象:
  - (1) BEpusdt/epusdt 新格式下 `payment.channel_type` 固定为 `bepusdt`/`epusdt`，真实交易类型（如 `usdt.arbitrum`、`token.network`）在创建支付时写进 `provider_payload.display_channel_type`；但回调处理 `applyPaymentUpdate` / `updateCallbackMeta` / `applyWalletRechargePaymentUpdate` 用 `payment.ProviderPayload = input.Payload` 整体覆盖，回调后该字段消失，通知和后台列表退化成显示 `bepusdt`。另外 CSV 导出缺少 display_channel_type 列。
  - (2) ①旧渠道 channel_type 为 `usdt`/`usdt-trc20`/`usdc-trc20`/`trx`，trade_type 通过 channel_type 推导；新格式 channel_type 固定 `bepusdt`、交易类型存 config.trade_type。混用时 adapter 每次运行时推导，逻辑脆弱。②cashier（收银台）模式下前端仍保留/提交 `trade_type`，transaction 模式提交残留 `currencies`。③cashier 模式配置成 QR 交互会无法渲染（建单时无固定地址/金额）。④QR 模式原先把 PaymentURL 当二维码，应使用 token（收款地址）。
- 根因:
  - (1) 回调原文与创建阶段元数据共用一个 JSON 字段，更新采用整体替换。
  - (2) 渠道配置的模式字段之间没有互斥校验，旧数据未一次性迁移。
- 原修复方式:
  - (1) 新增 `mergeProviderPayload(existing, incoming)`：复制旧 map，再用回调字段覆盖同名键，未出现的键保留；三处回调更新全部改用 merge。adapter 的 CreateResult 新增 `DisplayChannelType`，`applyProviderPayment` 写入 `provider_payload.display_channel_type`（不改 channel_type 的 DB 语义）；收银台模式返回空（无固定币种）。通知 `notificationPaymentChannel` 优先读 display_channel_type（`notificationPayloadString` 避免 nil 变成 "<nil>"）。后台列表 lightweight 查询用 `jsonTextExpr(provider_payload, display_channel_type)` 单独补读；CSV 导出在 channel_type 后新增 `display_channel_type` 列（`paymentDisplayChannelType` 优先字段，否则从 payload 兜底）。
  - (2) `ensurePaymentChannelBepusdtConfigMigration`（事务 + settings 表 marker，幂等）：provider_type=bepusdt 且 channel_type 为已知旧值 → 写 `trade_type`（缺省 `usdt.trc20`，与旧行为一致）、`order_mode=transaction`、channel_type 改为 `bepusdt`；未知 channel_type 不改动，并把 ID 写入 marker 的 `skipped_channel_ids`。adapter 去掉运行时推导。`ValidateChannel`：bepusdt + interaction_mode=qr + order_mode=cashier → `ErrPaymentChannelConfigInvalid`；CreatePayment 中 cashier+QR 也报错；cashier 模式 channel_type 必须是 `bepusdt`；transaction 模式新格式缺 trade_type 报错。QR 模式 `qrCodeURL = token`、redirectURL 置空，token 为空报 ErrResponseInvalid；payload.data 追加 `chain`/`token_id`（`resolveBepusdtTradeLabels`：`usdt|usdc` 前缀为 token.network，其余为 network.token；trc20→tron、erc20/eth→ethereum、bep20→bsc）。前端 PaymentChannelModal：cashier 时 trade_type 强制为空并 `delete configJson.trade_type`，transaction 时 `delete configJson.currencies`。cashier 支持 `currencies` 限制。
- **对我们实现的要求**:
  - (1) payment 的 provider_payload 在回调更新时必须是“浅合并”（incoming 覆盖同名键），绝不整体替换；展示用渠道类型建议单独建列（display_channel_type）而不是藏在 JSON 里，避免跨方言 JSON 取值。CSV 导出列顺序：`id,order_id,recharge_no,recharge_status,recharge_user_id,channel_id,provider_type,channel_type,display_channel_type,status,amount,currency,created_at,paid_at,expired_at,provider_ref`。
  - (2) 渠道保存时做模式互斥校验（cashier ⇔ 无 trade_type、仅 redirect；transaction ⇔ 必须 trade_type）；迁移要幂等（marker）且对未知数据“不改+记录”，不静默改成错误币种；前端提交时按模式删除互斥字段。
- **必测用例**:
  - (1) 创建 BEpusdt transaction 支付（trade_type=usdt.arbitrum）→ payload.display_channel_type=usdt.arbitrum；投递不含该字段的回调 → 支付成功且 display_channel_type 仍在、回调字段也在；钱包充值回调同样；cashier 模式 display 为空，通知显示 bepusdt；CSV 第 9 列为 usdt.arbitrum。
  - (2) 旧渠道 channel_type=usdc-trc20 无 trade_type → 迁移后 channel_type=bepusdt、trade_type 取推导值、order_mode=transaction；channel_type=foo → 不变且出现在 skipped_channel_ids；迁移跑两次结果不变；cashier+qr 保存被拒；transaction QR 建单 qr_code_url=收款地址；`usdt.arbitrum`→chain=arbitrum、token_id=arbitrum-usdt；`tron.trx`→tron/tron-trx。

#### PAY-45 不向前端暴露 provider_payload；回调 payload 结构化保存

- 提交: e7ae0357 2026-02-13, 8157a58b 2026-02-13, e82950ea 2026-02-13, 0ce2280a 2026-02-13
- 严重度: **低**
- 问题现象: 公开的 CreatePayment/GetLatestPayment/游客支付接口在响应中返回 `provider_payload`（网关原始响应，可能含内部字段）；BEpusdt QR 模式把收款地址 token 当二维码，前端需额外逻辑；回跳 URL 被拼接 `epusdt_return` 参数。
- 根因: 早期集成设计。
- 原修复方式: 从 4 个公开响应删除 `provider_payload`；BEpusdt 仅支持 redirect 模式，`payment.QRCode = result.PaymentURL`，return_url 直接用配置值；回调 payload 用 `json.Marshal(data)` 结构体保存。
- **对我们实现的要求**: 公共支付接口响应不包含 provider_payload（只返回 pay_url/qr_code/状态等必要字段）；BEpusdt 渠道只允许 redirect 交互模式。
- **必测用例**: 用户创建支付响应 JSON 中不存在 provider_payload 字段；后台配置 BEpusdt 渠道选 qr 模式 → 拒绝或不可选。

### 支付 · TokenPay


#### PAY-46 法币币种与加密币种混用导致下单/回调金额币种错误

- 提交: 1e260343 2026-05-30
- 严重度: **高**
- 问题现象: 创建订单时把订单法币 input.Currency(CNY/USD) 传给 TokenPay 的 Currency（应为 USDT_TRC20/TRX）；回调时取 data.Amount（加密币数量）和 data.Currency（加密币种）与 payment.Amount/Currency（法币）严格比对 → 金额/币种永远不一致，回调失败。
- 根因: TokenPay 字段语义：ActualAmount=法币金额，BaseCurrency=法币；Amount=换算后的币数量，Currency=加密币种。
- 原修复方式: CreateInput.Currency = cfg.Currency；回调 amount 取 data.ActualAmount，currency 取 data.BaseCurrency，缺省回退 cfg.BaseCurrency。
- **对我们实现的要求**: 每个 USDT 类网关明确“比对口径”为法币（与 payment.amount/currency 一致）；字段映射写单元测试固定下来。
- **必测用例**: 订单 10.00 CNY，渠道 currency=USDT_TRC20 → 请求体 Currency=USDT_TRC20、ActualAmount=10.00；回调 ActualAmount=10.00/BaseCurrency=CNY/Amount=1.38/Currency=USDT_TRC20 → 校验通过；ActualAmount=9.99 → 拒绝。

#### PAY-47 网关币种代码原样透传，不做大小写转换

- 提交: ae4de74b 2026-03-23
- 严重度: **低**
- 问题现象: TokenPay 的 currency（如自定义链币种标识）大小写敏感，原实现在配置、下单、回调解析中 `ToUpper`，导致下单币种与网关配置不匹配/回调币种比较失败。
- 根因: 过度规范化。
- 原修复方式: tokenpay Config.Currency、CreatePayment、回调 payload Currency 均改为仅 TrimSpace。
- **对我们实现的要求**: 网关特定的币种/通道代码按配置原样传递与比较（只 trim），站点法币代码才统一大写。
- **必测用例**: 配置 currency="USDT_TRC20"/"usdt_trc20" → 下单请求体原样；回调同值 → 匹配成功。

### 支付 · DujiaoPay


#### PAY-48 新网关 webhook 验签规则

- 提交: b9de2466 2026-06-14, 0462e860 2026-06-14, 1cbfe019 2026-06-14
- 严重度: **高**
- 问题现象: （新增网关）DujiaoPay channel_type 为 token_id（tron-usdt/base-usdc），同一 webhook 入口事先不知道渠道。
- 根因: —（功能引入时即内置的防护）
- 原修复方式: 头 `DJP-Webhook-Timestamp`、`DJP-Webhook-Signature`（可带 `sha256=` 前缀）、`DJP-Webhook-ID`；签名 = hex(HMAC-SHA256(webhook_secret, timestamp + "." + rawBody))，hmac.Equal 常量时间比较；时间戳与当前相差 >5 分钟拒绝；缺 webhook_secret 视为配置错误。channel_id 缺失时按 provider_type=dujiaopay 列出 active 渠道逐个验签。event_type=order.paid 映射 success，paid_at 缺省取 created_at。请求 API 时签名头 DJP-Timestamp/DJP-Signature（method、path、query、body、nonce 参与）。注意：webhook 数据中不含金额，业务层无法比对回调金额。
- **对我们实现的要求**: 用原始 body 字节验签（axum 中先取 Bytes 再解析 JSON）；±5 分钟时间窗；常量时间比较；未携带金额的网关，入账前应通过查单接口或以本地 payment.amount 为准并记录 tx_hash，不能信任客户端/回调中的其它金额字段。
- **必测用例**: 正确签名 → 成功；timestamp 偏移 301s → 拒绝；body 改 1 字节 → 拒绝；带/不带 `sha256=` 前缀都能验证；无 channel_id 两个渠道时正确匹配。

#### PAY-49 收银台（延迟分配）模式的 channel_type/order_mode 一致性

- 提交: 030a131b 2026-07-20, aa4bbf6e 2026-07-20
- 严重度: **中**
- 问题现象: DujiaoPay 新增 cashier 模式，建单不传 chain/token_id，付款人在托管收银台自选；若 channel_type 仍填 token_id 或配置 QR 交互，会按固定币种建单或二维码无法渲染。
- 根因: 模式与 channel_type、interaction_mode 之间缺少约束。
- 原修复方式: `Config.Normalize`：order_mode 默认 transaction；cashier 时清空 Chain/TokenID，transaction 时清空 AllowedMethods。`ValidateConfig`：order_mode 只允许 transaction/cashier；cashier 下 `allowed_methods`（逗号分隔，去空格去重）每项必须是受支持 token_id；transaction 必须 token_id。`checkDujiaoPayChannelTypeForMode`：cashier ⇒ channel_type 必须为 `dujiaopay`；transaction ⇒ channel_type 不可为 `dujiaopay`。cashier + interaction_mode=qr 建单报错。建单 payload：cashier 传 `allowed_methods:[{chain,token_id}]`，否则传 chain/token_id。
- **对我们实现的要求**: 同上一条的统一规则：所有“收银台/延迟分配”模式网关只允许 redirect，channel_type 取 provider 名；保存与建单双重校验。
- **必测用例**: cashier + allowed_methods="tron-usdt, base-usdc,tron-usdt" → 请求体 allowed_methods 两项且去重；cashier + 非法 token → 保存失败；cashier + channel_type=tron-usdt → ErrUnsupportedChannel；transaction + channel_type=dujiaopay → 报错；cashier + qr → 报错。

## 5. 发货/卡密（DLV，10 条）


#### DLV-01 卡密出库导出：并发重复出库与数量不足

- 提交: 834b8856 2026-06-01, 02574a29 2026-06-01, 983ce46a 2026-05-25
- 严重度: **高**
- 问题现象: （新增“可用卡密出库导出”）需防止导出与自动发货/并发导出拿到同一批卡密；按筛选导出时未选 ids/batch/filter 报 invalid。
- 根因: —
- 原修复方式: ExportAvailableCardSecrets：product_id 必填、limit>0、format 仅 txt/csv；校验商品为自动发货且 SKU 属于该商品；事务内 ListAvailableByProductBatchForUpdate(product, sku, batch, limit)（FOR UPDATE、id asc）；取到数量 < limit → ErrCardSecretInsufficient（不部分出库）；delete_after_export 则删除否则标记 used；affected 必须等于条数否则回滚；响应头 X-Exported-Count，附件名 card-secrets-available-YYYYMMDD-HHMMSS.{fmt}；新 RBAC 策略。983ce46a：导出无任何筛选条件时导出当前全部结果（resolveExportTargetCardSecretIDs 回退 ListIDs(filter)）。
- **对我们实现的要求**: 出库与发货共用“锁定可用卡密”逻辑：PG/MySQL 用 SELECT ... FOR UPDATE SKIP LOCKED 或条件更新 `WHERE status='available'` 并校验 rows_affected；SQLite 依赖单写连接事务。不足量时整体失败。
- **必测用例**: 可用 5 张，导出 limit=6 → insufficient 且 5 张仍 available；两个并发导出各 limit=3（共 5 张）→ 一个成功一个 insufficient，无重复；导出后状态为 used 或已删除；非自动发货商品 → invalid。

#### DLV-02 交付使用说明只在付款后可见，并做 HTML 净化

- 提交: 563f6dca/fd386d1e/eea1a501 2026-04-17
- 严重度: **高**
- 问题现象: 新增商品"交付使用说明"（可能包含账号使用方法等机密信息）。不能在未付款订单详情、订单列表、商品公开接口中泄露；渲染富文本存在 XSS 风险；邮件或 Telegram 无法渲染 HTML。
- 根因: 新功能。
- 原修复方式: 下单时把 `product.InstructionsJSON` 快照到 `order_items.instructions_json`。`NewOrderDetail`：`PaidAt==nil` 时置 nil；`NewOrderSummary` 永远置 nil（测试 `TestOrderDetailHidesInstructionsBeforePayment`）。渠道 API 同样只在已付款时返回按语言解析后的字符串，按文本去重拼接。邮件只在 delivered/completed 状态时追加，用 `htmltext.StripToPlainText` 转纯文本（块级标签转换行，剥离标签，解码实体，折叠空行）。前端在 `fulfillment.status==='delivered'` 时才展示，用 DOMPurify 白名单（p/br/strong/em/u/s/code/pre/blockquote/ul/ol/li/a/h1-6/span/div/img/hr；属性只允许 href/target/rel/src/alt/title；禁用 style/class/id；URI 限定 `https?:|mailto:|tel:|#|/` 开头的相对路径）。
- **对我们实现的要求**: Rust DTO 层按 paid_at 裁剪 instructions，公开商品 DTO 不输出该字段；后端存储时也建议用 ammonia 做一次白名单净化；前端 v-html 必须经 DOMPurify。
- **必测用例**: 未付款订单详情中 instructions=null；已付款订单返回说明；订单列表永远不返回；游客订单查询同样遵守；说明中含 `<img onerror>`、`<a href="javascript:">`，渲染结果中不保留这些内容；邮件正文为纯文本。

#### DLV-03 下单预占卡密：加行锁 + 条件更新并校验影响行数，建复合索引

- 提交: 4a4abb1c 2026-03-31
- 严重度: **高**（高（并发下同一张卡密可能被两个订单预占，即重复发卡））
- 问题现象: 创建订单时先 `SELECT ... WHERE product_id=? AND sku_id=? AND status='available' ORDER BY id LIMIT n`，再 `Reserve(ids)`。两个并发订单会选中同一批 id。Reserve 本身是 `UPDATE ... WHERE id IN ? AND status='available'`，并且检查 `affected != len(ids)` 时报 `ErrCardSecretInsufficient`，所以不会重复占用，但后到的订单会直接失败（假性库存不足）。修复在 SELECT 上加了 `FOR UPDATE`，同时把 (product_id, sku_id, status) 建成复合索引 `idx_card_secret_reserve`，payment_channels.is_active 也加了索引。
- 根因: 先读后写存在竞态。
- 原修复方式: 如上：行锁 + 条件更新 + 影响行数校验。
- **对我们实现的要求**: 预占必须满足：①`UPDATE card_secrets SET status='reserved', order_id=? WHERE id IN (...) AND status='available'`，影响行数不等于期望值就回滚；②MySQL/PG 使用 `SELECT ... FOR UPDATE SKIP LOCKED`（sea-orm 的 `lock_with_behavior`），减少冲突；SQLite 没有行锁，依靠单写者和条件更新保证。可以在冲突时重试一次。建立 (product_id, sku_id, status) 复合索引。
- **必测用例**: ①库存 1 张，两个并发下单：一个成功，一个返回库存不足，卡密只关联一个订单；②库存 10 张，10 个并发下单各买 1 张（PG/MySQL 下使用 SKIP LOCKED），全部成功且卡密互不重复；③Reserve 影响行数不足时整个订单回滚，manual 库存也不扣减。

#### DLV-04 自动发货多 SKU 商品：卡密必须指定 SKU；禁用仍有卡密库存的 SKU 被拒绝

- 提交: f3b5b75b 2026-03-24, a9f94240 2026-03-24
- 严重度: **高**
- 问题现象: (1) 自动发货商品有多个启用 SKU 时，批量导入卡密不选 SKU，`resolveCardSecretSKU` 回退到 DEFAULT SKU 或"唯一 SKU"逻辑，卡密挂到错误/不可售 SKU 上 → 买家买 A 规格发出 B 规格卡密或显示缺货；(2) 管理员编辑商品把某个仍有可用卡密的 SKU 禁用/删除，卡密变成孤儿库存，仪表盘告警失真。
- 根因: SKU 解析未区分启用状态；SKU 更新没有库存保护。
- 原修复方式: `resolveCardSecretSKU`：显式 sku_id 必须属于该商品，auto 商品还必须是启用 SKU；未指定时 auto 商品：0 个启用 SKU→走旧逻辑，1 个→用它，≥2 个→`ErrProductSKURequired`。`applyProductSKUsWithStockGuard` + `ensureAutoSKUCardSecretStockSafe`：auto 商品中原本启用、更新后变为禁用或被移除的 SKU，若 `available>0 || total-used>0`（含 reserved）→ `ErrProductSKUHasCardSecretStock`（400 `error.product_sku_has_card_secret_stock`）；Update 中先做 SKU 校验再更新商品（同一事务）。前端导入页在 SKU>1 时强制选择。仪表盘：仅禁用 SKU 有库存时回退为商品级告警。
- **对我们实现的要求**: 卡密导入/批量创建 API：auto 商品多启用 SKU 时 sku_id 必填；更新商品 SKU 时对"将被禁用/移除"的 SKU 查询卡密 available+reserved，>0 则拒绝整个更新（事务回滚）。
- **必测用例**: auto 商品 SKU A/B 均启用，导入不传 sku_id → 400 sku_required；仅 A 启用 → 自动落到 A；传入禁用 SKU id → 400；A 有 3 张 available 卡密时更新把 A is_active=false → 400 且商品其它字段也未改；A 仅有 used 卡密 → 允许禁用。

#### DLV-05 超长交付内容：接口截断 + 单独下载；下载必须校验订单归属（含子订单）

- 提交: dc2134ca 2026-03-29, 782da242 2026-03-29, 89fed21f 2026-03-29, e4b02e63 2026-03-31, f26e7c80 2026-03-29, bd7fd713 2026-03-29
- 严重度: **中**（中（前端崩溃；下载接口存在越权风险））
- 问题现象: 一个订单交付上万行卡密时，订单详情 API 返回整个 payload，前端渲染把浏览器卡死或崩溃。新增的下载接口最初是 `GET /user/orders/:id/fulfillment/download`，支持父订单或子订单 id：先按"父订单 + user"查询，查不到就 `OrderRepo.GetByID` 并比较 `raw.UserID==uid`。游客版把 email 和 order_password 放在 query string 中，并比较 `raw.GuestEmail==email`（大小写敏感，且没有校验 user_id=0）。
- 根因: 大文本没有分页或截断；下载路径的鉴权分散在各处。
- 原修复方式: `Fulfillment.TruncatePayload(100)` 截断到 100 行，并返回 `payload_line_count`；用户、游客和管理员详情都做截断。采购单的 `UpstreamPayload` 同样截断，并提供 `/procurement-orders/:id/upstream-payload/download`。下载接口返回 `Content-Disposition: attachment; filename="fulfillment-{order_no}.txt"`：父订单拼接各子订单的 payload（用 `\n` 连接）。e4b02e63 改为 `/orders/:order_no/fulfillment/download`，用 `GetAnyByOrderNoAndUser`（`order_no=? AND user_id=?`，不区分父子订单）和 `GetAnyByOrderNoAndGuest`（`user_id=0 AND guest_email=? AND guest_password=?`）。前端显示前 N 行，并提供"下载完整内容"按钮。
- **对我们实现的要求**: 详情接口的交付内容截断为 100 行，并返回总行数。下载接口按 order_no 查询，在 SQL 条件里同时限定归属（用户：user_id；游客：user_id=0 + 归一化邮箱 + 订单密码）。游客凭据最好不放在 URL query 中（改用 POST 或短期签名下载 token），避免进入访问日志。文件名只使用 order_no（校验字符集），防止 header 注入。
- **必测用例**: ①3000 行 payload 的详情只返回 100 行，payload_line_count=3000；②下载得到完整 3000 行；③用户 B 用用户 A 的子订单号下载返回 404；④游客邮箱大小写不同也能下载，错误密码返回 404；⑤父订单下载内容为各子订单 payload 的拼接。

#### DLV-06 批量导入卡密数量过多时失败

- 提交: 12ff0123 2026-03-28
- 严重度: **中**
- 问题现象: 一次导入数千条卡密时 `db.Create(&items)` 生成单条超大 INSERT，超过 SQLite 变量上限（999 或 32766）、MySQL 的 max_allowed_packet 或 PG 的 65535 参数上限，整批导入失败。
- 根因: 批量插入没有分块。
- 原修复方式: `CreateInBatches(&items, 200)`。
- **对我们实现的要求**: sea-orm 的 `insert_many` 手动按 200 行（或按参数数 / 列数计算）分块，所有分块放在同一个事务里（全部成功或全部回滚），导入前在内存中去重，并返回成功数和重复数。
- **必测用例**: ①一次导入 10000 条，在 SQLite、MySQL、PG 上都成功，count=10000；②第 5000 条违反约束时整批回滚（或按约定部分成功，需要明确并测试）。

#### DLV-07 卡密批量操作的目标解析、批次实时计数与搜索

- 提交: 268d793e 2026-03-22, e1503f95 2026-03-22, 8c354677 2026-03-24, 6a098027 2026-03-24, ecfa2e6d 2026-03-24
- 严重度: **中**
- 问题现象: (1) 批量改状态/删除/导出只能传 ids，无法按批次或筛选结果操作；(2) 批次列表显示的是导入时写死的 total_count，卡密被删除/使用后数字不变；(3) 卡密/批次号搜索在 PostgreSQL 下大小写敏感（LIKE），SQLite 不敏感，行为不一致；(4) 后台商品搜索输入数字 ID 搜不到。
- 根因: 接口契约过窄；统计未实时聚合；LIKE 大小写行为依赖数据库。
- 原修复方式: `resolveBatchTargetCardSecretIDs(ids, batchID, filter)`：优先 ids → 其次任一筛选条件（product/sku/batch/status/secret/batch_no）`ListIDs` → 其次 batch_id；**三者都为空返回 ErrCardSecretInvalid**；匹配为空返回 404。批次列表 `CountByBatchIDs` 按 `batch_id,status` GROUP BY 实时算 available/reserved/used/total。搜索 `LOWER(col) LIKE LOWER(?)`，批次号需 LEFT JOIN card_secret_batches。List 中 sku_id>0 但 product_id=0 报错。分页上限 100→200。商品搜索：数字时 `(localized LIKE ...) OR id = ?`，用分组条件包裹避免 OR 破坏其它过滤。
- **对我们实现的要求**: 批量危险操作（删除/改状态）绝不允许"空条件=全部"；搜索统一 `LOWER() LIKE LOWER()` 并转义 `%`/`_`；批次统计实时聚合；OR 条件必须用括号分组（sea-orm `Condition::any()` 嵌在 `Condition::all()` 内）。
- **必测用例**: 批量删除 body `{}` → 400 且无行被删；`{batch_id: 7}` → 仅删批次 7；筛选 status=available&batch_no=abc（实际 ABC）→ 命中；批次导入 10 张、删 2 张、用 3 张 → total=8, used=3, available=5；商品搜索 "12" 同时匹配 id=12 且 is_active 过滤仍生效。

#### DLV-08 自动发货商品库存按 SKU 统计卡密（含 sku_id=0 通用卡密），多 SKU 不重复/虚高

- 提交: 94971eff 2026-02-28, 6fbc1607 2026-02-28; 3ea0c239 2026-02-25, 89cb8c7d 2026-02-25, 9d58d2ca 2026-02-25
- 严重度: **中**
- 合并条目: (1) 多 SKU 商品自动发货库存重复/虚高 ｜ (2) 自动发货商品库存需按 SKU 统计卡密（含 sku_id=0 通用卡密），公开接口只返回启用 SKU
- 问题现象:
  - (1) `ApplyAutoStockCounts` 把 `sku_id=0` 的历史卡密数量加到每个 SKU 上，3 个 SKU 时库存显示×3，前台可下单数量虚高；`decorateProductStock` 汇总时把停用 SKU 的库存也算进商品；此外 GORM 按字段名 `SKUID` 推导列名，聚合扫描结果列 `sku_id` 无法映射。
  - (2) 自动发货商品只按商品维度统计可用卡密，多 SKU 时前端无法得知每个 SKU 的库存，可超量加购；前端仅对 manual 类型做库存限制，auto 类型无上限；公开接口预加载了未启用 SKU。
- 根因:
  - (1) 旧数据兼容逻辑放大；未过滤 is_active；聚合结构体列名未显式指定。
  - (2) 库存统计粒度不够；前端 `shouldEnforceSkuStock` 仅 manual。
- 原修复方式:
  - (1) `resolveLegacyStockTargetSKUIndex`：sku_id=0 的卡密只归并到 DEFAULT 编码的活跃 SKU，其次首个活跃 SKU，再其次 DEFAULT，最后 index0；商品级汇总 `if !sku.IsActive continue`；`SKUStockCount` 字段加 `gorm:"column:sku_id"` 等显式列名。测试 `TestApplyAutoStockCounts_LegacyStockPrefersDefaultSKU`、`TestDecorateProductStock_AutoSkipsInactiveSKUs`。
  - (2) `CardSecretRepository.CountStockByProductIDs`：`SELECT product_id, sku_id, status, COUNT(*) ... WHERE product_id IN ? GROUP BY product_id, sku_id, status`；`ProductService.ApplyAutoStockCounts(products []Product)` 填充商品及每个 SKU 的 `auto_stock_available/total/locked/sold`（available=available 状态，locked=reserved，sold=used，total=available+locked），SKU 数值 = 本 SKU + sku_id=0 的卡密（旧数据兼容）；公开查询 `Preload("SKUs", is_active=true ORDER BY sort_order DESC, id ASC)`，后台预加载全部 SKU。前端 auto 类型用 `sku.auto_stock_available` 作为最大购买数量，购物车数量改为可输入框并夹紧到 [1, max]。
- **对我们实现的要求**:
  - (1) 卡密库存 group by (product_id, sku_id, status) 后按上述规则只归并一次；商品级汇总只统计 is_active SKU；sea-orm 自定义聚合 select 必须 `.column_as(..., "sku_id")` 与 FromQueryResult 字段一一对应。
  - (2) 商品列表/详情（公开和后台）返回 per-SKU 自动库存字段；按一次 GROUP BY 聚合避免 N+1；sku_id=0 的卡密计入每个 SKU。公开接口仅返回启用 SKU。前端购物车/详情/结算页对 auto 类型按 SKU 可用卡密数限制数量。
- **必测用例**:
  - (1) 商品 3 个 SKU（A,B 活跃，DEFAULT 活跃），sku_id=0 卡密 5 张 → 只有 DEFAULT 得 +5；停用 SKU 有 10 张卡密 → 商品汇总不含这 10。
  - (2) 商品 auto，SKU1 有 3 张 available + 1 reserved，sku_id=0 有 2 available → SKU1 auto_stock_available=5，auto_stock_total=6，auto_stock_locked=1；前端数量输入 10 → 自动夹到 5；未启用 SKU 不出现在公开详情。

#### DLV-09 自动发货成功后订单直接为 completed，并发送含卡密的完成邮件

- 提交: 411cb29a 2026-02-23
- 严重度: **中**
- 问题现象: 自动发货后订单停在 `delivered`，需要用户手动确认；状态邮件模板只在 `delivered` 时附交付内容，改为 completed 后卡密内容不出现在邮件中。
- 根因: 自动发货状态流转设计 + 邮件模板状态分支不全。
- 原修复方式: `fulfillment_service.go::CreateAuto` 将订单状态、父订单聚合目标状态和邮件状态改为 `OrderStatusCompleted`；`email_service.go::buildOrderStatusContent` 对 `Delivered, Completed` 都渲染"交付内容"。
- **对我们实现的要求**: 自动发货事务内：分配卡密→写 fulfillment→订单状态 completed（父订单按子订单汇总）；邮件模板 delivered 和 completed 都附 payload。
- **必测用例**: 自动发货后订单状态 == completed；completed 状态邮件（zh）主题含"已完成"，正文含"交付内容"和卡密 "AUTO-CODE-001"。

#### DLV-10 卡密导入去重开关（默认去重）

- 提交: b8e91e49/6ea35df2 2026-05-04
- 严重度: **低**
- 问题现象: 循环卡密场景（同一卡密需要多次售出）下，导入时被强制去重，无法录入重复的卡密。
- 根因: `normalizeSecrets` 总是去重；CSV 解析时又去重了一次。
- 原修复方式: 请求增加 `deduplicate *bool`，nil 时默认 true；CSV 表单 `deduplicate` 按 ParseBool 解析，非法值返回 400；CSV 解析阶段不再去重，统一由 `normalizeSecrets(values, dedup)` 处理（按行拆分并 trim、跳过空行）。
- **对我们实现的要求**: Rust 批量导入保持"缺省去重"语义，只做批内去重。
- **必测用例**: deduplicate 缺省时 "a\na\nb" 导入 2 条；deduplicate=false 时导入 3 条；deduplicate=abc 返回 400。

## 6. 退款（RFD，4 条）


#### RFD-01 退款手续费按比例冲回（累计法吸收舍入误差）+ 支持历史退款补录

- 提交: c6e59679 2026-08-11, f9409e10 2026-08-12
- 严重度: **高**
- 问题现象: 商家承担手续费（merchant_absorbed）的订单部分/全额手动退款后，渠道退回的手续费无法记账，利润统计中手续费成本无法冲回；多次部分退款按单次比例四舍五入会累计超出/不足原手续费。另外更新手续费返还后接口只返回裸 record，详情字段（order_no、refund_type_label、user_email、items）丢失。
- 根因: 缺少手续费返还字段与分摊算法。
- 原修复方式: order_refund_records 新增 `payment_fee_refunded bool`、`payment_fee_refunded_amount decimal(20,2)`。domain `CalculatePaymentFeeRefundAmount(paymentAmount, paymentFee, refundedPrincipalBefore, refundedFeeBefore, refundAmount)`：全部 Round(2)，累计本金 = min(before+本次, paymentAmount)，目标累计手续费 = fee*累计本金/paymentAmount round2（累计本金≥支付额时直接=fee），本次 = 目标-已退手续费（负数归零，且不超过剩余手续费）。可退手续费快照 `refundablePaymentFeeSnapshot` 只统计根订单（子订单取 ParentID）下 status=success、非 wallet、fee_policy=merchant_absorbed、exception_code 为空、币种与订单一致的支付。`UpdatePaymentFeeRefunded` 只允许 manual 类型退款记录，事务内先锁订单再锁退款记录（`GetRefundRecordByIDForUpdate`），重算时排除当前记录。handler 更新后重新查询 `GetAdminRefundItem` 返回完整视图。
- **对我们实现的要求**: 退款记录保存手续费返还标记与金额；按累计法分摊；只针对 manual 退款记录允许事后切换；锁顺序固定为 订单 → 退款记录；更新接口返回完整详情 DTO。
- **必测用例**: 支付 100.00 手续费 3.01，第一次退 33.33 → 手续费 1.00，第二次退 66.67 → 2.01（合计 3.01）；支付 80 手续费 2.40，已退本金 60/手续费 1.80，再退 50 → 0.60（封顶）；退款 40（支付 100 手续费 3.00）→ 1.20，关闭再开启仍为 1.20；wallet 类型退款记录切换 → status_invalid。

#### RFD-02 部分退款/全额退款的金额守卫、状态流转、父子订单同步

- 提交: 8a13e995/922cb6fd/18e10d7c 2026-04-13, f77f47d4 2026-04-13
- 严重度: **高**
- 问题现象: 退款原先只有"退到钱包"，也不改订单状态。新增手动退款（仅记账）和部分退款后，需要处理：累计退款不能超过实付；父子订单的状态要一致；返利要按退款冲回；利润统计要扣除退款。
- 根因: 缺少退款记录表和状态机。
- 原修复方式: 新表 `order_refund_records`（user_id/guest_email/order_id/type manual|wallet/amount/currency/remark）。`AdminManualRefund` 和 `WalletService.AdminRefundToWallet` 在事务内：`SELECT ... FOR UPDATE` 锁订单 → 要求 `PaidAt != nil` 且 `TotalAmount > 0` → `amount = round2(amount) > 0` → `refundable = total - refunded_before`，`amount > refundable` 时报 `ErrWalletRefundExceeded` → `new_refunded >= total` 时状态置为 `refunded`，否则 `partially_refunded` → 如果是父订单，用 `applyParentRefundChildStatusUpdatesTx` 把所有子订单状态统一成父订单的目标状态；如果是子订单，执行 `syncParentStatus`（`calcParentStatus`：子订单全部 refunded 时父为 refunded，有任一 refunded/partially_refunded 时父为 partially_refunded）→ 执行 `affiliateSvc.HandleOrderRefundedTx(tx, order, amount, refundedBefore)` 冲回佣金 → 写退款记录。整个过程在同一个事务内完成。后台手动把状态改成 partially_refunded/refunded 时（`order_service_child.go`），子订单同步改状态，并逐个检查 `isTransitionAllowed`。利润看板：`profitOrderStatuses` 包含 refunded，退款按 **退款记录的 created_at** 所在窗口扣减（订单在窗口外、退款在窗口内也要扣；某天只有退款没有订单也要出现一行）。
- **对我们实现的要求**: Rust 退款服务在一个 DB 事务里完成"锁订单行 → 校验 → 更新 refunded_amount 和状态 → 父子同步 → 冲回返利 → 写退款记录 → 钱包入账（退款到钱包时）"。金额全程用 Decimal，保留 2 位小数。
- **必测用例**: 1) 实付 100，先退 30（状态为 partially_refunded），再退 80 被拒绝（超额），再退 70 后状态为 refunded；2) 负数或 0 金额被拒绝；3) 未支付订单退款被拒绝；4) 对父订单部分退款，子订单无论原状态（有的已完成、有的在发货中）都变成 partially_refunded；5) 两个子订单一个 refunded、一个 completed，父订单为 partially_refunded；6) 利润趋势：窗口外的订单在窗口内退款，要从窗口内那天的收入里扣除。

#### RFD-03 管理员退款到余额必须要求订单已支付(paid_at 非空)

- 提交: 7af926fc 2026-02-24, bb19a243 2026-02-24
- 严重度: **高**
- 问题现象: `AdminRefundToWallet` 只校验 `TotalAmount > 0` 与 `amount <= total - refunded`，对未支付/已取消订单也能"退款"到用户余额，凭空造钱。
- 根因: 缺少支付状态守卫。
- 原修复方式: `wallet_service.go::AdminRefundToWallet` 在 `SELECT ... FOR UPDATE` 锁订单后加 `if order.PaidAt == nil → ErrOrderStatusInvalid`；handler 映射为 400 `error.order_status_invalid`；后台 `Orders.vue` 对无 `paid_at` 的订单可退款金额为 0、隐藏退款按钮。
- **对我们实现的要求**: 退款到余额流程（单事务）：锁订单行 → user_id!=0（游客不支持）→ paid_at 非空 → total>0 → amount>0 且 amount<=total-refunded（round 2）→ 锁/建钱包账户 → 加余额 → 更新 refunded_amount → 写流水（reference `order:<id>:admin_refund`）。前端对未付订单不显示退款。
- **必测用例**: 订单 40 元，状态 canceled、paid_at 为空，退款 15 → ErrOrderStatusInvalid，余额与 refunded_amount 不变；已付订单 40 退 15 成功后再退 30 → ErrWalletRefundExceeded。

#### RFD-04 退款时效窗口 max_refund_days

- 提交: 060efdba 2026-04-13, 115e9657/6b7497a0 2026-04-15
- 严重度: **中**
- 问题现象: 没有退款时效限制，很久以前的订单也能被退款。
- 根因: 缺少配置项。
- 原修复方式: 新增设置 `order_config{payment_expire_minutes(1..10080,默认15), max_refund_days(0..3650,默认30,0=不限)}`。`isOrderRefundWindowExpired`：以 `paid_at` 为基准（没有则用 created_at），`now > base + days` 时返回 `ErrOrderRefundExpired`（HTTP 400 `error.order_refund_expired`）。手动退款和退到钱包两条路径都要检查。settings 表没有配置时回落到 config.yaml 的 `order` 段（6b7497a0，config.yaml 可以把 max_refund_days 配成 0）。参数越界时归一化到默认值或上限。
- **对我们实现的要求**: 两条退款路径都校验时效窗口，days=0 表示不限。配置优先级：DB setting > config.yaml > 内置默认。
- **必测用例**: days=30 时，paid_at 为 31 天前的订单退款返回 expired，29 天前的可以退；days=0 时，1 年前的订单也能退；未配置 DB setting、config.yaml 配 0 时视为不限。

## 7. 钱包/充值/礼品卡（WAL，3 条）


#### WAL-01 充值回调终态矩阵与超时过期任务

- 提交: 92b49a2c 2026-02-26, 5af2d715 2026-02-27
- 严重度: **高**
- 问题现象: ①充值单没有超时机制，pending 永久挂着；②迟到的 pending/failed 回调会把已 expired/failed/success 的充值"重开"或改回；③重复 success 回调可能重复入账；④超时任务入队失败时充值单悬空。
- 根因: 回调状态迁移没有终态保护；缺少过期任务。
- 原修复方式: `canApplyWalletRechargeCallback(paymentStatus, rechargeStatus, target)`：target=success 永远允许（网关延迟成功通知可覆盖 expired/failed），非 success 回调在 payment 或 recharge 处于 success/failed/expired 任一终态时一律忽略，仅更新回调元数据。`ExpireWalletRechargePayment(paymentID)`：事务内 `SELECT ... FOR UPDATE` 锁 payment 与 recharge，仅 `payment.OrderID==0` 且 recharge=pending、payment∈{initiated,pending} 时置 expired（幂等）。创建充值支付后按 `resolveExpireMinutes()`（设置项 → 配置 → 默认 15）入队 `wallet_recharge:timeout_expire`；入队失败则把 payment/recharge 标 failed（若 recharge 已 success 不动）并返回 ErrQueueUnavailable。测试：SuccessAfterExpireCreditsOnce、SuccessThenExpireKeepsSuccess、DuplicateSuccessDoesNotDuplicateCredit、PendingAfterExpireDoesNotReopen、TerminalStateMatrixDoesNotReopen、SuccessAfterFailedCreditsOnce、ExpireDoesNotOverrideSuccess、ExpireSkipsOrderPayment。
- **对我们实现的要求**: 充值回调与过期都在一个事务里 `lock_exclusive()` 锁定 payment+recharge 行后按矩阵判定；入账必须以"recharge 由非 success → success 的那一次迁移"为唯一触发点（UPDATE ... WHERE status<>'success' 看 rows_affected）；过期任务只处理充值单（order_id=0/NULL）。
- **必测用例**: expired 后收 success → 入账 1 次、状态 success；success 后收 success ×2 → 余额只加一次；expired 后收 pending/failed → 状态不变；success 后跑过期任务 → 仍 success；订单支付单 ID 传给过期任务 → 不处理；队列不可用 → 创建接口报错且 recharge=failed。

#### WAL-02 管理员调账必须显式指定操作、填写备注并记录操作人

- 提交: 4e4d5bbc 2026-07-27
- 严重度: **中**
- 问题现象: `AdjustUserWallet` 未传 operation 时默认 "add"，误操作/恶意请求会给用户加钱；备注可空，流水无操作人，事后无法审计。
- 根因: 默认值不安全；缺审计字段。
- 原修复方式: operation 为空 → bad_request；只接受 add/subtract；remark trim 后为空 → `error.wallet_adjust_remark_required`；wallet_transactions 新增 `operator_admin_id`（从 JWT 取 adminID）。
- **对我们实现的要求**: 调账接口 operation 必填且枚举校验、remark 必填、流水记录 operator_admin_id。
- **必测用例**: 不带 operation → 400；remark 为空 → 400；成功调账后流水 operator_admin_id=当前管理员。

#### WAL-03 钱包充值"检查支付状态"：渠道不支持主动 capture 时回退为查询当前状态

- 提交: 42f89df6 2026-02-22
- 严重度: **低**
- 问题现象: 用户在充值页点击"检查支付状态"，对不支持 capture 的渠道（易支付、支付宝等）接口返回错误，而不是当前状态。
- 根因: `CaptureMyWalletRechargePayment` 把 `ErrPaymentProviderNotSupported` 当作错误直接返回。
- 原修复方式: `public/wallet.go`：若错误是 `ErrPaymentProviderNotSupported`，改为 `PaymentService.GetPayment(paymentID)` 返回当前 payment。
- **对我们实现的要求**: capture/检查状态接口在渠道不支持主动查询时返回当前 payment 状态（200），其他错误照常映射。仍需校验 payment 属于当前用户。
- **必测用例**: 易支付渠道充值单调用 capture → 200，返回 status=pending；PayPal 渠道 → 实际执行 capture。

## 8. 优惠券/活动价/批发价/会员价 定价（PRC，15 条）


#### PRC-01 并发下单绕过优惠券总次数/单用户次数限制

- 提交: 0407f2ac 2026-08-16
- 严重度: **高**
- 问题现象: 优惠券 UsageLimit=1（或 PerUserLimit=1）时，多个并发下单请求在事务前的资格校验中都读到 used_count=0，全部通过并各自写入 coupon_usage，券被用多次。
- 根因: 额度校验只在事务外进行，未对优惠券行加锁复核。
- 原修复方式: coupon gormstore 新增 `GetByIDForUpdate`（`SELECT ... FOR UPDATE WHERE deleted_at IS NULL`）；order_service.go `createOrder` 事务内写 usage 前锁券，复核 `UsageLimit>0 && UsedCount>=UsageLimit → ErrUsageLimit`，`PerUserLimit>0 && UserID!=0` 时 `usageRepo.CountByUser(couponID,userID) >= PerUserLimit → ErrPerUserLimit`；券不存在 → ErrNotFound；这三个错误原样返回给调用方（不包装成通用下单失败）。
- **对我们实现的要求**: 下单事务内锁定优惠券行后复核总次数与单用户次数，再 `used_count = used_count + 1`（最好用条件更新 `WHERE used_count < usage_limit` 并检查 rows_affected 作为双保险，兼容 SQLite 无 FOR UPDATE）；取消/超时回滚时对称减少。
- **必测用例**: UsageLimit=1，20 个并发请求下单 → 恰好 1 单成功，其余返回 usage_limit 错误，used_count=1；PerUserLimit=1 同一用户并发 10 单 → 1 单成功。

#### PRC-02 批发价（商品级→SKU 级）：门槛数量口径、匹配优先级、写入校验、前后端一致

- 提交: 09c15402 2026-07-03, 0a63d7cc 2026-07-03, c10fc93e 2026-07-03, 24a1d0e8 2026-07-06; 5861cc34 2026-06-01, 956dd5e0 2026-06-01, 9d6a4411 2026-06-03, 0e074f71 2026-05-28
- 严重度: **高**（高（直接影响成交价）；高）
- 合并条目: (1) SKU 级批发价：匹配优先级、门槛数量口径、写入校验 ｜ (2) 批发价门槛按“同商品全部 SKU 总数量”判定，前后端需一致
- 问题现象:
  - (1) 批发价原为商品级扁平阶梯，多 SKU 价格差异大时无法分别设置；引入 SKU 级阶梯后出现：SKU 专属阶梯与通用阶梯混算、tier 的 sku_id 与 sku_code 不一致仍匹配、前端展示/购物车预估与后端不一致、创建商品时 SKU 尚未落库导致 sku_id 无法校验。
  - (2) 同一商品两个 SKU 各买 6 件（门槛 10），后端按单行数量 6 判定不命中，前端购物车/结算显示却按另一逻辑，出现前台显示价与实付不一致；此外底价 50 的 SKU 若被套用商品级批发价 80 会被“拉高”。
- 根因:
  - (1) 阶梯缺少 scope 概念及一致性校验；前后端各自实现匹配逻辑。
  - (2) 档位判定数量口径不一致；批发档是商品级扁平单价。
- 原修复方式:
  - (1) `WholesalePriceTier` 增加 `sku_id`/`sku_code`（均空=全 SKU 通用）。`resolveWholesaleUnitPrice(product, base, skuID, skuCode, matchQty, lineQty)`：tier 优先级——tier.sku_id>0 时必须 == 当前 sku_id 且（若 tier 有 code）code 相同（大小写不敏感）→3；只有 code 且匹配 →2；通用 →1；不匹配 →0。若该 SKU 存在专属 tier（优先级≥2）则**只看专属 tier，不回退通用 tier**；否则只看通用 tier。通用 tier 用“同商品所有 SKU 总数量”判门槛，专属 tier 用“本行数量”判门槛。在满足门槛的 tier 中取**单价最低**的，且仅当低于基准价才生效；优惠 = (base - tier) * lineQty。写入：`normalizeWholesalePriceInputsForSKUs` 要求 sku_id 必须属于该商品、sku_id 与 sku_code 同时给出时必须一致，只给 code 时反查补全 id；scope key 以 code（小写）优先、否则 id、否则 all；同 scope 内 min_quantity 不可重复，单价随门槛**严格递减**；min_quantity<=0 或单价<=0 拒绝。商品 Create/Update 改为先写 SKU 再按 SKU 列表校验批发价；Update 仅在请求显式携带 wholesale_prices 时覆盖（nil 保留原值）。分销订单（resellerOrder）不应用批发价。前端 `getWholesalePrices(product, skuId, skuCode)` / `resolveWholesalePriceAmount(..., matchQty, skuId, skuCode, lineQty)` 与后端同规则，详情页只显示所选 SKU 适用的阶梯。
  - (2) buildOrderResult 先按 ProductID 汇总 productQuantityTotals，ResolveWholesaleUnitPriceWithMatchQuantity(product, base, matchQty=商品总量, qty=本行数量)；只有档位价 < 该 SKU 底价才生效，各行只算本行优惠。前端 Checkout.vue 增 cartProductQuantities（按 productId 汇总）并复用 useProductLabels.resolveWholesalePriceAmount；ProductDetail/ProductQuickBuy 同步计算；新增 resolveMemberPriceAmount（SKU 级会员价优先于商品级，否则按 discount_rate%）。
- **对我们实现的要求**:
  - (1) Rust 端实现单一的批发价解析函数（前端 TS 版逐条镜像）；写入侧强校验 SKU 归属与一致性、scope 内严格递减；商品保存流程先持久化 SKU 再校验批发价；PATCH 语义区分“字段缺省”和“空数组”。
  - (2) 后端与 Vue 端共用同一规则（最好前端只展示后端 preview 结果）：档位判定用同商品跨 SKU 总量；单价仅在 tier.unit_price < sku.base 时替换；行级 discount=(base-tier)×行数量。
- **必测用例**:
  - (1) SKU A 专属 5 件 70、SKU B 专属 5 件 60，各买 5（基价 100）→ 原价 1000、批发优惠 350、合计 650，A 单价 70/优惠 150，B 单价 60/优惠 200；通用 10 件 80 + A 专属 10 件 70，A、B 各买 6 → A 不回退通用（单价 100、优惠 0），B 用通用（总数 12≥10，单价 80、优惠 120），合计 1080；tier sku_id=A 但 sku_code=B 的写入 → ErrWholesalePriceInvalid；sku_id 不属于该商品 → 拒绝；同 scope 门槛 5→90、10→95 → 拒绝；Update 请求不带 wholesale_prices → 原阶梯保留。
  - (2) 档 {min10:80}，SKU-A 底价100×6 + SKU-B 底价50×6 → original=900，wholesale_discount=120，total=780，A 行 unit=80/discount=120，B 行 unit=50/discount=0；单 SKU 数量 9 不命中。
- ⚠ 演进说明：2026-06 版（5861cc34）为商品级阶梯、门槛按同商品全部 SKU 总数量判断；2026-07 起引入 SKU 级阶梯：**通用阶梯用同商品全 SKU 总量判门槛、SKU 专属阶梯用本行数量判门槛，存在专属阶梯时不回退通用阶梯**。以最新规则为准。

#### PRC-03 优惠券“禁止批发价商品使用”与“固定金额券按件抵扣”

- 提交: d10fd072 2026-07-06, f402d958 2026-07-06, c8ae0687 2026-07-06, b68faa6a 2026-07-06, ebfbebe0 2026-07-06
- 严重度: **高**（高（金额计算））
- 问题现象: 运营需要优惠券不与批发价叠加；固定金额券需要按商品件数抵扣。
- 根因: 新增业务规则（需求型，但定义了必须遵守的计算口径）。
- 原修复方式: coupon 新增 `disabled_wholesale_price`（bool，默认 false）、`per_item_discount`（bool，默认 false，仅 fixed 类型有效，Create/Update 时 type≠fixed 强制 false）。资格计算 `resolveCouponEligibility`：在 scope 内的商品行中，若券禁用批发价且该行 `wholesale_discount>0` 则排除；若 scope 命中行全部因批发被排除 → `ErrCouponWholesaleDisabled`（独立错误码与 i18n），否则无命中 → `ErrCouponScopeInvalid`。门槛 min_amount 以合格小计判断。fixed+per_item：优惠 = value × 合格行数量之和；再受 max_discount 上限、再不超过合格小计。订单侧 `applyCouponDiscountToItems` 分摊时同样排除批发行（批发行 coupon_discount=0）。
- **对我们实现的要求**: 优惠券计算与分摊用同一套“合格行”集合；错误码区分“因批发价不可用”和“不在适用范围”；per_item 只对 fixed 生效。
- **必测用例**: 单商品买 5 件命中批发 80 + 券禁批发 → ErrCouponWholesaleDisabled；A（批发命中，5×80=400）+ B（1×100）券 10% 禁批发 → 券优惠只来自 B=10，总额 490，A 行 coupon_discount=0；fixed 5 元 per_item，数量 3 → 15；同上 max_discount=10 → 10；per_item + 禁批发，批发行 5 件 + 普通行 2 件 → 10；percent 券 per_item=true → 忽略按件。

#### PRC-04 价格叠加顺序：活动价/批发价取优 → 会员价 → 优惠券；会员累计只在真实状态迁移时原子累加

- 提交: ca069b57 2026-05-08, c00526e2 2026-05-28, d7b84b1c 2026-06-11, f34c1777 2026-06-11; 44de7315 2026-03-16, 7c055863 2026-03-16, 86761c1f 2026-03-16, f84eda30 2026-03-16, 087819b9 2026-03-16, 285b6ab5 2026-03-16, 91ee5cf9 2026-03-17
- 严重度: **高**
- 合并条目: (1) 优惠叠加顺序：活动价/批发价取优 → 会员价 → 优惠券 ｜ (2) 会员价与活动价取低不叠加；等级累计只在真实状态迁移时计入
- 问题现象:
  - (1) 旧逻辑会员价与活动价二选一取低（会员不能在活动价上再享折扣）；无“原价”快照，前端价格明细无法展示。批发价加入后需确定与活动价、会员价、优惠券的关系。
  - (2) 引入会员等级后，需要确定会员价与活动价关系、优惠券基数、升级计数。
- 根因:
  - (1) 叠加规则未定义。
  - (2) 新功能的定价口径。
- 原修复方式:
  - (1) buildOrderResult 每行：1) basePrice → 活动价 promoUnit；2) 批发价按档位得 wholesaleUnit，若 matched 且 < promoUnit 则用批发价并清除 promotion/promotionDiscount，否则保留活动价，二者不叠加；3) 在已命中单价上调用 ResolveMemberPrice 计算会员价，更低则记 memberDiscount=(unit-member)×qty；4) 活动命中但无实际优惠则 promotion=nil；5) 优惠券在上述最终价之后按比例计算。OrderItem 新增 original_unit_price/original_total_price（迁移 migration/order_item_original_price_v1：历史行 =0 的回填为 unit_price/total_price）。预览（PreviewOrder/PreviewGuestOrder）SkipManualFormCheck=true，不因人工表单未填报错。
  - (2) `ResolveMemberPrice(level, product, sku, base)` 优先级：SKU 级覆盖价 > 商品级覆盖价（sku_id=0）> 等级折扣率 `base×rate/100`（rate<=0 或 >=100 视为无折扣）；覆盖价 >= base 时不生效。下单：`unit = min(会员价, 活动价)`，会员价更低则 promotion=nil 并记 `member_discount = (base-member)×qty`，否则走活动价记 promotion_discount；unit<=0 报 ErrProductPriceInvalid；订单快照 member_level_id、member_discount_amount，order_item 记 member_discount。升级：充值成功后 `OnRechargeCompleted`，订单支付后 `OnOrderPaid` 累加 `total_spent` 并 `CheckAndUpgrade`（只升不降）。
- **对我们实现的要求**:
  - (1) 价格计算严格按：原价 → min(活动价, 批发价)（互斥，各自记录 discount）→ 会员价（基于上一步单价）→ 优惠券（基于小计）。所有金额 decimal 两位 Round。存 original_unit_price/original_total_price。预览不校验人工表单必填，下单校验。
  - (2) (1) 会员价与活动价互斥取低，优惠券在该单价基础上计算；(2) total_spent/total_recharged 累加必须只在"pending→paid / recharge→success"真实迁移的那次执行，并用原子 SQL（`UPDATE users SET total_spent = total_spent + ?`）在同一事务里，避免重复回调重复累计（原实现为读改写且在事务外调用，存在重复/并发丢失风险）；(3) 前台/下游 API 展示的会员价与下单计算走同一函数。
- **必测用例**:
  - (1) 原价 100、批发档 {min5:80}、数量5：活动 10%（90）→ 批发胜，wholesale_discount=100，券 10% → coupon=40，total=360，unit=80；活动 30%（70）→ 活动胜，promotion_discount=150，coupon=35，total=315；会员折扣在批发价 80 上再打折；预览不传人工表单不报错、下单报错。
  - (2) base=100、活动价 90、会员 85 → 单价 85，member_discount=15×qty，无 promotion_id；会员 95 → 用活动价 90；SKU 覆盖价 120 → 无会员优惠；同一支付回调重复两次 → total_spent 只加一次。
- ⚠ 演进说明：2026-03（44de7315 等）实现为“会员价与活动价取低、互不叠加”；2026-05（ca069b57/c00526e2）起改为“在活动价/批发价命中后的单价上再应用会员价”，当前 HEAD `order_service_validate.go` 第 3 步即为此逻辑。**以最新叠加顺序为准**；但旧版中“total_spent 只在真实迁移时累加”的要求仍然有效。

#### PRC-05 批发阶梯必须随门槛严格递减；选档取最低单价

- 提交: 28e60dbf 2026-06-03, c00526e2 2026-05-28
- 严重度: **高**
- 问题现象: 配置 {5:80, 10:90} 时买 10 件反而单价 90（买更多更贵，用户可拆单规避）；旧选档规则取“门槛最高者”。
- 根因: 写入侧未校验单调性；读取侧依赖配置正确。
- 原修复方式: normalizeWholesalePriceInputs：min_quantity>0、unit_price>0（Round2）、门槛不重复、按门槛升序后要求 price[i] < price[i-1]（相等也拒绝）→ ErrWholesalePriceInvalid；resolveWholesaleUnitPrice 在满足门槛的档位中取 unit_price 最低者（兼容历史脏数据）。
- **对我们实现的要求**: 同样的写入校验（严格递减）和读取兜底（取最低价）；前端 resolveWholesalePriceAmount 当前按“最高门槛”选档，Vue 重写应改为与后端一致的“最低价”。
- **必测用例**: {5:80,10:90} 与 {5:80,10:80} 保存被拒；历史数据 {5:80,10:90}、base 100、qty10 → unit=80、discount=200；qty4 不命中；tier 价 ≥ base 不生效。

#### PRC-06 会员累计金额与自动升级的并发丢失更新/降级

- 提交: 646f2d72 2026-06-02
- 严重度: **高**
- 问题现象: OnRechargeCompleted/OnOrderPaid 读-改-写 total_recharged/total_spent 再 Save，并发支付回调时累计金额丢失；CheckAndUpgrade 用 Save 写等级，可能覆盖并发流程已设置的更高等级；两个启用等级 sort_order 相同时，升级到“同级”的另一个等级（横跳）。
- 根因: 非原子更新 + 全行 Save + 等级顺序不唯一。
- 原修复方式: IncrementTotalRecharged/IncrementTotalSpent 用 `SET col = col + ?`（金额 Round2 且 >0 才执行）；UpdateMemberLevelIfCurrent: `UPDATE users SET member_level_id=? WHERE id=? AND member_level_id=?`（CAS），RowsAffected=0 时重读重试，最多 3 次；只升不降且要求 target.sort_order > current.sort_order（严格大于）；当前等级不在启用列表时按 ID 取其 sort_order；ListAllActive 排序 `sort_order desc, id asc`；创建/更新启用等级时 sort_order 与其他启用等级重复 → ErrMemberLevelSortOrderUsed。
- **对我们实现的要求**: sea-orm 用 `Expr::col(...).add(amount)` 原子累加；等级更新用带条件的 update_many + rows_affected 判断；启用等级 sort_order 唯一校验。
- **必测用例**: 并发 10 次 OnOrderPaid(10) → total_spent=100；用户已被并发提升到等级 3 时，基于旧读取的升级到等级 2 不生效；两个启用等级同 sort_order 保存被拒；同 sort_order 不会横跳。

#### PRC-07 停用的会员等级仍然享受会员价或折扣

- 提交: 7db7fa83 2026-04-03
- 严重度: **高**（高（金额：停用等级后用户仍按折扣价下单））
- 问题现象: 管理员把 VIP 等级设为 is_active=false，但用户的 member_level_id 仍然指向它。`ResolveMemberPrice` 先查 SKU 或商品级的会员价覆盖（member_level_prices）直接返回优惠价，最后才读取 level 的折扣率，而且没有检查 IsActive。`ResolveMemberPriceForProducts`（列表批量价）完全不检查。结果是停用后下单和展示仍然是会员价。
- 根因: 价格解析只看价格表，忽略了等级的启用状态。
- 原修复方式: 在两个函数入口先 `levelRepo.GetByID(levelID)`；level 不存在或 `!IsActive` 时返回原价、折扣 0（批量版返回 nil）。
- **对我们实现的要求**: 会员价解析的第一步必须校验等级存在且启用；列表展示价、购物车价、下单价、订单预览价使用同一个解析函数。折扣率合法区间为 (0,100)，越界视为无折扣。
- **必测用例**: ①等级 VIP 有 SKU 覆盖价 8.00（原价 10），启用时下单单价 8.00；②停用 VIP 后下单单价 10.00，member_discount=0；③商品列表批量价在停用后不返回 member_price；④折扣率为 0 或 100 时不打折。

#### PRC-08 阶梯活动价匹配与 SKU 级活动价展示

- 提交: 81a00a2a 2026-03-08, 67b51e6c 2026-03-08, 842dac15 2026-03-09, a284daa8 2026-03-09
- 严重度: **高**
- 问题现象: ①商品配置多条活动价（如满 100 打 9 折、满 300 打 8 折）时 `GetActiveByProduct` 只取一条，结果可能不是满足条件的最优档/正确档；②前台用商品展示价算活动价，多规格商品每个 SKU 实际价不同，展示的促销价与下单价不一致；③选中 SKU 后仍显示商品级促销。
- 根因: 促销仓储只返回单条；展示按商品价计算。
- 原修复方式: `GetAllActiveByProduct`（scope=product、is_active、starts_at/ends_at 为空或覆盖 now，`ORDER BY min_amount ASC`）；`ApplyPromotion` 用 `subtotal=unitPrice*qty`，从 min_amount 最高往低找第一个 `min_amount<=0 || subtotal>=min_amount` 的规则。公共商品视图对每个活跃 SKU 用 SKU 价格（priceCarrier.PriceAmount=sku.PriceAmount）调用 ApplyPromotion，只有促销价 < SKU 原价才输出 `promotion_price_amount`；商品级取所有 SKU 中最低促销价；输出 `promotion_rules` 供展示。前端选中 SKU 显示 SKU 促销价/节省额。
- **对我们实现的要求**: 活动价匹配以"SKU 单价×数量"为 subtotal，按门槛降序选第一条满足的；展示与下单共用同一计算函数（传 SKU 价），避免展示价≠结算价；促销价不低于原价时不展示。
- **必测用例**: 规则 [min 0: 95折, min 100: 9折, min 300: 8折]，单价 50：qty1 → 47.50；qty2(100) → 45.00；qty6(300) → 40.00；两个 SKU 价 10/20 → 各自促销价分别计算，商品级展示最低者；starts_at 在未来的规则不生效。

#### PRC-09 防 0 元购：订单总额必须 >0，原价按折前价累计

- 提交: a1b2e736 2026-02-25, d649835e 2026-02-25
- 严重度: **高**
- 问题现象: 管理员误配活动价（percent=100 或 fixed 减免 ≥ 单价）或优惠券面额 ≥ 订单价时，订单总额为 0 仍可下单（0 元购）。另 `originalAmount` 用活动价后单价累加，导致原价/折扣展示错误：单价 59.90×2，活动 20%，原价应为 119.80、活动折扣 23.96、总额 95.84，旧代码原价算成 95.84。
- 根因: `buildOrderResult` 没有对最终 totalAmount 下限校验；原价使用了错误基数。
- 原修复方式: `order_service.go::buildOrderResult` 中 `baseTotal := basePrice*qty`，`originalAmount += baseTotal`；最终 `if totalAmount <= 0 → ErrInvalidOrderAmount`（活动价使单价为 0 已有 `ErrProductPriceInvalid`）。后台 `Promotions.vue` 增加各类型说明，fixed 减免值 ≥ 参考单价（启用 SKU 最低价，无 SKU 取商品价）时显示高风险警告。
- **对我们实现的要求**: 订单构建后（活动价+优惠券+会员价等全部应用完）总额必须 >0，否则拒绝（ErrInvalidOrderAmount）；活动价计算后单价 <=0 → ErrProductPriceInvalid。original_amount = Σ(折前 SKU 单价×数量)。后台活动价表单要显示风险提示。所有金额 decimal round(2)。
- **必测用例**: ① 商品 10 元 + percent 100 活动 → ErrProductPriceInvalid；② 商品 10 元 + 面额 10 的固定优惠券 → ErrInvalidOrderAmount；③ 59.90×2 + 20% 活动 → original=119.80, promotion_discount=23.96, discount=0, total=95.84。

#### PRC-10 商品更新未携带 wholesale_prices 时被静默清空

- 提交: 9d6a4411 2026-06-03, d46e21a0 2026-06-11, 03c80206 2026-06-11
- 严重度: **中**
- 问题现象: 后台商品编辑（或其它不关心批发价的局部更新）请求中省略 wholesale_prices，Update 把 product.WholesalePrices 覆盖为空，已配阶梯丢失。
- 根因: Go 切片 nil 与“空数组”语义不区分。
- 原修复方式: 请求/输入改为 `*[]WholesalePriceRequest`：nil=不修改，非 nil（含 []）=整体覆盖；新增独立接口 `PATCH /admin/products/:id/wholesale-prices`（body wholesale_prices 必填，RBAC 注册）只改批发价字段（QuickUpdate）。
- **对我们实现的要求**: serde 用 `Option<Vec<Tier>>` 区分缺省与空数组（必要时 `#[serde(default, deserialize_with=...)]` 区分 null）；Update 缺省时保留原值。新增独立 PATCH 接口并加入 RBAC 策略。
- **必测用例**: 创建带 1 档 → PUT 不带 wholesale_prices → 仍 1 档；PUT `wholesale_prices: []` → 清空；PATCH 非法阶梯 → 400 error.wholesale_price_invalid。

#### PRC-11 金额字段语义：unit_price 已扣活动价/会员价，coupon 单独分摊，避免重复扣减

- 提交: 0850c757 2026-04-06, f39f94d2 2026-05-07, e3749771/ac72b881 2026-05-09
- 严重度: **中**
- 问题现象: ①后台订单详情利润按 `unit_price*qty - promotion_discount - member_discount - cost` 计算，而 unit_price 已经是扣完活动价和会员价后的价格，导致利润被重复扣减（与仪表盘不一致）；②`OrderSummary` DTO 缺少 discount_amount/member_discount_amount/promotion_discount_amount，列表页无法展示优惠；③订单明细的"单价/小计"显示的是折后价，用户看不到原价；"优惠金额"实际只是优惠券金额。
- 根因: 各个金额字段的语义没有写清楚。
- 原修复方式: 利润改为 `revenue = total_price`（再减成本）。Summary DTO 补上三个折扣字段。订单项增加 `original_unit_price/original_total_price`，明细展示原价、各项分摊、"共减 = promotion + member + coupon"、"实付 = total_price - coupon_discount_amount"（不小于 0）；把"优惠金额"标签改为"礼券金额"；结账页每行按 preview 的 `original_total_price` 与 `total_price - coupon_discount_amount` 显示原价与优惠价，合计标为"应付金额（预估）"，以服务端计算为准。
- **对我们实现的要求**: Rust 订单项字段约定：original_unit_price（原价）、unit_price（活动价/会员价后）、total_price=unit_price*qty、coupon_discount_amount（订单级优惠券分摊到行）、行实付=total_price-coupon_share；订单级 discount_amount 只表示优惠券。利润=Σ(total_price - coupon_share?) - cost：口径需与仪表盘统一，不能重复扣减；列表 DTO 也要带上折扣字段。
- **必测用例**: 原价 100、活动价 80、会员再减 5（unit_price=75）、qty=2、优惠券 10 → total_price=150、行实付=140、共减=60；利润计算中不再重复减去 promotion/member。

#### PRC-12 会员等级自动升级：同 sort_order 不升级；管理员手动置为已支付不触发；注册默认等级被覆盖

- 提交: 7c8e0820 2026-03-30, 70c30c5c 2026-03-30, 99dab6b6 2026-03-29, 2b47bf5d 2026-03-30
- 严重度: **中**
- 问题现象: ①默认等级和 VIP 的 sort_order 都是 0，VIP 的消费门槛为 0.01。`CheckAndUpgrade` 用 `level.SortOrder <= currentSortOrder` 跳过，所以同 sort_order 的等级永远升不上去（测试 `OnOrderPaidUpgradesWithEqualSortOrder`：消费 0.01 后应升级为 vip）。②管理员在后台把订单手动改为 paid（`UpdateOrderStatus`，包括父订单分支和普通分支）时没有调用 `OnOrderPaid`，累计消费和升级都不触发；支付回调里 `OnOrderPaid`、`OnRechargeCompleted` 的错误被 `_ =` 吞掉。③注册时先 `AssignDefaultLevel(user.ID)`，随后 `userRepo.Update(user)` 用内存中 member_level_id=0 的旧对象整行保存，把默认等级覆盖回 0。
- 根因: 比较条件写错；状态变更入口有多个但触发不一致；整行 Save 覆盖了其他写入。
- 原修复方式: ①条件改为 `SortOrder < current` 时跳过，并且 `level.ID == user.MemberLevelID` 时跳过；列表按 sort_order desc 排序（70c30c5c 撤回了附加的 `id desc`）。②UpdateOrderStatus 在 target=paid 且 user_id>0 时调用 `OnOrderPaid(order.UserID, order.TotalAmount)`，所有调用的错误都记 warn 日志。③把 AssignDefaultLevel 挪到最后一次 Update 之后。
- **对我们实现的要求**: 升级判断：候选等级按 sort_order desc 排列，候选 sort_order >= 当前、id != 当前、且满足门槛即可升级（门槛比较用 Decimal 的 >=）。"订单变为 paid"的所有入口（网关回调、余额支付、管理员手动标记、上游采购成功）共用一个 `on_order_paid` 钩子，并且要幂等（同一订单只累计一次消费，建议记录 counted 标记）。更新用户时只 update 变化的列（sea-orm ActiveModel 只 set 需要的字段），禁止整行 save 覆盖。
- **必测用例**: ①default(sort0) 和 vip(sort0, 门槛 0.01)，支付 0.01 后升级为 vip；②已是高等级 gold(sort10) 的用户支付后不被降到 vip；③管理员手动把订单标记为 paid 后 total_spent 增加并触发升级；④同一订单重复触发 paid 钩子，total_spent 只加一次；⑤注册后 member_level_id = 默认等级 id。

#### PRC-13 商品展示价应取首个启用 SKU 价，活动价基于展示价计算

- 提交: 9e3d0c6b 2026-02-25, 91dc1c3e 2026-02-25
- 严重度: **中**
- 问题现象: 多 SKU 商品列表/详情显示 `product.price_amount`（59.90）而实际默认 SKU 价 89.90；活动价 fixed-10 也基于 59.90 算出 49.90，与下单价 79.90 不一致。另外结算页同一商品多个 SKU 时，人工表单按 `productId:skuId` 分组，仅在该商品只出现一次时才写 `payload[productId]`，而后端 `resolveManualFormSubmission` 按 productId 取表单，导致多 SKU 场景人工表单数据丢失。
- 根因: 展示层与下单层取价来源不一致；前后端人工表单 key 不一致。
- 原修复方式: `public.go::resolvePublicDisplayPrice` 取 SKUs 中第一个 `IsActive` 的价格（SKU 已按 `sort_order DESC, id ASC` 预加载），否则商品价；`ApplyPromotion` 用替换过价格的副本；判断 `discounted < displayPrice`。订单项快照增加 `image`（`firstProductImage`）。前端 Checkout 人工表单按 productId 分组，一个商品只填一次（显示"适用于 N 个规格"）。
- **对我们实现的要求**: 公共商品展示价 = 排序后第一个启用 SKU 的价格；活动价展示基于该价；只有活动价严格小于展示价才返回 promotion_price_amount。人工交付表单以 product_id 为 key，前后端一致。订单项 snapshot 保存第一张非空图片。
- **必测用例**: 商品价 59.90，SKU A(active, sort 100, 89.90)、B(active, sort 10, 49.90) → 展示 89.90；加 fixed-10 活动 → promotion_price 79.90；同商品两个 SKU 进购物车提交人工表单 → 后端两个订单项都能拿到表单数据。

#### PRC-14 列表展示价与活动价必须来自同一 SKU

- 提交: 51fcd4d0 2026-03-24
- 严重度: **低**
- 问题现象: 前台商品卡片"原价"取默认展示 SKU，活动价却取所有 SKU 中最低的活动价 → 出现"原价 ¥10、活动价 ¥50"或折扣比例错误。
- 根因: 两个值取自不同 SKU。
- 原修复方式: `resolvePublicDisplaySKUID` = 第一个启用 SKU（已按 sort_order DESC 排序），商品级 promotion 信息只取该 SKU 的活动价。
- **对我们实现的要求**: 商品列表的 price/promotion_price/member_price 统一以同一"展示 SKU"计算。
- **必测用例**: SKU1(排序高) 价 10 无活动、SKU2 价 100 活动价 50 → 列表 promotion_price 为空（或与 SKU1 一致），不能出现 50。

#### PRC-15 优惠券按适用商品筛选：JSON 数组列需边界匹配

- 提交: b2ea219e 2026-02-23, 85a5702a 2026-02-23
- 严重度: **低**
- 问题现象: 后台按商品筛选优惠券时，`scope_ref_ids`（JSON 数组文本如 `[1,2,3]`）用简单 `LIKE '%1%'` 会误命中 11、21。
- 根因: 文本 LIKE 无边界。
- 原修复方式: `coupon_repository.List`：`scope_ref_ids = '[id]' OR LIKE '[id,%' OR LIKE '%,id,%' OR LIKE '%,id]'`；handler 对 `scope_ref_id` 非正整数返回 400。
- **对我们实现的要求**: 对 JSON 数组列筛选时使用边界匹配（或使用方言 JSON 函数/关联表）；序列化时保证无空格的 `[1,2,3]` 格式，否则边界匹配失效。
- **必测用例**: 优惠券 A scope_ref_ids=[11,21]，B=[1,5] → 按 scope_ref_id=1 仅返回 B；scope_ref_id=abc → 400。

## 9. 推广返利（AFF，1 条）


#### AFF-01 佣金到期确认改为调度任务

- 提交: fe20c9c4 2026-03-05
- 严重度: **低**
- 问题现象: 每个 worker 进程内 goroutine 每分钟执行 `ConfirmDueCommissions`，多实例部署时并发执行同一批佣金确认。
- 根因: 进程内定时器无分布式协调。
- 原修复方式: 改为 asynq Scheduler 注册 `@every 1m` 的 `affiliate:confirm_commissions` 任务入队，由消费者执行（Stop 时 Shutdown scheduler）。
- **对我们实现的要求**: 周期任务需考虑多实例（分布式锁/单 leader 调度），且 `ConfirmDueCommissions` 本身必须幂等：用 `UPDATE ... SET status='confirmed' WHERE id=? AND status='pending' AND confirm_at<=now` 以 rows_affected 决定是否入账。
- **必测用例**: 两个 worker 同时执行确认 → 每条佣金只确认/入账一次。

## 10. 分销商/租户/域名（RSL，10 条）


#### RSL-01 多次部分退款累计超扣分销利润

- 提交: f7b6381c 2026-06-20
- 严重度: **高**（高（资损/分销商利益受损））
- 问题现象: 订单 130、利润 30。第一次退 52：旧算法 ratio=52/130，扣 12；第二次退 78：旧算法 ratio=78/(130-52)=1，再扣 30 → 累计 42 > 30。
- 根因: 以“递减的剩余额”为分母计算比例，但分子乘的是原始总利润；未对已扣总额封顶。
- 原修复方式: 比例固定以订单总额为分母（refund/orderAmount）；查询 `SumLedgerAmountByOrderAndType(orderID, refund_deduct)` 得到已扣总额，remainingProfit = profit - |已扣|，≤0 直接返回；若本次后累计退款 ≥ 订单额（全额退款）或计算值 > remainingProfit，则扣减 = remainingProfit；单次退款额先 clamp 到剩余可退额。item 级分摊比例改为 deduct/profit，保证明细和=总额。
- **对我们实现的要求**: 按比例扣减类逻辑一律“总额为分母 + 累计封顶 + 最后一次收敛到剩余”，并对同一订单加行锁/幂等键（`refund_deduct:{refund_record_id}`）。
- **必测用例**: 130/30 订单先退 52 再退 78 → 累计扣减 = -30.00；退 130 一次 → -30；三次退 43.33/43.33/43.34 → 累计恰好 -30；再多一次退款 → 不再产生扣减。

#### RSL-02 提现校验只看正数流水导致超额提现；余额缓存重复扣减 withdrawn

- 提交: 1f382a26 2026-06-16, f7b6381c 2026-06-20
- 严重度: **高**（高（平台资损））
- 问题现象: ①available 流水：利润 +100、退款扣减 -50，净可用 50；`ApplyWithdraw` 仅按可锁定的正数流水凑金额，申请 80 被放行，账户被提成负数。②`refreshBalanceAccountTx` 计算 net = SUM(available) - SUM(withdrawn)，但 withdrawn 流水本就不在 available 中，重复减去，提现打款后余额被算成负数/冻结；部分提现（60 中提 25）后剩余应为 35。
- 根因: 余额口径不统一；提现前没有以净额做总量校验。
- 原修复方式: ApplyWithdraw 在锁定账户后先 `SumLedgerAmountGroupedByStatus([available])`，amount > 净可用 → `ErrResellerWithdrawInsufficient`，再锁流水拆分（拆分剩余行的幂等键改为 `split:{rowID}:{nano}` 避免键无限增长）。余额刷新 net = SUM(available)（不再减 withdrawn），负数时记 negative 缓存。dashboard 增加 `withdraw_enabled` / `withdraw_disabled_reason`（profile 非 active → profile_inactive；settlement_status 非空且非 normal → settlement_unavailable）；handler 错误码改为具体 i18n key（reseller_withdraw_insufficient、reseller_balance_frozen 等）。
- **对我们实现的要求**: 可提现额 = 该币种 available 状态流水（含负数）之和，提现申请在账户行锁内先比较总额；余额缓存只由 available/locked 汇总得出；前端依据后端 withdraw_enabled 控制按钮。
- **必测用例**: +100、-50 两条 available → 申请 80 被拒（ErrResellerWithdrawInsufficient），申请 50 成功；60 可用中申请 25 并打款 → 缓存 available=35、locked=0、negative=0、normal；全额提现打款 → 全为 0；profile disabled → withdraw_enabled=false、reason=profile_inactive；settlement frozen → settlement_unavailable。

#### RSL-03 被禁用分销商的域名仍可解析为可用租户；分销站可访问分销商控制台；域名状态流转

- 提交: f7b6381c 2026-06-20, 3e3cca78 2026-06-18
- 严重度: **高**（高（被禁用分销商继续接单收款））
- 问题现象: ①分销商 profile 被 disabled 后，其 active+verified 域名 `FindActiveVerifiedDomain` 仍命中，店铺继续营业；②在分销站域名下可以请求 `/api/v1/user/reseller/*` 控制台接口；③审核通过时 subdomain_base 未配置只返回通用“请求参数错误”。
- 根因: 域名查询未联表检查 profile 状态；控制台路由未限制 tenant。
- 原修复方式: 仓储查询 JOIN reseller_profiles（deleted_at IS NULL 且 status=active）；resolver 额外判断 `domain.Profile.Status != active` → 写 not-found 缓存并返回 Unavailable tenant。新增中间件 `RequireMainTenantForResellerConsole`：tenant 为有效分销 tenant 时返回 403 envelope（HTTP 200 + status_code 403）。profile 状态机：rejected 仅从 pending_review；disabled 可从 pending_review/active/rejected；active（恢复）仅从 disabled；变更后删除该分销商所有域名的缓存。域名状态机：active 仅从 pending_review/disabled（同时设 verified、若无其它 active+verified 主域名则设为主域名）；disabled 仅从 pending_review/active，若原为主域名则把第一个 active+verified 的其它域名提为主域名；SetPrimary 只允许 active+verified 域名。自定义域名：禁止含 `://`、`/?#`，不可为 main host，不可为 subdomain_base 本身或其子域；系统子域名 label 仅 `[a-z0-9-]`、≤63、不以 `-` 开头结尾、只能一级；唯一冲突映射为 ErrResellerDomainConflict；subdomain_base 缺失返回专用错误 `error.reseller_subdomain_base_missing`。markup 校验：default/max 不可为负，max>0 时 default ≤ max。
- **对我们实现的要求**: 租户解析必须同时校验 域名状态 + 验证状态 + 分销商 profile 状态，并在任何状态变化后失效缓存；分销控制台接口只能在主站访问；状态流转用显式白名单。
- **必测用例**: profile disabled + 域名 active verified → 解析为 Unavailable、ResellerID=nil；分销站 Host 访问 /user/reseller/profile → status_code 403 且 handler 未执行；主站访问 → 200；禁用主域名 → 另一个 active verified 域名成为主域名；`hello` + base `shop.example.com` → `hello.shop.example.com`；`a.b` 标签 / `-x` / 主站域名 → 拒绝；subdomain_base 为空审核 → 专用错误信息。

#### RSL-04 利润待确认期内发生退款，扣减被记成“可用负余额”误冻结账户；到期确认不刷新余额缓存

- 提交: bb7941cb 2026-06-19
- 严重度: **高**（高（资金/冻结误判））
- 问题现象: 分销订单利润 30 入账为 pending_confirm（7 天确认期），期间退款一半，refund_deduct -15 以 `available` 状态入账 → 可用余额 -15，账户被标记 negative_balance 并冻结提现；而且会从其它已到账订单的可用余额中扣。另外 `ConfirmDueLedgerEntries` 只改流水状态，不刷新 `reseller_balance_accounts` 缓存，dashboard 可用余额长期停留在旧值。确认天数硬编码 7。
- 根因: 扣减流水状态与对应利润流水状态不对齐；缓存刷新遗漏。
- 原修复方式: `HandleRefundDeductTx` 查询 `order_profit:{orderID}` 流水，若为 pending_confirm，则扣减流水也为 pending_confirm 并沿用其 available_at（缺失时 now+confirmDays），metadata 记 deduct_status。`ConfirmDueLedgerEntries` 放入事务：先 `ListDueLedgerScopes(now)`（status=pending_confirm AND available_at<=now 的 reseller_id+currency 去重），再批量 UPDATE 为 available，然后对每个 scope `refreshBalanceAccountTx`。余额刷新用 `SumLedgerAmountGroupedByStatus` 一次查询 available/locked。确认天数改为配置 `reseller.settlement_confirm_days`（默认 7，<0 取 0，>3650 取 3650）。
- **对我们实现的要求**: 退款扣减流水的状态/到账时间必须与该订单利润流水一致；任何改变流水状态的批处理都要在同一事务中刷新受影响账户的余额缓存；配置做上下限钳制。
- **必测用例**: 利润 30 pending（ConfirmDays=7），退 65/130 → 扣减 -15 为 pending_confirm 且有 available_at；此时余额 available=0、negative=0、status=normal；以 now+8 天调用确认 → available 流水和=15，余额缓存 available=15、negative=0、normal；ConfirmDays=-5→0，99999→3650。

#### RSL-05 分销利润记账、退款扣回与提现锁定

- 提交: 7daaa373 2026-06-16, a2e8f1b5 2026-06-16
- 严重度: **高**
- 问题现象: （新功能）支付成功记利润、退款扣回、提现，需防重复入账与并发超提。
- 根因: —
- 原修复方式: 支付成功事务内 PostOrderProfitTx：无快照记 warn 跳过（不回滚支付）、profit_eligible=false 或利润≤0 跳过；ledger 幂等键 `order_profit:{orderID}`，状态 pending_confirm，available_at=now+confirmDays，定时任务 ConfirmDueLedgerEntries 转 available。退款（后台手工退款与钱包退款）同事务 HandleRefundDeductTx：幂等键 `refund_deduct:{refundRecordID}`，负数金额、状态 available；扣回额 = profit × refund/remainingBefore（remainingBefore = 分销订单额 − 此前已退）。提现 ApplyWithdraw：金额>0、币种、渠道、账号必填；事务内 GetOrCreateBalanceAccountForUpdate，账户状态 negative_balance/frozen_review/disabled 拒绝；ListAvailableLedgerEntriesForUpdate 按条目累加，最后一条超出时拆分（原条目改为锁定额，新建剩余 available 条目，幂等键带 split 后缀），不足 → ErrResellerWithdrawInsufficient；选中条目置 locked 并关联 withdraw_request_id；审核 reject 解锁回 available，pay 置 withdrawn（GetWithdrawRequestByIDForUpdate + 状态必须 pending）。后台财务路由挂在支付合规组并加入 RBAC。
- **对我们实现的要求**: 同样的幂等键与 FOR UPDATE 锁；利润入账与支付成功同事务。扣回公式必须为 profit × refund / orderAmount 并以“累计扣回 ≤ profit”封顶（详见 f7b6381c）。
- **必测用例**: 同一订单支付回调重复 2 次 → 仅 1 条 order_profit；两次各退一半 → 扣回合计 == profit；可用 30+50，提现 60 → 30 条锁定、50 条拆成 30 锁定 + 20 available；并发两次提现各 60（可用 80）→ 一个成功一个 insufficient；reject 后条目恢复 available。
- ⚠ 注：本功能初版扣回公式 `profit × refund / remainingBefore` 在多次部分退款时会超扣（先退一半再退另一半合计扣 1.5×profit），已由 f7b6381c 修正，见 RSL-01。

#### RSL-06 租户（分销站）解析与缓存隔离

- 提交: a0792c8f 2026-06-15
- 严重度: **高**
- 问题现象: （新功能基础层）按 Host 区分主站/分销站；公开配置缓存键原为单一 `public:config`，多租户下会串站。
- 根因: —
- 原修复方式: ResellerTenantMiddleware 挂在 storefront 组（/public、/guest、/auth、用户路由），解析失败 500，未找到/未验证域名 → 404 "site unavailable"；Host 归一化（小写、去端口、去尾点、IPv6 方括号）；仅当配置 reseller.trusted_forwarded_host=true 才采信 X-Forwarded-Host 首值；main_hosts 默认 localhost/127.0.0.1/::1；reseller.enabled=false 全部视为主站；域名命中缓存 5 分钟、未命中负缓存 60 秒；只接受 FindActiveVerifiedDomain（active 且已验证）；公开配置缓存键 `public:config:main` / `public:config:reseller:{id}`，清缓存用 DelPattern。
- **对我们实现的要求**: axum 中间件注入 TenantContext 扩展；默认不信任 X-Forwarded-Host；所有租户相关缓存键带租户维度；域名变更时删除正/负缓存。
- **必测用例**: Host "Shop.EXAMPLE.com.:8080" 归一化为 shop.example.com；未验证域名 → 404；trusted_forwarded_host=false 时伪造 X-Forwarded-Host 不生效；主站与分销站 GetConfig 缓存互不影响。

#### RSL-07 分销站定价与订单租户隔离

- 提交: b9df90c9 2026-06-15
- 严重度: **高**
- 问题现象: （新功能）分销站下单需使用分销价，且防止分销商低价倾销/自买刷利润；订单查询需租户隔离。
- 根因: —
- 原修复方式: 分销订单禁止优惠券（传 coupon_code → ErrResellerCouponNotAllowed），且不应用活动价/批发价/会员价（各 discount 置 0、PromotionID/MemberLevelID 置空）；商品/SKU 设置 is_listed=false → ErrResellerProductNotListed（列表中在分页前排除，详情拒绝）；分销单价必须 > 0、≥ SKU 基础价、≥ SKU 成本价（>0 时），加价率 (reseller-base)/base×100 不得超过 profile.max_markup_percent（>0 时）；markup 计算 base×(100+p)/100 Round2；利润 = 分销总额 − 基础总额；买家为分销商本人或关联账号 → profit_eligible=false、effective_profit=0（记录 risk snapshot）；游客 buyer_user_id=0。订单写入 reseller_id 与快照；所有用户/游客订单查询（详情、列表、统计）强制 scope：主站 `reseller_id IS NULL`，分销站 `reseller_id = ?`。
- **对我们实现的要求**: 完整实现上述价格下限/加价上限/自买校验；订单仓储所有前台查询方法必须带租户 scope 参数（不可遗漏 stats、guest 查询、按 order_no 查任意父/子单）。
- **必测用例**: 基础价 100、成本 90、max_markup 50%：分销价 99 → 拒绝；151 → markup_exceeded；120 → 通过 profit=20×qty；分销商本人下单 → effective_profit=0；分销站用户查询主站订单号 → 404；带优惠券 → 拒绝。

#### RSL-08 分销站公告以错误结构输出、禁用时泄漏主站公告

- 提交: 1a9346a6 2026-07-02, bf0081ff 2026-07-02
- 严重度: **中**（中（白标站泄漏主站内容））
- 问题现象: 分销站保存的 announcement 原样（含 enabled 字段、无 version）覆盖到 public config，前台弹窗组件期望 `{type,title,content,version}`，导致启用的公告不弹、`success` 类型无样式；未启用时仍输出对象。
- 根因: overlay 直接透传存储结构，没有转换成与主站一致的公共结构。
- 原修复方式: `applyResellerAnnouncementToPublicConfig`：先 `delete(out,"announcement")`（保证主站公告不泄漏）；enabled=false 或内容为空 → 不输出；否则输出 `{type(默认 info), title, content, version=homeAnnouncementVersion(type,title,content)}`（8 位指纹，与主站不同），不暴露 enabled；兼容 map 与 models.JSON 两种载体。前端 AnnouncementModal 增加 success 类型；vault 页脚优先用 brand.site_description。
- **对我们实现的要求**: 分销 overlay 对公告/导航等“分销站自有字段”必须先清除主站值再按需写入，并输出与主站相同的公开结构（含内容指纹 version，用于前端“已读”判断）。
- **必测用例**: 主站 config 有 announcement{version:"main0000"}，分销站公告 disabled → 输出无 announcement 字段；enabled、type=success、content zh-CN="<p>测试测试</p>" → type=success、无 enabled 键、version 长度 8 且 ≠ main0000、content 原样。

#### RSL-09 分销展示价因单个 SKU 失效配置导致整页/整商品报错

- 提交: bb7941cb 2026-06-19
- 严重度: **中**
- 问题现象: 分销商保存了 SKU 固定价 80，之后站长把基准价调到 100，`ResolveDisplayPrices` 对该 SKU 校验失败直接返回 error，整个商品列表接口失败。
- 根因: 展示路径复用了保存时的严格校验并向上抛错。
- 原修复方式: 展示路径中单个 SKU 定价解析/校验失败 → 记 warn、放入 `HiddenSKUIDs` 跳过；无 SKU 商品失败 → 返回 Visible=false。下单路径仍严格校验（ffc6d9be 测试：预览与下单对隐藏商品都拒绝，且运行时价格与预览、快照一致）。
- **对我们实现的要求**: 展示层对脏配置降级（隐藏 SKU/商品），下单层严格拒绝；两者使用同一定价函数。
- **必测用例**: SKU11 固定价 130（有效）、SKU12 固定价 80 低于基价 100 → 结果 Visible、HiddenSKUIDs[12]=true、无 12 的价格、DisplaySKUID=11 价格 130；下单 SKU12 → 报 ErrResellerPriceBelowBase 类错误。

#### RSL-10 分销订单买家标识脱敏；分销站结算隐藏优惠券

- 提交: f7b6381c 2026-06-20, e9d64a2a 2026-06-23
- 严重度: **低**
- 问题现象: 分销商订单列表会员买家只显示 "user"，无法区分；分销站结算页显示优惠码输入框和恒为 0 的优惠/促销/批发行，买家填了也无效（后端对分销订单清零所有折扣，主站券返回 `error.reseller_coupon_not_allowed`）。
- 根因: 展示信息不足 / 前端未感知租户模式。
- 原修复方式: 会员买家批量查 email 后脱敏为 `b***@example.test`，无 email 回落 `user#{id}`；游客用 guest_email 脱敏，否则 "guest"。前端 `appStore.isResellerTenant`（config.tenant.mode==='reseller'）时隐藏优惠码区块与 coupon/promotion/wholesale 预览行。
- **对我们实现的要求**: 分销商只能看到脱敏后的买家信息（首字符 + *** + @域名）；分销站下单后端拒绝/忽略优惠券，前端同步隐藏。
- **必测用例**: 会员 buyer-label@example.test → "b***@example.test"（列表与详情一致）；分销 tenant 下单带 coupon_code → 报 reseller_coupon_not_allowed 或折扣为 0；前端 reseller tenant 结算页无优惠码输入框。

## 11. 上游对接/采购/下游回调/对账（含 SSRF）（UPS，22 条）


#### UPS-01 下游回调客户端 SSRF 防护：只连公网 IP、不跟随重定向

- 提交: 8d73eb72 2026-09-15
- 严重度: **高**
- 问题现象: 下游（本站作为上游 B 向 A 推送）的回调地址由对方配置，原 `http.Client{Timeout:15s}` 会连接 127.0.0.1、10.x、169.254.169.254（云元数据）等内网地址，且默认跟随 3xx 重定向，可被用于 SSRF。
- 根因: 回调 URL 可控，但 HTTP 客户端未限制目标地址，也未禁止重定向（DNS rebinding/重定向到内网）。
- 原修复方式: callbackclient/transport.go `newSafeHTTPClient()`：自定义 `DialContext`，拨号时自行 `LookupIPAddr` 解析域名，逐个 IP 检查 `isPublicIP`（排除 loopback、private、unspecified、multicast、link-local 单播/组播、interface-local 组播、100.64.0.0/10 CGNAT），只连公网 IP；`CheckRedirect` 返回 `http.ErrUseLastResponse`（不跟随）；拨号 10s、TLS 10s、响应头 15s、总 15s。
- **对我们实现的要求**: reqwest 客户端用 `redirect::Policy::none()`，并通过自定义 DNS resolver（`reqwest::dns::Resolve`）在解析阶段过滤非公网 IP（包括 IPv6 ::1、fc00::/7、fe80::/10、IPv4-mapped 地址），在连接时校验而非仅在保存 URL 时校验（防 DNS rebinding）。此规则同样适用于所有"用户/对端可配置 URL"的出站请求（通知 webhook 等）。
- **必测用例**: 回调地址 `http://127.0.0.1:xxxx`、`http://10.0.0.1`、`http://169.254.169.254`、`http://100.64.1.1`、解析到 127.0.0.1 的域名 → 投递失败，错误为 forbidden address；公网测试服务器返回 302 → 不跟随，按 302 结果记录。

#### UPS-02 上游回调：状态机守卫 + 先写交付记录再推进状态 + body 限 1MB + 密钥解密失败拒绝

- 提交: f517f12b 2026-09-15
- 严重度: **高**
- 问题现象: (1) 已 fulfilled/completed/refunded/canceled 的采购单再收到 `delivered`/`canceled` 回调仍会被处理（重复发货、已交付单被取消、已退款单重新标成交付）。(2) 原先先把采购单改为 fulfilled 再创建交付记录，交付记录写失败时采购单已是终态，上游重试被拒，本地订单永远没有卡密。(3) `io.ReadAll` 无上限，可被大 body 打爆内存。(4) `DecryptSecret` 失败时静默使用密文当密钥继续验签。
- 根因: 缺状态转换白名单；写入顺序错误；缺 body 上限；错误吞掉。
- 原修复方式: procurement/application/callback.go 新增 `isUpstreamTransitionAllowed(current, upstream)`：upstream ∈ {delivered, completed, fulfilled, canceled} 且 current ∈ {fulfilled, completed, refunded, canceled} → 拒绝（记日志、返回 nil 即 ok）；退款类回调（refunded/partially_refunded）不受限（部分退款可能先于交付到达）。delivered 分支改为先 `createUpstreamFulfillment`（幂等），失败则返回错误、采购单保持非终态，成功后才 `UpdateStatus(fulfilled)` 并写 upstream_payload。upstream_callback.go：`io.ReadAll(http.MaxBytesReader(w, body, 1<<20))`；解密失败 log + 返回 `{"ok":false,"message":"internal error"}`。
- **对我们实现的要求**: 采购单回调处理在事务中锁行（SELECT ... FOR UPDATE / SQLite 用 BEGIN IMMEDIATE）后按白名单判定转换；交付记录创建必须幂等且先于状态推进；axum 对该路由加 `DefaultBodyLimit::max(1<<20)`；密钥解密失败直接拒绝，绝不回退为原文。
- **必测用例**: 状态表：accepted→delivered 允许；partially_refunded→delivered 允许；fulfilled→delivered 拒绝；refunded→delivered 拒绝；canceled→delivered 拒绝；fulfilled→canceled 拒绝；accepted→canceled 允许；accepted→partially_refunded 允许；fulfilled→refunded 允许。模拟交付记录写入失败 → 采购单状态仍为 accepted，上游重试后成功。body 1MB+1 字节 → 拒绝。密钥解密失败 → ok:false，不调用 HandleUpstreamCallback。

#### UPS-03 上游回调归属校验：采购单必须属于本次认证的连接且上游订单号一致

- 提交: 47cc6207 2026-09-02
- 严重度: **高**
- 问题现象: 上游回调按 payload 里的 `downstream_order_no`（本地订单号）查采购单，任意一个已通过签名认证的连接（另一个上游站点）都能凭本地订单号伪造他人采购单的交付/取消状态。
- 根因: 验签只证明"是某个合法连接"，未校验该采购单属于这个连接。
- 原修复方式: upstream_callback.go 在处理前检查 `procOrder.ConnectionID != conn.ID || (procOrder.UpstreamOrderID != 0 && payload.OrderID != procOrder.UpstreamOrderID)` → 返回 `{"ok":false,"message":"procurement order not found"}`（不泄露存在性）。upstream_order_id 为 0（下单响应尚未落库、回调抢跑）时只校验连接归属。
- **对我们实现的要求**: 回调处理必须同时匹配 connection_id 与 upstream_order_id（已登记时）；不匹配时返回与"不存在"相同的响应。
- **必测用例**: 同连接同上游单号 → 处理；upstream_order_id=0 且同连接 → 处理；连接 2 冒用连接 1 的本地订单号 → 不处理；同连接但 payload.order_id=99 而登记 88 → 不处理。

#### UPS-04 导入/同步上游批发价时直接使用上游 SKU ID，造成本地阶梯指向错误 SKU

- 提交: 24a1d0e8 2026-07-06
- 严重度: **高**（高（本地 SKU ID 与上游 SKU ID 碰撞时会给错误 SKU 打折））
- 问题现象: `convertUpstreamWholesalePrices` 把上游 tier 的 `SKUID`/`SKUCode` 原样写入本地商品；而且导入时在本地 SKU 创建前就写入批发价。
- 根因: 未做上游 SKU → 本地 SKU 的映射。
- 原修复方式: 导入时先建 SKU，再用 `buildUpstreamWholesaleSKUIndex(localSKUs, upstreamSKUs, skuMappings)` 建立索引：优先 SKUMapping（upstream_sku_id→local_sku），其次 sku_code（小写）匹配，若本地与上游都只有 1 个 SKU 则直接对应。`resolveUpstreamWholesaleTierScope`：有 code 且命中 → 用本地 id/code，若同时有 sku_id 且映射到的本地 SKU 与 code 命中的不同 → 丢弃；有索引但 code 未命中 → 丢弃；仅有 sku_id → 必须映射命中否则丢弃；通用 tier 保留。丢弃时记 warn 日志。同步（SyncProduct / syncProductFromData）在价格重算后调用 `syncUpstreamWholesalePrices`，上游未返回批发价或转换后为空时**不覆盖**本地配置。
- **对我们实现的要求**: 任何从上游来的 ID 都必须经过映射表转换为本地 ID，无法映射的数据宁可丢弃并告警；同步时“上游为空”不能清空本地手工配置。
- **必测用例**: 上游 tier sku_id=5（上游）映射到本地 sku 12 → 本地 tier sku_id=12；上游 tier sku_code 不存在于本地 → 丢弃；上游 sku_id 与 code 指向不同本地 SKU → 丢弃；单 SKU 对单 SKU 自动对应；上游返回空批发价 → 本地阶梯保持不变。

#### UPS-05 站点对接的汇率/加价/取整/自动同步价格；修改连接参数后已映射商品售价联动

- 提交: da8331a8 2026-03-26, 7a3dcd35 2026-03-26, 3aab6181 2026-03-26, f32a9a0b 2026-03-26; a8e00b90 2026-06-20, fcd781d0 2026-06-20
- 严重度: **高**（高；中（售价与成本脱节，可能亏本销售））
- 合并条目: (1) 站点对接的汇率 + 加价 + 取整 + 自动同步价格 ｜ (2) 修改连接汇率/加价/取整后，已映射商品本地售价不联动
- 问题现象:
  - (1) 导入上游商品时本地售价=上游价（同价甚至亏本/币种不一致），需要按连接配置自动加价与汇率换算。
  - (2) 管理员把对接连接 exchange_rate 从 1 改为 6.9，已导入商品价格不变；前端“重新应用加价”按钮只在 markup≠0 时显示，汇率≠1 时看不到。
- 根因:
  - (1) 原先无定价规则。
  - (2) SiteConnection.Update 只保存连接，不触发价格重算。
- 原修复方式:
  - (1) SiteConnection 增加 `exchange_rate decimal(16,6) default 1`、`price_markup_percent decimal(10,4)`、`price_rounding_mode`(none/ceil_int/ceil_tenth)、`auto_sync_price`。`CalculateLocalPrice = CalculateMarkedUpPrice(up × rate(<=0 视为 1), markup, mode)`；markup=0 → Round(2)；result=up×(1+markup/100)，**负数返回 0**；ceil_int 向上取整到整数，ceil_tenth 向上到 0.1。导入时商品价/SKU 价均用该公式；SyncProduct 在 auto_sync_price 时更新已有 SKU 价格，并 `recalcProductPrice` = 启用 SKU 最低价；`ReapplyMarkup(connectionID)` 按 SKUMapping.UpstreamPrice 重算。Update 用指针区分"未传"与 0。
  - (2) SiteConnectionService 注入 `MarkupReapplier`（由 ProductMappingService 实现 `ReapplyMarkup(connectionID)`，setter 注入避免循环依赖）；Update 记录旧 ExchangeRate/PriceMarkupPercent/PriceRoundingMode，任一实际变化（decimal Equal 比较）才调用重算；重算失败只记 warn 不影响保存。前端按钮条件改为 markup≠0 或 exchange_rate≠1。
- **对我们实现的要求**:
  - (1) (1) 校验 markup_percent > -100（原实现允许负数导致价格为 0 → 0 元商品可被下单，必须禁止），exchange_rate > 0，rounding_mode 枚举；(2) 计算结果 <=0 时拒绝导入/不更新价格而不是写 0；(3) 使用 Decimal 输入（原实现用 float64 JSON 反序列化再 NewFromFloat，有精度问题）；(4) 同步/重算应在事务中批量更新 SKU 与商品最低价。
  - (2) 定价相关配置变更必须触发已映射商品价格重算（可在同一事务后异步/尽力而为，但要有告警与手动补救入口）；比较 decimal 用值相等而非字符串。
- **必测用例**:
  - (1) up=10, rate=7.2, markup=20, none → 86.40；ceil_int：12.01→13、12.00→12；ceil_tenth：12.34→12.40、12.30→12.30；markup=-150 → 保存连接 400；auto_sync_price=true 且上游 SKU 从 10 变 12 → 同步后本地 SKU 与商品价随之更新；false → 不变。
  - (2) 汇率 1→6.9 → reapply 被调用 1 次（connection_id 正确）；只改名称、汇率传入 1.0（与原值相等）→ 不调用。

#### UPS-06 上游库存兜底与“上游已删除”误判

- 提交: 5193129f 2026-05-27, f5806668 2026-05-27, f54ee787 2026-06-01
- 严重度: **高**
- 问题现象: 1) 本地缓存 upstream_stock 过时，下单成功但上游无货导致采购失败；2) 全量同步分页中途上游返回空页/超过 200 页时，未拉到的商品被标记 deleted，大批商品被误下架；3) 同步间隔设为 30h 时固定 24h 全量阈值导致每次都全量；4) SiteConnection Ping 创建 adapter 出错被忽略继续调用（nil adapter panic）。
- 根因: 缺少实时校验与“拉取完整性”判断。
- 原修复方式: EnsureUpstreamStockForOrder(skuID, qty)：设置 upstream_sync_config.pre_order_stock_check_enabled 关闭则跳过；无映射/缓存 -1(无限)/缓存≥qty 通过；否则实时 SyncProduct 后重读，仍不足返回 ErrUpstreamStockInsufficient；查库或同步失败 fail-open 放行。syncConnectionStock 记录 fetchComplete（已拉数量≥result.Total 才为真）；空页或达 maxPages 不置真；只有 includesInactive && fetchComplete 才标 deleted。全量间隔 = max(24h, 同步间隔×3)。Ping 中 adapter 创建错误直接返回。
- **对我们实现的要求**: 同样实现下单前上游库存兜底（fail-open）、分页完整性判定、全量间隔公式、错误早返回。
- **必测用例**: 缓存库存 1、下单 3、实时同步后为 5 → 通过；同步后 2 → ErrUpstreamStockInsufficient；同步报错 → 放行；上游 total=120 但第 2 页返回空 → 不标记任何 deleted；间隔 12h → 全量阈值 36h。

#### UPS-07 采购单终态失败时必须把本地订单回退并告警，accepted 超时也要告警

- 提交: 83f0b1a8 2026-04-02
- 严重度: **高**（高（用户已付款的订单卡在 fulfilling，没有人处理））
- 问题现象: 本地订单支付后提交到上游，状态变为 fulfilling。下列情况发生时，采购单被标记为 rejected/canceled，但本地订单一直停在 fulfilling，管理员也不知道，既不能重新交付也不能退款：上游返回不可重试错误（`rejectProcurement`）；可重试错误的重试次数用尽（`handleSubmitFailure`）；上游回调 canceled（`HandleUpstreamCallback`）。另外，采购单长时间停在 accepted 时巡检只是跳过。
- 根因: 采购单状态机的失败终态没有回写本地订单。
- 原修复方式: 新增 `rollbackLocalOrderOnProcurementFailure`：本地订单状态为 fulfilling 时回退到 paid（非 fulfilling 状态不动）；如果是子订单，再调用 `syncParentStatus` 同步父订单；然后 `notifyProcurementFailure`（入队 exception_alert，biz_type=procurement，数据包括 local_order_no 和 error）。在 rejectProcurement、handleSubmitFailure 的终态分支、上游 canceled 回调三处调用。`SyncAcceptedOrders` 巡检中，`time.Since(UpdatedAt)>24h` 仍为 accepted 时发送告警。配套测试覆盖：Reject/Canceled 回退到 paid、Delivered 创建 Fulfillment 且订单变为 delivered、NonRetryable 变为 rejected 并回退、Retryable 变为 failed（不 reject）、重试用尽变为 rejected 并回退、CreateForOrder 对同一订单幂等（ErrProcurementExists）。
- **对我们实现的要求**: 采购单进入每个终态失败（rejected/canceled/重试耗尽）时，都要带条件地把本地订单回退（`UPDATE orders SET status='paid' WHERE id=? AND status='fulfilling'`），同步父订单状态，并发送管理员告警。accepted 状态超过阈值（24h）发告警。CreateForOrder 必须幂等（local_order_id 唯一或存在性检查）。
- **必测用例**: ①上游返回不可重试错误码后采购单为 rejected，订单从 fulfilling 变为 paid，并产生 1 条告警；②可重试错误时采购单为 failed，订单保持 fulfilling；③retry_count 达到上限后变为 rejected 并回退；④上游 canceled 回调同样回退；⑤子订单回退后父订单状态被重新计算；⑥本地订单已是 delivered 时，收到 canceled 回调不改订单状态；⑦对同一订单 CreateForOrder 两次，第二次返回已存在。

#### UPS-08 上游价格解析失败被当作 0；成本价要按汇率换算

- 提交: 83f0b1a8 2026-04-02, f747fc1c 2026-03-31, 75303b84 2026-03-29
- 严重度: **高**（高（金额：本地售价可能被同步成 0 元））
- 问题现象: 之前写的是 `decimal.NewFromString(upSKU.PriceAmount)` 并忽略错误。上游返回的价格格式异常（空串、"N/A"）时 price=0，如果开启了 AutoSyncPrice，`CalculateLocalPrice(0,...)` 就把本地 SKU 售价同步成 0，用户可以 0 元购买；导入时新建的 SKU 也会是 0 元。另外，成本价 CostPriceAmount 直接取上游原始价格（上游币种），而本地售价是按汇率换算后的价格，例如上游 1 USD、汇率 7.2 时成本记成 1，利润计算完全错误。采购单也没有记录上游币种。
- 根因: 忽略了解析错误；成本和售价的币种口径不一致。
- 原修复方式: 所有解析点都检查 err。导入时解析失败记日志并按 0 处理（后续取 SKU 最低价逻辑会跳过 <=0 的值）。同步已有 SKU 映射时解析失败只同步 is_active/stock，不更新价格。同步时新出现的 SKU 解析失败则跳过，不创建。新增 `convertCurrency(price, rate)`（rate<=0 按 1 处理），所有成本价写入点（导入、SyncProduct、ReapplyMarkup、syncProductFromData）都改为 `上游价 × 汇率`，不加价。procurement_orders 新增 `upstream_currency`。`SyncAllStock` 并发同步时用 `errors.Join` 汇总所有连接的错误，不再只保留最后一个。
- **对我们实现的要求**: 上游金额一律 `Decimal::from_str` 并处理错误。解析失败时绝不写入 0 价，已有价格保持不变，新 SKU 不创建（或创建为下架状态）。本地售价 <= 0 时禁止上架或下单（下单侧再兜底校验 unit_price > 0）。成本价统一存为本地币种，并保留上游原始价格和上游币种。
- **必测用例**: ①上游 SKU price="abc"，同步后本地价格保持原值，只有库存被更新；②导入时一个 SKU 价格非法，该 SKU 不以 0 价上架；③汇率 7.2、上游 1.00 → cost_price=7.20，售价 = CalculateLocalPrice(1.00,7.2,markup)；④ReapplyMarkup 后 cost_price 仍是 7.20。

#### UPS-09 映射商品的 fulfillment_type 必须恒为 upstream

- 提交: a6b3584a 2026-03-23
- 严重度: **高**
- 问题现象: 后台商品详情对映射商品展示"上游原始交付类型"(auto/manual)，管理员编辑保存时前端把 auto 回传，`Update` 直接写入 → 商品变成本地 auto 发货，下单后从本地卡密库发货（无卡密则卡单），不再向上游采购。
- 根因: 展示字段与存储字段混用，服务端信任前端提交的 fulfillment_type。
- 原修复方式: `ProductService.Update` 中 `if product.IsMapped { fulfillmentType = upstream }`。
- **对我们实现的要求**: is_mapped=true 的商品在任何更新接口（全量编辑、快捷更新、批量）里 fulfillment_type 服务端强制 upstream；对外展示用单独的 effective/upstream_fulfillment_type 字段。
- **必测用例**: 映射商品提交 fulfillment_type=auto 的更新 → 返回与重新读取均为 upstream。

#### UPS-10 上游开放 API 安全加固（SSRF、幂等、限流、用户状态、时间窗）

- 提交: e1b3fd8b 2026-03-06, 22f101ed 2026-03-06（前端筛选）, b2e55f9b 2026-03-06（审批不回显 secret）
- 严重度: **高**
- 问题现象: ①下游传入任意 `callback_url`（如 http://127.0.0.1:6379、169.254.169.254），本站回调时形成 SSRF；②下游网络重试导致同一 `downstream_order_no` 重复建单重复扣钱包；③钱包支付失败后留下未支付订单占库存；④被禁用用户的 API Key 仍可调用；⑤签名时间窗 300s 过大；⑥无调用频率限制；⑦管理员审批凭证接口把 api_key/api_secret 明文返回给管理员。
- 根因: 开放 API 缺少输入校验与幂等键。
- 原修复方式: `validateCallbackURL`：scheme 仅 http/https、必须有 host、拒绝 localhost/127.0.0.1/::1/0.0.0.0 以及 IP 字面量的 loopback/private/link-local；`GetByCredentialAndDownstreamNo(credentialID, downstreamOrderNo)` 命中则直接返回已有订单信息；支付失败调用 `CancelOrder` 并在响应中返回 `status=canceled`；`GetByApiKey` Preload User，中间件 `cred.User==nil || status!=active` → 403 `user_disabled`；`MaxTimestampSkew` 300→60；`/upstream` 组加 `RateLimitMiddleware`（按 `Dujiao-Next-Api-Key` 头，无则 IP，60 次/60s，封 30s）；审批响应只返回 `approved:true`。
- **对我们实现的要求**: callback_url 校验需在"发送时"再做一次 DNS 解析并校验解析出的所有 IP（原实现只检查 IP 字面量，域名可指向内网/DNS rebinding），并禁止重定向到内网；(api_credential_id, downstream_order_no) 建唯一索引，靠唯一约束而非先查后插保证幂等；鉴权中间件检查凭证状态与所属用户状态；时间窗 ±60s；按 API Key 限流；secret 只在生成时对本人展示一次。
- **必测用例**: callback_url=http://10.0.0.1/x、http://localhost/、ftp://a.com → 400 invalid_callback_url；callback 域名解析到 127.0.0.1 → 拒绝；同一 downstream_order_no 并发两次 → 只生成 1 单且第二次返回同一 order_no；余额不足 → 返回 ok:false、status=canceled 且库存释放；禁用用户的 key → 403；时间戳偏差 61s → 401；第 61 次/分钟 → 429。

#### UPS-11 采购单提交错误分类、轮询到期不判失败、定时巡检

- 提交: b2e55f9b 2026-03-06
- 严重度: **高**
- 问题现象: ①连接被删/无 SKU 映射/本地订单无明细等永久性错误也返回 error 让 asynq 无限重试；②瞬时错误不记录原因；③提交成功后沿用提交阶段的 retry_count，轮询次数被提前耗尽；④短期轮询次数用完直接把采购单标 `failed`，但上游其实稍后会交付（人工发货），导致本地误判失败/退款；⑤没有兜底同步，回调丢失时订单永远卡在 accepted。
- 根因: 未区分永久/瞬时错误；轮询超时语义错误。
- 原修复方式: `rejectProcurement`（永久错误，状态置 `rejected` 并记 error_message，返回 nil 不重试）与 `markProcurementError`（瞬时错误，状态不变只写 error_message，返回 err 让队列重试）；成功后 `retry_count=0`；轮询固定间隔 `30s,30s,1m,1m,2m,2m,5m,5m,10m`，用尽只记日志"handoff to periodic sync"，不改状态；新增 `SyncAcceptedOrders` 每 30 分钟扫描 accepted（每批 200）调 `adapter.GetOrder`（15s 超时），delivered/completed → `HandleUpstreamCallback(delivered)`，canceled → canceled，其余等待。
- **对我们实现的要求**: 队列任务返回值要区分"可重试"与"终止"；采购单超时不得自动判失败/触发退款；必须有周期性对账巡检兜底，且巡检与回调走同一幂等的状态迁移函数。
- **必测用例**: SKU 映射缺失 → 采购单 rejected、任务不再重试；上游 5xx → 状态不变、error_message 有值、任务重试；轮询 9 次仍 processing → 状态仍 accepted；巡检发现上游 delivered → 本地交付一次（重复巡检不重复交付）。

#### UPS-12 下游回调：子订单找父引用、重发重置、交付信息取自子订单

- 提交: b2e55f9b 2026-03-06
- 严重度: **高**
- 问题现象: 本站作为 B 站时，下游下的单拆成父+子订单，`DownstreamOrderRef` 只挂在父订单上；子订单交付后 `EnqueueCallback(childID)` 找不到 ref 直接 return，下游永远收不到交付；父订单自身无 Fulfillment，回调/查单返回 fulfillment 为空；回调状态为 failed 后再次状态变化不会重发（只处理 `sent`）且 retry_count 不清零；支付成功时也不回调；多级链路中间节点收到上游交付后不通知下游。
- 根因: 回调查找与交付数据来源未考虑父子订单结构。
- 原修复方式: `EnqueueCallback`：ref 找不到时查订单 `ParentID` 再用父订单 ID 找 ref；只要 `CallbackStatus != pending` 就重置为 pending 并 `CallbackRetryCount=0` 再入队；`SendCallback` 与上游 `GetOrder` 的交付信息：自身 Fulfillment 为空时取第一个有 Fulfillment 的子订单，并带上 `delivery_data`（LogisticsJSON）；`enqueueOrderPaidAsync` 增加下游回调；`HandleUpstreamCallback` 交付后对本地订单及其父订单都 `EnqueueCallback`。列表查询 Preload Fulfillment。
- **对我们实现的要求**: 下游回调按"根订单"定位 ref；回调 payload 汇总子订单交付；每次状态变化都能重新触发（重置计数）；paid/delivered/canceled 均回调。
- **必测用例**: 父子订单子单交付 → 下游收到 order.fulfilled 且含子单 payload；回调曾 failed(retry=5) 后订单再变化 → 重新 pending、retry=0 并发送；A→B→C 三级，C 交付 → B 的本地单交付且 A 收到回调。

#### UPS-13 上游同步清空本地批发价、导入非法档位

- 提交: d137d288 2026-05-30
- 严重度: **中**
- 问题现象: SyncProduct 每次都 `localProduct.WholesalePrices = convertUpstreamWholesalePrices(...)`，上游不返回批发价（旧版上游）时把运营手动配置的批发价清空；上游档位换算后本地价 ≤0 或非单调时整组失败也写空。公开 API 直接输出 models 类型，unit_price 格式不统一。
- 根因: 同步逻辑无“缺省不覆盖”语义。
- 原修复方式: 仅当 len(upProduct.WholesalePrices)>0 且换算后非空才覆盖，否则保留并记日志；逐档跳过 min_quantity<=0 / unit_price<=0 / 换算后本地价<=0 的档位；needsProductUpdate 标志避免无谓更新。公开响应改用 WholesalePriceResp{min_quantity, unit_price:"80.00"} 并过滤无效档。后台列表支持 wholesale=true/false 过滤（sqlite json_array_length(COALESCE(col,'[]'))，pg jsonb_array_length(COALESCE(col::jsonb,'[]'::jsonb))）。
- **对我们实现的要求**: 上游同步对可选字段“未返回=不修改”；换算后校验并逐档过滤；公共 API 金额字符串固定两位小数。JSON 数组长度过滤需三方言（SQLite/MySQL JSON_LENGTH/PG）。
- **必测用例**: 本地已有 {5:80}，上游返回无 wholesale_prices → 同步后仍 {5:80}；上游档位换算后为 0 的档被跳过；wholesale=true 只返回有档位商品。

#### UPS-14 上游商品同步：SKU 增删、下架/删除识别、真实库存

- 提交: 8e2039d0 2026-05-06, 926d8768 2026-05-06; b2e55f9b 2026-03-06, f5a76f13 2026-03-06
- 严重度: **中**
- 合并条目: (1) 上游商品下架/删除的识别与同步 ｜ (2) 商品同步：SKU 增删、下架、真实库存
- 问题现象:
  - (1) 上游下架商品后，`GetProduct` 返回 404 `product_unavailable`，下游无法区分"下架"和"删除"；增量同步（updated_after）永远拿不到已删除商品，下游一直在售，造成超卖或采购失败；上游已下架的商品仍然可以被导入。
  - (2) ①上游删除 SKU 后本地 SKU 仍可售（只把映射 UpstreamIsActive=false）；②上游新增 SKU 不会同步；③上游下架商品本地仍在售；④库存只按 stock_status 伪造（in_stock→999、low→1），超卖上游；⑤导入商品图片/正文图片仍指向上游 URL（相对路径下载失败）；⑥slug 为空导致唯一冲突。
- 根因:
  - (1) 上游接口对下架商品直接返回错误，列表只返回上架商品，没有全量兜底。
  - (2) 同步只更新映射表，不回写本地商品/SKU。
- 原修复方式:
  - (1) 上游 `GetProduct` 对下架商品返回 200 + `is_active=false`；软删除的商品返回 404 `product_not_found`。`ListProducts` 支持 `include_inactive=true`，并在响应中回显 `includes_inactive`。下游新增 `ProductMapping.UpstreamStatus` 字段（active/inactive/deleted）。`markUpstreamUnavailable`：本地商品下架，所有 SKUMapping 置 `UpstreamIsActive=false, UpstreamStock=0`，本地 SKU 下架；状态为 deleted 时同时停用映射。每 24h（`upstream:last_full_sync:{conn}`，没有记录也算）强制一次全量同步；全量同步中缺失的商品，只有当上游回显 includes_inactive=true 时才判定为 deleted（兼容旧上游，避免误下架）。上游恢复在售时，mapping 状态恢复为 active，但**本地商品不自动上架**，由管理员决定。导入时如果上游 `!IsActive` 或已删除，返回 `ErrUpstreamProductNotFound`。
  - (2) `SyncProduct`：上游缺失的 SKU → 映射 stock=0/inactive 并停用本地 SKU；上游新增 SKU → 创建本地 SKU + 映射；已有 SKU 同步 spec_values/is_active/price/`stock_quantity`（-1 无限）；上游 `!IsActive` → 本地下架，但上游重新上架不自动上架（留给管理员）；同步 manual_form_schema；记录 `UpstreamFulfillmentType`。导入：`downloadContentImages` 正则提取 markdown/`<img>` 图片下载替换，DownloadImage 相对路径拼 baseURL；slug 缺省 `upstream-{conn}-{pid}-{ms}`；上游 API 暴露 `stock_quantity`（SKU 级手动库存，-1 无限；原来按商品级 ManualStockTotal 计算是错的）。
- **对我们实现的要求**:
  - (1) 按上述协议实现上下游接口，字段名保持一致以兼容。
  - (2) 同步是"本地 SKU 集合与上游对齐"的完整 diff（新增/更新/停用），库存使用上游真实数量；上游下架→本地下架是单向的；下载外部图片需限制大小/类型并防 SSRF（只允许连接 baseURL 同源）。
- **必测用例**:
  - (1) 上游返回 is_active=false → 本地商品和 SKU 下架，mapping 状态为 inactive；全量同步 + includes_inactive=true 且商品缺失 → deleted 且 mapping.is_active=false；旧上游（没有回显字段）缺失商品 → 只打日志、不下架；上游恢复在售 → mapping 恢复 active，本地商品仍是下架状态。
  - (2) 上游删 SKU#2 → 本地 SKU#2 is_active=false 且不可下单；上游新增 SKU#3 → 本地出现并建映射；上游 stock_quantity=3 → 本地下单 4 件被拒；上游下架→本地下架；上游重新上架→本地仍下架。

#### UPS-15 Bot/渠道目录对上游映射商品库存误报缺货

- 提交: ab56fb0e 2026-04-15
- 严重度: **中**
- 问题现象: Telegram Bot 渠道 API（`channel_catalog.go` GetProducts/GetProductDetail）中，fulfillment_type=upstream 的映射商品全部显示缺货。原因是只解析了展示用的交付类型，没有把上游 SKU 库存写回本地库存字段，auto 类型的 `AutoStockAvailable=0`。
- 根因: Web 端有 `decorateUpstreamStock`，渠道端漏了这一步；另外 auto 类型库存为 -1（无限）时，`computeStockStatus` 判成缺货。
- 原修复方式: 新增 `applyUpstreamMappings`（必须在 `ApplyAutoStockCounts` 之后调用）：批量读取 ProductMapping 和 SKUMapping，按展示类型把 `UpstreamStock` 写到 `AutoStockAvailable` 或 `ManualStockTotal`。未激活的 SKU 映射写 0；任一 SKU 库存无限则商品写 -1；否则写各 SKU 之和；所有 SKU 都未激活则写 0。降级规则：没有 mapping 记录，或批量查询出错 → 视为无限库存（不误报缺货）。`computeStockStatus/Count`：auto 与 manual 统一按"<0 表示无限、>0 表示有货"判断。
- **对我们实现的要求**: Web、渠道 API、上游 API 共用同一个"商品库存装饰"函数，不要各写一份。库存 -1 表示无限的语义在所有交付类型下保持一致。
- **必测用例**: 映射商品有 2 个 SKU，上游库存分别为 5 和 -1 → 商品有货，库存为 -1；SKU 映射全部未激活 → 缺货；auto 商品库存为 -1 → in_stock。

#### UPS-16 上游退款状态同步和对账一致性映射

- 提交: 18e10d7c 2026-04-13
- 严重度: **中**
- 问题现象: 上游退款后，下游采购单仍显示已完成，对账被误报为不一致；上游返回 `fulfilled` 或 `cancelled`（双 l）时状态没识别。
- 根因: 状态映射没有覆盖同义词和退款态。
- 原修复方式: `mapProcurementUpstreamStatus`：先转小写并去空白；`delivered|completed|fulfilled`→delivered，`canceled|cancelled`→canceled，`refunded|partially_refunded` 保持原值。`HandleUpstreamCallback` 遇到 refunded/partially_refunded 时，只更新采购单状态（ProcurementStatusRefunded/PartiallyRefunded），**不改本地订单状态**（测试：发货中收到 partially_refunded 回调时订单状态保持不变，已完成后收到 refunded 也保持不变）。`isStatusConsistent`：本地 completed/fulfilled 对应上游 delivered/completed/fulfilled/refunded/partially_refunded 都算一致；本地 canceled 对应 canceled/cancelled/refunded/partially_refunded 算一致。上游 GetOrder 接口额外返回 `refunded_amount` 和 `refund_records`，采购单视角的退款状态优先。
- **对我们实现的要求**: 上游状态统一走一个 normalize 函数；上游退款只影响采购单，本地订单的退款由管理员决定。
- **必测用例**: 上游回调 status="Cancelled " 时映射为 canceled；本地 completed + 上游 partially_refunded 时对账结果一致；fulfilling 状态收到 refunded 回调时，本地订单状态不变、采购单状态变为 refunded。

#### UPS-17 上游同步任务防重叠 + 按连接批量拉取 + 增量同步

- 提交: 75303b84 2026-03-29, c8582869 2026-03-29, d898a22e 2026-04-02, a36f3f6d 2026-04-02
- 严重度: **中**
- 问题现象: 周期任务每 1 分钟执行 `SyncAllStock`，对每个映射串行发 HTTP。映射多时一轮超过 1 分钟，任务重叠执行，重复请求把上游打爆，并发写同一个 SKU 映射。
- 根因: 周期任务没有互斥，也没有批量接口。
- 原修复方式: 用 Redis `SetNX("upstream:sync_stock_running", 30m)` 加互斥锁，Redis 不可用时降级为直接执行，结束时 defer Del。按 connection 分组，每个连接调用分页 `ListProducts`（page_size 50，最多 200 页），在内存中匹配映射；连接之间并发度为 3。支持增量：记录 `upstream:last_sync:{conn}`，下次请求带上 `updated_after = last-1min`，增量失败时回退到全量。上游 handler 支持 `updated_after`（RFC3339），page_size 上限为 50。同步间隔可以配置（默认 5m）。全量模式下上游缺失的商品记 warn 日志。
- **对我们实现的要求**: 周期同步必须有分布式互斥锁（锁值用随机 token，释放时比对 token，避免误删别的实例的锁，TTL 大于最长执行时间），也要支持单机运行。增量同步的前提是上游在库存或价格变化时会刷新 products.updated_at，否则卡密入库这类变化会被增量同步漏掉，所以需要定期做全量。分页循环要有终止条件（total 已拉满、空页、页数上限）。
- **必测用例**: ①两个 worker 同时触发，只有一个执行；②增量请求报错后自动回退全量并完成同步；③上游返回 total=120 时拉取 3 页；④上游某商品缺失时记日志，不崩溃。

#### UPS-18 下游 API 凭证申请：待审核空 api_key 撞唯一索引、软删后重新申请、密钥字段长度

- 提交: ae87b206 2026-04-01, 17442b93 2026-04-01, 2f8dae2f 2026-03-30; 62094966 2026-03-22
- 严重度: **中**
- 合并条目: (1) API 凭证申请：待审核记录 api_key 为空导致唯一索引冲突；密钥字段长度不足 ｜ (2) 下游 API 申请：软删除后重新申请与凭证重置
- 问题现象:
  - (1) api_credentials.api_key 上有唯一索引。用户申请时创建的是 `ApiKey=""` 的 pending 记录，管理员还没审核时第二个用户申请，也插入空串，唯一索引冲突，申请失败。被拒绝或删除后重新申请时 reset 把 ApiKey 置空，也会冲突。另外 site_connections.api_secret（AES 加密后 base64）用 varchar(256)，api_credentials.api_secret 用 varchar(128)，加密后的长度会超出，在 MySQL/PG 上写入失败或被截断（截断后无法解密）。
  - (2) (1) 管理员删除（软删）某用户的 API 凭证后，用户重新申请 → `GetByUserID` 查不到（已软删）→ Create 撞 user_id 唯一索引失败；(2) 被拒绝后重新申请时仅改 status，旧 api_key/api_secret/approved_at/is_active 残留。
- 根因:
  - (1) 用空串做唯一列的占位值；加密后密文膨胀没有预留长度。
  - (2) 软删除 + 唯一约束；重申请未清理旧凭证。
- 原修复方式:
  - (1) Apply 以及 `resetApiCredentialForReapply` 都用 `generateRandomHex(32)` 生成新的 api_key（secret 仍然清空，状态为 pending_review，is_active=false）。api_secret 字段分别扩容到 varchar(512) 和 varchar(256)。
  - (2) `GetAnyByUserID`（Unscoped）；已软删 → `resetApiCredentialForReapply`（api_key/secret 置空、status=pending_review、reject_reason 清空、approved_at/last_used_at=nil、is_active=false、deleted_at 清空）后 `UpdateAny`（Unscoped Save）；rejected 同样重置；pending → ErrApiCredentialPendingExist；approved → ErrApiCredentialExists。
- **对我们实现的要求**:
  - (1) 唯一列不能用空串作占位：要么生成随机值，要么让列可为 NULL（三种数据库对 NULL 都不做唯一约束）。鉴权时必须同时要求 status=approved && is_active，只有 key 存在不够。加密后存储的密文字段使用 TEXT 或足够长度（按 nonce+tag+base64 膨胀计算）。
  - (2) 带唯一约束的软删除表，"重新创建"必须改为恢复旧行；重申请必须作废旧 key/secret，审批通过时重新生成；API 鉴权查询排除 deleted/非 active/非 approved。
- **必测用例**:
  - (1) ①用户 A、B 先后申请且都未审核，两次都成功，api_key 不同且非空；②被拒绝后重新申请得到新 api_key，旧 key 失效；③pending 状态的 key 调用上游 API 返回 401；④长度 200 的 secret 加密后在 MySQL 中存取一致。
  - (2) 删除后重新申请 → 同一行恢复、status=pending、api_key 为空；旧 api_key 调用下游接口 → 401；pending 状态再次申请 → 409；rejected → 可重申请且字段已重置。

#### UPS-19 多级对接：映射商品按上游“真实交付类型”展示与计算库存，前台隐藏 upstream 类型

- 提交: 8f85d08b 2026-03-26, 6bbb0b38 2026-03-25; 6afb43fc 2026-03-06, b7bfd7fb 2026-03-06, a0b632d7 2026-03-06, f5a76f13 2026-03-06, 223da79c 2026-03-06, b2e55f9b 2026-03-06（表单）
- 严重度: **中**
- 合并条目: (1) 多级对接：中间站点对下游暴露"真实交付类型"并据此计算库存 ｜ (2) 前台隐藏 upstream 交付类型并按上游原始类型展示库存
- 问题现象:
  - (1) A(源)→B(中间)→C。B 上的映射商品 fulfillment_type=upstream，B 的上游 API/渠道目录直接返回 "upstream"；C 导入时把非 auto 一律当 manual，且 B 用 upstream 类型计算 stock_status/stock_count → C 看到的库存和类型都错（auto 卡密商品变人工、库存显示缺货）。前端导入页只把 `in_stock` 当有货，`low_stock`/`unlimited` 被显示为缺货。
  - (2) 前台商品/购物车/订单列表/游客订单接口直接返回 `fulfillment_type="upstream"`，暴露"代销/对接上游"事实；前端对 upstream 类型不识别库存，可无限加购；上游映射商品在前台库存字段为空导致误判售罄或不限购；upstream 商品有人工表单时下单没有校验/快照表单。
- 根因:
  - (1) 对外输出未解析映射的 upstream_fulfillment_type。
  - (2) 内部交付类型直接透出；库存字段未按展示类型填充。
- 原修复方式:
  - (1) 上游 handler（`ListProducts/GetProduct`）和 channel catalog（列表/详情）新增 `resolveEffectiveFulfillmentTypes`：`is_mapped && type==upstream` 时批量查 ProductMapping，取 `UpstreamFulfillmentType`（非 auto 归为 manual），用于返回的 fulfillment_type 和库存计算。前端 `isSkuAvailable = in_stock|low_stock|unlimited`。
  - (2) `Order.MaskUpstreamFulfillmentType()`（递归 Items、Fulfillment、Children）在所有前台订单接口（创建/列表/详情/按单号/取消/游客）调用；购物车 upstream→manual；`decorateUpstreamStock` 用 `mapping.UpstreamFulfillmentType`（非 auto 一律 manual）作为展示类型，并把 SKU 映射库存写入 auto_stock_available 或 manual_stock_total（-1 无限），无映射时降级为有货 manual，无活跃映射 → 售罄；后台列表 `applyUpstreamDisplayTypes` 同理；前端 auto 类型 `auto_stock_available<0` 视为不限；`buildOrderResult` 对 upstream 且有 schema 的商品也校验并快照人工表单。
- **对我们实现的要求**:
  - (1) 所有对外（下游 API、Bot 渠道、前台）输出的交付类型都用 effective 类型；库存计算用 effective 类型；前端有货判断包含 low_stock、unlimited。
  - (2) 对外序列化层统一做交付类型脱敏（upstream→上游原始 auto/manual），不能靠各 handler 手工调用漏掉；库存字段按展示类型填充同一语义（-1 无限）。
- **必测用例**:
  - (1) B 映射了 A 的 auto 商品 → B 的 `/upstream/products` 返回 fulfillment_type=auto，stock_count 为同步的上游库存；C 导入后 mapping.upstream_fulfillment_type=auto。
  - (2) 上游映射商品（上游 auto，库存 5）→ 前台 fulfillment_type=auto、auto_stock_available=5；订单详情/列表/游客查单 JSON 中不出现 "upstream" 字符串；upstream 商品带必填表单缺字段下单 → 400。

#### UPS-20 导入上游商品必须单事务：商品+SKU+映射+SKU映射

- 提交: f3b5b75b 2026-03-24
- 严重度: **中**
- 问题现象: `ImportUpstreamProduct` 在事务内创建本地商品和 SKU，事务提交后再创建 ProductMapping 和 SKUMapping；后者失败时留下"已上架但无映射"的本地商品（fulfillment_type=upstream），下单后无法采购。
- 根因: 事务边界不完整；mapping repo 没有 WithTx。
- 原修复方式: 给 ProductMappingRepository/SKUMappingRepository 增加 `WithTx`；把 mapping、`createSKUMappingsWithRepo` 移入同一事务，使用事务内创建的 localSKUs 列表建立映射。
- **对我们实现的要求**: sea-orm 中导入流程全部使用同一个 `DatabaseTransaction`；任何一步失败整体回滚。
- **必测用例**: 注入 SKU 映射插入失败 → products/product_skus/product_mappings 三表均无新增行。

#### UPS-21 对账金额比较口径错误与统计口径

- 提交: e1b3fd8b 2026-03-06, 3e7de050 2026-03-06
- 严重度: **中**
- 问题现象: 第一版把本地售价 `LocalSellAmount` 与采购价 `UpstreamAmount` 比较，只要有利润就判定"金额不一致"，对账全部报差异；`TotalCount` 把没有上游单号、上游查询失败的也算进去，导致 matched 数虚高；已完成的对账任务可被重复执行；本地 `fulfilling` 与上游 `processing/paid` 被判不一致。
- 根因: 比较口径混淆；统计未区分跳过/错误。
- 原修复方式: `UpstreamOrderDetail` 增加 `amount/currency`；金额比较改为 `po.UpstreamAmount`（本地记录的采购价）vs 上游返回的 `detail.Amount`（两者都为正时才比较），明细 LocalAmount=UpstreamAmount、UpstreamAmount=上游实际金额；`TotalCount=len-skipped-errors`，结果 JSON 带 skipped/errors；`Execute` 对 completed 任务直接返回；`isStatusConsistent` 增加 fulfilling ↔ fulfilling/processing/paid。
- **对我们实现的要求**: 对账只比较"本地记录的采购成本"与"上游实际收取金额"，永远不要拿售价比；统计区分 compared/skipped/errors；任务执行需状态机（running/completed 不重入，用条件 UPDATE 抢占）。
- **必测用例**: 售价 12、采购价 10、上游返回 10 → 无差异；上游返回 11 → amount_mismatch；无上游单号 3 条 + 查询失败 2 条 + 正常 5 条 → total=5, skipped=3, errors=2；对 completed 任务再次执行 → 不变。

#### UPS-22 列表状态统计必须基于全量筛选结果；时间筛选要解析成时间类型

- 提交: e48521f0/19263e7c 2026-04-26, b57cc89d/907a546f 2026-04-27
- 严重度: **低**
- 问题现象: ①采购单页面的状态计数只统计当前页数据，与实际总数不符；②`created_from/created_to` 作为原始字符串传入 SQL `created_at >= ?`，SQLite 中按字符串比较，格式或时区不一致时结果错误；前端只传日期 `2026-04-27`，created_to 会漏掉当天的数据。
- 根因: 统计在前端做；时间参数没有类型化。
- 原修复方式: 新增 `GET /admin/procurement-orders/stats`：复用列表的筛选条件，但**不应用 status 筛选**，按 `status, COUNT(*)` 分组，返回 `{total, by_status}`。时间参数用 `shared.ParseTimeNullable`（RFC3339）解析为 `*time.Time`，解析失败返回 400；前端改用 datetime-local，经 `toRFC3339` 转换后带时区发送。
- **对我们实现的要求**: Rust 列表接口的时间范围参数统一反序列化为 `DateTime<Utc>`，解析失败返回 400；统计接口在服务端聚合，并忽略自身的维度筛选。
- **必测用例**: created_to=2026-04-27T23:59:59+08:00 时包含当天 23:00 的记录；非法时间字符串返回 400；status=failed 时统计接口仍返回所有状态的计数。

## 12. 用户认证/2FA/JWT/OAuth（Telegram/Google）（AUTH，10 条）


#### AUTH-01 Google 登录：ID Token 严格校验 + 仅权威邮箱自动关联 + redirect state 一次性消费

- 提交: 4e580883 2026-07-30
- 严重度: **高**
- 问题现象: （新功能，必须保留其中的安全规则）Google 登录若只校验签名，易被 aud 混淆、未验证邮箱、非权威域邮箱接管已有本地账号等攻击。
- 根因: —（新增功能的安全设计）
- 原修复方式: googleauth `VerifyCredential`：credential 长度上限；JWT 仅允许 RS256，kid 必填，从 Google JWKS（按 Cache-Control 缓存，未知 kid 强制刷新一次）取 key；`aud` 必须恰好一个且等于 ClientID，`azp` 若存在必须等于 ClientID；iss ∈ {accounts.google.com, https://accounts.google.com}；exp 必须有，iat 必须有且不在未来（允许时钟偏差）；sub 非空限长；email 小写且 `mail.ParseAddress` 后完全相等；`email_verified=true` 否则 ErrGoogleEmailUnverified；`EmailAuthoritative = 邮箱以 @gmail.com 结尾 || hd 非空`。登录：已有同邮箱本地账号时只有权威邮箱才允许自动关联（否则 ErrGoogleAutoLinkForbidden，需先登录后手动绑定），账号需 active；新建账号需注册开放且通过邮箱域名白名单；同一用户已绑定其他 Google sub → ErrUserOAuthAlreadyBound；锁顺序 user → identity，唯一键冲突重试一次。Redirect 流程：state 为 32 字节随机 base64url，Redis 存 intent（flow、绑定用户、租户）并在验证凭证前原子 Take（一次性）；验证后生成 2 分钟一次性 handoff，交换时校验租户一致、绑定用户一致、ClientID 未变更。解绑时必须保留至少一种可用登录方式（本地密码或其他可用第三方身份）。登录走原有 JWT/2FA 流程。
- **对我们实现的要求**: Rust 中用 jsonwebtoken + JWKS 缓存实现上述全部校验；完整实现自动关联/创建规则、state/handoff 一次性消费、解绑保留登录方式规则。
- **必测用例**: aud 为其他 client → 拒绝；email_verified=false → 拒绝；自定义域名（无 hd）邮箱与现有本地账号同邮箱 → auto_link_forbidden；gmail 同邮箱 → 关联成功；state 重放第二次 → 失败；仅剩 Google 身份时解绑 → 拒绝。

#### AUTH-02 Telegram OIDC 登录：身份 ID 口径与安全校验

- 提交: 9dadd18f 2026-05-12, 6b67ef97 2026-05-12, 5d0ce38c 2026-05-12, b1e527fb 2026-05-14, 42315601 2026-05-18, 92ac12d1 2026-05-18
- 严重度: **高**
- 问题现象: 初版 OIDC 用 claims.sub 作为 provider_user_id，而旧 Widget/MiniApp 登录用 Telegram 数字 id → 同一 Telegram 账号被识别为新用户（重复账号/无法登录原账号/绑定冲突）。
- 根因: OIDC sub 与 Telegram 数字 ID 不同。
- 原修复方式: telegramOIDCProviderUserID 只接受 claims.id（int64>0）作为 ProviderUserID，sub 若不同放入 ProviderUserIDAliases；查询先按数字 ID，未命中再按别名；命中别名时 canonicalizeTelegramProviderUserID 把记录迁移为数字 ID（若数字 ID 已被其他身份占用 → ErrUserOAuthIdentityExists）；绑定比对也接受别名。OIDC 安全基线：state 32B 随机，存 Redis `telegram:oidc:state:` TTL 600s 且 SetNX，取出即删（一次性）；PKCE S256；token 请求 Basic(client_id=bot id:client_secret)；id_token 仅 RS256、iss=https://oauth.telegram.org、aud=client_id、必须有 exp；JWKS 缓存 10 分钟、kid 未知时刷新、body 限 1MB；id_token 指纹做重放标记；state 中存 intent(login/bind)+userID，login 回调 intent 必须 login，bind 回调 intent 必须 bind 且 userID 等于当前登录用户。后台可解绑用户 Telegram（DELETE /admin/users/:id/oauth/telegram），用户无真实邮箱时拒绝（ErrTelegramUnbindRequiresEmail）防止账号无法登录。
- **对我们实现的要求**: Rust 中 Telegram 身份统一以数字 ID 为主键，兼容 sub 别名并迁移；完整实现上述 OIDC 校验；bind 流程必须校验 state 中的 userID；解绑需保证用户仍有可用登录方式；需要 2FA 的用户返回 challenge 而非 token。
- **必测用例**: 历史绑定 provider_user_id=sub 的用户 OIDC 登录 → 登录同一用户且记录被改为数字 id；state 重放第二次 → state_invalid；login state 用于 bind 回调 → 拒绝；A 用户发起的 bind state 被 B 使用 → 拒绝；aud 不匹配/过期 → id_token_invalid。

#### AUTH-03 2FA：挑战 token 与访问 token 隔离（typ），挑战 jti 一次性、失败计数、恢复码、覆盖所有登录入口

- 提交: 75cb5384 2026-04-27, 5423090b 2026-04-28; 75cb5384 2026-04-27, 5423090b 2026-04-28, 164d06bd 2026-04-28
- 严重度: **高**
- 合并条目: (1) 2FA 挑战 token 必须不能当访问 token 用（typ 声明隔离） ｜ (2) 2FA 登录流程的完整约束（挑战 jti 撤销、失败次数、恢复码、所有登录入口）
- 问题现象:
  - (1) 75cb5384 首版管理员 2FA 中，`IssueChallengeToken` 用**与正式 JWT 相同的 `JWT.SecretKey`**、HS256 签发 `ChallengeClaims{admin_id, jti, purpose:"2fa_challenge"}`。`JWTAuthMiddleware` 用 `JWTClaims{admin_id, username, token_version}` 解析，挑战 token 同样验签通过，且缺省 `token_version=0` 与新管理员 `TokenVersion=0` 一致 → 只输入密码拿到的挑战 token 可直接调用后台接口，2FA 形同虚设。
  - (2) 新增 2FA 时要堵住多个绕过点：挑战 token 在 5 分钟内可以被反复暴力猜 6 位码；验证成功后同一个挑战 token 还能再用；Telegram 登录和 Mini App 登录如果直接发 JWT 就会绕过 2FA；开启或关闭 2FA 后旧设备的 token 仍然有效。
- 根因:
  - (1) 两类 token 共用密钥和同名 claim（admin_id / user_id），中间件没有区分 token 用途。
  - (2) 登录被拆成两步后，中间态需要有状态的约束。
- 原修复方式:
  - (1) 5423090b 增加 `typ` claim：`TokenTypAccess="access"`、`TokenTyp2FAChallenge="2fa_challenge"`；`GenerateJWT` / `GenerateUserJWT` 写入 `typ=access`，挑战 token 写入 `typ=2fa_challenge`；`JWTAuthMiddleware` 和 `UserJWTAuthMiddleware` 调用 `IsAccessTokenTyp(typ)`（为兼容旧 token，允许空串或 "access"），其他值一律 401；`ParseChallengeToken` 同时要求 `purpose` 和 `typ` 都是挑战类型。测试见 `middleware_2fa_typ_test.go`。
  - (2) 登录第一步通过后，如果 `TOTPEnabledAt != nil`，只返回 `{requires_totp:true, challenge_token, challenge_expires_at}`（TTL 5 分钟，带 jti）。验证时先查 Redis `2fa:challenge:{jti}:revoked`；每次失败 INCR `...:fails`，达到 5 次就 revoke 并返回 `too_many_attempts`；成功后立即 revoke jti（一次性），再 `CompleteLoginAfter2FA` 签发正式 JWT。`LoginWithTelegram`、`LoginWithTelegramMiniApp` 同样返回挑战。TOTP 参数为 period 30、digits 6、skew 1。secret 用 AES-GCM 加密存储（密钥由 `app.secret_key` 派生，所以 bce8a828 在 config 示例里补了 `app.secret_key`）。绑定分两步：`setup` 生成 pending secret（10 分钟过期），`enable` 校验首个码，失败 5 次清空 pending。恢复码 10 个，格式 `xxxx-xxxx`，bcrypt 哈希存储，每个只能用一次（记录 used_at）。登录验证用恢复码会消耗一个；重新生成恢复码必须提供 TOTP 码，不接受恢复码。Disable、AdminReset 执行 `ClearTOTP` 并 bump TokenVersion，同时删除 Redis 鉴权缓存（`DelAdminAuthState` / `DelUserAuthState`）。用户 enable 后也 bump TokenVersion 并给当前会话换发新 JWT。超管不能用 reset 接口重置自己的 2FA（`ErrTOTPCannotResetSelf`）。管理员帮用户移除 2FA 的接口是 `DELETE /admin/users/:id/2fa`，要求 operatorID 非 0，并写审计日志（operator、target、IP、UA、request_id）。登录日志记录 `password_ok_2fa_pending`、`invalid_totp_code` 等事件。
- **对我们实现的要求**:
  - (1) Rust 所有 JWT claims 都带 `typ` 字段（access / 2fa_challenge / 其他用途）。鉴权 extractor 只接受 typ 为 access 的 token，老 token 缺 typ 时是否放行要明确规定（新项目建议必须带 typ）。最好让挑战 token 用单独派生的密钥（例如 HKDF(secret,"2fa")），双重隔离。管理员和用户 token 也不能互相通用（用不同 secret 或加 aud）。
  - (2) 以上逻辑在 Rust 中逐条实现。Redis 不可用时（原实现直接跳过计数）我们应退回 DB 或内存计数，不能无限放行。其他登录方式（OAuth、Telegram、邮箱验证码登录）必须经过同一个"是否需要 2FA"的出口函数。TOTP 验证建议记录 last_used_step 防止同一个码重放（原实现未做）。
- **必测用例**:
  - (1) 1) 用挑战 token 调 `GET /admin/xxx` 和 `/api/v1/user/me`，都返回 401；2) typ="refresh" 的 token 返回 401；3) 正常 access token 返回 200；4) 用 access token 调 `/2fa/verify` 当挑战 token 用，被拒绝。
  - (2) 1) 同一 jti 输错 5 次后，第 6 次即使码正确也返回失败；2) 验证成功后用同一个 challenge 再验一次，返回 401；3) 恢复码用过一次后再用，返回失败；4) 开启 2FA 的账号用 Telegram 登录，返回 requires_totp=true，没有 token；5) 用户 disable 或管理员 reset 后，旧 access token 返回 401；6) 挑战 token 过期（超过 5 分钟）后被拒绝。

#### AUTH-04 Telegram Mini App：initData 严格校验（签名/时效/重放），前端先 ready() 且脚本加载超时

- 提交: 4335ff62 2026-03-22, c3e8ab7a 2026-03-22, 62e85152 2026-03-22; 434b84c2 2026-04-04, 2bec6cb1 2026-04-01
- 严重度: **高**（高；低）
- 合并条目: (1) Telegram Mini App initData 校验 ｜ (2) Telegram Mini App 自动登录：必须先调用 ready()，脚本加载要有超时
- 问题现象:
  - (1) 新增 Mini App 登录/绑定（`/auth/telegram/miniapp/login`、`/me/telegram/miniapp/bind`），需正确验证 initData，防伪造与重放。
  - (2) 在部分 Telegram 客户端上，调用 `WebApp.ready()` 之前 initData 为空。原代码判断 `!isMiniApp`（依据 initData）就提前 return，结果永远不调用 ready()，initData 一直为空，自动登录失败。telegram-web-app.js 加载卡住时 Promise 永不结束，页面一直等待；加载失败后 promise 被缓存，之后无法重试。普通浏览器中也显示了 miniapp 登录入口。
- 根因:
  - (1) Mini App 签名算法与 Login Widget 不同。
  - (2) 初始化顺序错误；Promise 没有超时和重置。
- 原修复方式:
  - (1) `VerifyMiniAppInitData`：url.ParseQuery；hash/auth_date/user 必填，user.id>0；data_check_string = 除 `hash` 外所有字段（**包括新的 `signature` 字段**）按 key 排序 `k=v` 用 `\n` 连接；secret = HMAC_SHA256(key="WebAppData", msg=bot_token)；hash = hex(HMAC_SHA256(secret, dcs))，`hmac.Equal` 比较；时间校验：auth_date 超前 >1 分钟拒绝，超过 LoginExpireSeconds 过期；重放：Redis SetNX `telegram:auth:replay:{uid}:{hash}` TTL=ReplayTTLSeconds，已存在→ErrTelegramAuthReplay。绑定时若该 TG 已被其他用户占用则拒绝。前端：miniapp 登录路径加入"认证接口"正则，避免 401 触发全局登出跳转。
  - (2) 只要 webApp 存在就调用 `ready()` 和 `expand()`。脚本加载设置 3s 超时，超时或 onerror 时把 `scriptLoadPromise` 置 null 以便重试（settled 标记防止重复 resolve）。普通浏览器隐藏 miniapp 入口。
- **对我们实现的要求**:
  - (1) Rust 实现严格按上面算法（常量时间比较），不要把 `signature` 从校验串里剔除；重放键用 SET NX EX；复用注册开关检查。
  - (2) 前端按"加载脚本（带超时）→ ready() → 读 initData → 调用后端 miniapp/login"的顺序执行。后端必须用 bot token 校验 initData 的 HMAC 和 auth_date 时效（这部分不在本切片，但不能省）。
- **必测用例**:
  - (1) 官方样例 initData 验签通过；改动 user 字段任意字符 → 签名无效；auth_date 为 2 小时前（过期 1 小时）→ expired；auth_date 未来 2 分钟 → invalid；同一 initData 提交两次 → 第二次 replay；含 signature 字段的 initData → 通过。
  - (2) ①模拟 ready() 之后才填充 initData，能成功登录；②脚本 3s 未加载，reject 后再次调用会重新加载；③非 Telegram 环境不显示入口。

#### AUTH-05 注册开关/邮箱验证开关必须覆盖所有入口（含 Telegram 自动注册、发验证码、找回密码）

- 提交: a0441439 2026-03-20, cf93f5ca 2026-03-20, c0399d81 2026-03-20, 803f11a1 2026-03-24; 90f81b9b 2026-04-01, 8b4a783a 2026-04-01, 244bd7db 2026-04-01
- 严重度: **高**（高；中）
- 合并条目: (1) 注册开关必须覆盖所有注册入口（含 Telegram 自动注册），邮箱验证开关 ｜ (2) 邮箱验证关闭时，服务端也要拒绝发验证码和找回密码；订单状态邮件单独开关
- 问题现象:
  - (1) 后台关闭注册后，邮箱注册接口被拦住，但 Telegram 登录（Login Widget 与 Mini App）遇到未绑定的 TG 账号仍会 `findOrCreateTelegramUser` 自动创建新用户 → 绕过"关闭注册"。
  - (2) 管理员关闭邮箱验证（通常是 SMTP 不可用）后，`/auth/send-verify-code` 仍然会尝试发邮件（可能被刷接口，也会报错），`/auth/forgot-password` 仍可调用。关闭验证后，重置密码本来依赖的"邮箱所有权证明"就不存在了。另外，订单状态邮件无法单独关闭，只能整体关闭 SMTP。
- 根因:
  - (1) 注册开关只在 `UserRegister` / 发送注册验证码 handler 中检查。
  - (2) 开关只在前端隐藏了入口，后端没有检查。
- 原修复方式:
  - (1) 设置 key `registration_config` {registration_enabled 默认 true, email_verification_enabled 默认 true}（normalize 保证布尔）；`SendUserVerifyCode` purpose=register 时关闭即 403 `error.registration_disabled`；`UserRegister` 检查开关，email_verification_enabled=false 时 code 可空、跳过 verifyCode；`findOrCreateTelegramUser` 在"已有身份→直接登录"之后、创建新用户之前检查开关，关闭返回 `ErrRegistrationDisabled`，handler 映射 403 并记登录失败日志。已绑定用户在关闭注册时仍可 TG 登录。
  - (2) `SendUserVerifyCode` 开头检查 `GetEmailVerificationEnabled`，关闭时返回 403 `error.email_verification_disabled`；`UserForgotPassword` 在关闭时返回 403 `error.password_reset_disabled`（提示联系管理员）。SMTPSetting 新增 `order_notification_enabled`（默认 true，支持 patch 和 mask 输出），`enqueueOrderStatusEmailTaskIfEligible` 在它为 false 时不入队。public config 下发 `smtp_enabled`。前端 Forgot 页和 Login 页根据配置隐藏入口。
- **对我们实现的要求**:
  - (1) 在 service 层提供 `ensure_registration_allowed()`，所有"会新建 user 行"的路径（邮箱注册、OAuth/TG/MiniApp 首登、渠道 bot 自动建号等）统一调用；已存在身份的登录不受影响。邮箱验证开关只影响注册（不影响找回密码）。
  - (2) 所有功能开关必须在后端 handler 或 service 中强制检查。邮箱验证关闭时禁止验证码发送和基于邮箱验证码的找回密码（管理员可以在后台重置）。订单通知邮件入队前检查 smtp.enabled && order_notification_enabled。新增设置字段在反序列化旧数据时默认 true。
- **必测用例**:
  - (1) 关闭注册 → 邮箱注册 403、发注册验证码 403、TG 新用户登录 403 且 users 表行数不变；已绑定 TG 的用户登录成功；关闭邮箱验证 → 不带 code 注册成功；开启时缺 code → 失败。
  - (2) ①关闭邮箱验证后 POST send-verify-code（purpose=register/reset）返回 403；②关闭后 forgot-password 返回 403；③order_notification_enabled=false 时订单 paid 或 delivered 不产生邮件任务；④旧的 SMTP 配置 JSON 中没有该字段时视为 true。

#### AUTH-06 注册与换绑邮箱拒绝 Telegram 占位邮箱

- 提交: 393079aa 2026-09-15
- 严重度: **中**
- 问题现象: Telegram 登录用户由系统生成占位邮箱 `telegram_<id>@login.local`。用户注册或换绑新邮箱时可以填写 `telegram_12345@login.local`，从而与某个 Telegram 用户的占位邮箱冲突/抢注，或冒充该身份（后续 Telegram 登录按占位邮箱匹配可能绑定到攻击者账号）。
- 根因: 用户输入入口复用了通用 `normalizeEmail`，未排除系统保留格式。
- 原修复方式: userauth/application/service.go 新增 `normalizeUserSuppliedEmail`：normalize 后若 `telegramidentity.IsPlaceholderEmail`（小写后前缀 `telegram_` 且后缀 `@login.local`）返回 `ErrInvalidEmail`；用于 `Register`、`SendChangeEmailCode(kind=new)`、`ChangeEmail`。
- **对我们实现的要求**: 所有用户自填邮箱入口（注册、发送注册验证码、换绑发码、换绑确认，以及游客下单邮箱建议同样处理）调用统一的 `normalize_user_supplied_email`，拒绝保留域名 `@login.local` 的 telegram_ 占位格式（大小写不敏感）。
- **必测用例**: 注册 `Telegram_1@LOGIN.local` → email_invalid；换绑新邮箱 `telegram_abc@login.local` 发码 → email_invalid；普通邮箱正常。

#### AUTH-07 注册邮箱域名白名单

- 提交: 26d87ffe 2026-06-01, c074978b 2026-06-01, d279a2bf 2026-06-01
- 严重度: **中**
- 问题现象: （新增风控）需只允许指定邮箱域名注册；必须在发送验证码前拦截，否则仍可被用来发信轰炸。
- 根因: —
- 原修复方式: registration_config 新增 email_domain_allowlist_enabled、allowed_email_domains；归一化：按逗号/空白拆分、小写、去前导 @、校验标签（a-z0-9-，不以 - 开头结尾，≤63，必须含点，无 ..），去重，最多 100 个、每个 ≤253；CheckRegistrationEmailDomainAllowed 用 net/mail 严格解析，域名精确匹配（子域名不算）；SendVerifyCode(purpose=register) 与 Register 都校验 → ErrEmailDomainNotAllowed（400 error.email_domain_not_allowed）；Telegram 自动创建用户不受限制；公开配置暴露 enabled 与域名列表。
- **对我们实现的要求**: 同样在发送注册验证码之前校验；精确域名匹配；设置写入时归一化。
- **必测用例**: 白名单 [gmail.com]：a@gmail.com 允许；a@mail.gmail.com 拒绝；a@GMAIL.COM 允许；发送注册验证码到 a@qq.com → 拒绝且未发邮件；Telegram 新用户不受影响。

#### AUTH-08 TOTP 启用流程校验顺序

- 提交: 33e707dc 2026-06-01
- 严重度: **中**
- 问题现象: （重构统一管理员与用户 2FA 启用逻辑）需保证过期的 pending secret 在任何副作用前拒绝，错误验证码计数。
- 根因: 两份重复实现易漂移。
- 原修复方式: enableTOTPFor：账号不存在→NotFound；已启用→ErrTOTPAlreadyEnabled；pending_secret 为空或已过期→ErrTOTPPendingExpired（在失败计数检查、解密前）；checkEnableFailures 超限拒绝；验证失败 bumpEnableFailures 并返回 ErrTOTPCodeInvalid；成功后重新加密 secret、生成恢复码、写 enabled_at，最后清失败计数。
- **对我们实现的要求**: 管理员与用户共用一个 TOTP 启用函数，保持上述顺序。
- **必测用例**: pending 过期 → expired 且失败计数不变；错码 → 计数+1；达上限 → 拒绝且不校验验证码；成功 → 返回恢复码并清计数。

#### AUTH-09 登录接口的用户枚举时序防护：dummy bcrypt 必须是合法哈希

- 提交: 4a4abb1c 2026-03-31
- 严重度: **中**
- 问题现象: 管理员或用户登录时，账号不存在的请求立即返回，存在的要跑 bcrypt（约 50~100ms），通过响应时间可以枚举邮箱或用户名。修复代码在 user==nil 时执行 `bcrypt.CompareHashAndPassword("$2a$10$dummyhashtopreventtimingattacksxxxx...")`，但这个字符串长度和格式都不是合法的 bcrypt 哈希，Go 的 bcrypt 很可能在解析阶段就报错返回，没有真正做 cost=10 的计算，防护可能无效。
- 根因: 分支耗时不对称。
- 原修复方式: AuthService.Login 和 UserAuthService.LoginWithRememberMe 在用户不存在时做一次 dummy 比较后返回 ErrInvalidCredentials。
- **对我们实现的要求**: 启动时用与真实密码相同的 cost 生成一个合法的 dummy 哈希（`bcrypt::hash("dummy", COST)`，存放在 once_cell 中）。用户不存在、用户被禁用等分支都执行一次 verify，统一返回 invalid credentials。
- **必测用例**: ①dummy 哈希能被 `bcrypt::verify` 正常解析（返回 Ok(false)）；②统计不存在用户与密码错误两种请求的耗时，量级一致（基准测试或至少单测验证调用了 verify）。

#### AUTH-10 前端 token 过期处理：业务码 401 也需清登录态，但登录接口本身的 401 不能跳转

- 提交: 21576cd7 2026-02-28, 98fa4d1a 2026-03-05, 4ea16976 2026-03-05
- 严重度: **中**
- 问题现象: 后端以 HTTP 200 + `status_code:401` 返回过期，用户端拦截器只在 HTTP 401 分支清理，导致 `user_token`/`user_profile` 残留、界面显示已登录但所有请求失败；修复后又出现：登录/注册/Telegram 登录/忘记密码接口因密码错误返回 401 时被当作过期，页面被强制刷新到登录页、错误提示丢失；后台 `/admin/login` 同样问题。
- 根因: 未区分"凭证失效"与"认证接口本身失败"。
- 原修复方式: `api`/`userApi` 响应拦截器在 `data.status_code===401` 时 removeItem('user_token','user_profile') 并跳 `/auth/login`；新增 `isAuthEndpoint(url)` 正则 `/\/auth\/(login|register|telegram\/login|forgot-password)/`、后台 `isLoginEndpoint` `/\/admin\/login\b/`，这些接口的 401 不清理不跳转，只提示。
- **对我们实现的要求**: Vue3 客户端统一在一个拦截器同时处理 HTTP 401 与 body status_code 401；对登录类接口白名单豁免；清理 token 与缓存的用户资料。
- **必测用例**: 过期 token 请求 /me 返回 status_code 401 → localStorage 清空并跳登录；登录密码错误返回 401 → 停留在登录页并显示错误。

## 13. 管理员/RBAC/权限（ADM，6 条）


#### ADM-01 支付渠道配置密钥脱敏返回，合并更新保留原值

- 提交: 4e4d5bbc 2026-07-27
- 严重度: **高**
- 问题现象: 后台支付渠道详情/列表接口原样返回 config_json 中的私钥与密钥（merchant_private_key、api_v3_key、secret_key 等），任何有渠道读权限的管理员或 XSS 都能窃取。
- 根因: 未脱敏。
- 原修复方式: admin_channel_handler.go 敏感键集合 {api_secret, api_v3_key, auth_token, client_secret, merchant_key, merchant_private_key, merchant_token, notify_secret, private_key, secret_key, webhook_secret}（小写比较）；返回时非空值替换为 `••••••••`；更新时 `mergePaymentChannelConfig`：敏感键值为空字符串或掩码 → 保留旧值；显式 null → 清除；请求未包含的敏感键 → 保留旧值；非敏感键以请求为准。
- **对我们实现的要求**: 渠道 DTO 输出一律脱敏；更新采用上述合并语义；新增网关时把其密钥字段加入敏感集合。
- **必测用例**: GET 渠道 → secret_key 为掩码；PUT 回传掩码 → 数据库保持原密钥；PUT 传 null → 被清除；PUT 传新值 → 更新。

#### ADM-02 只读审计员禁止通配策略；内置角色不可通过通用 API 修改，启动时收敛多余策略

- 提交: 4e4d5bbc 2026-07-27, 1581d986 2026-07-26
- 严重度: **高**
- 问题现象: readonly_auditor 角色策略为 `/admin/* GET`，任何新增 GET 接口（支付渠道密钥、卡密、交付内容、回调原文、钱包流水等）自动对其开放；内置角色可以被通用角色 API 改写/删除，导致权限漂移。
- 根因: 通配授权；内置角色未保护。
- 原修复方式: authz/bootstrap.go 审计员只授予明确列表：dashboard overview/trends/rankings/inventory-alerts、compliance/status、authz/me、2fa/status、ads/render/:slotCode、posts/:id/products、media 的 GET；新增 `ErrImmutableBuiltinRole`，`IsImmutableBuiltinRole` 的角色在更新策略/删除/改继承时拒绝；启动引导对 Immutable 角色删除不在种子中的 g 继承与 p 策略。1581d986 补充 rbac_coverage_test 覆盖所有路由都有权限映射。
- **对我们实现的要求**: 权限模型中不使用路径通配授予只读角色；内置角色只读并在启动时与种子同步（多余策略删除）；测试保证每个 admin 路由都在权限目录中。
- **必测用例**: 审计员访问 GET /admin/payment-channels/:id → 403；调用角色 API 修改内置角色 → immutable 错误；手动往内置角色加一条策略后重启 → 被移除。

#### ADM-03 初始管理员：由配置创建者恒为超管（不依赖用户名 admin）；env > config；release 模式禁用默认密码

- 提交: 4850c174 2026-03-18; e57d5ea1 2026-02-27
- 严重度: **高**（高；低）
- 合并条目: (1) 首次初始化管理员用户名非 admin 时未被标记超管 ｜ (2) 默认管理员初始化来源与 release 模式保护
- 问题现象:
  - (1) 配置文件里 bootstrap 管理员用户名设为 `root` 等非 admin 名称，初始化后 `IsSuper = EqualFold(username,"admin")` = false，且没有任何角色 → 登录后台全部 403，无法自救。
  - (2) 仅支持环境变量 `DJ_DEFAULT_ADMIN_USERNAME/PASSWORD`，Docker/面板用户无法通过配置文件设置初始密码。
- 根因:
  - (1) 超管判定硬编码用户名 "admin"；已存在管理员时的"修复"也只对 username="admin" 生效。
  - (2) 初始化来源单一。
- 原修复方式:
  - (1) `normalizeBootstrapAdminUsername`（空→admin）；新建时 `IsSuper: true`；已有管理员时对 bootstrap 用户名执行 `UPDATE admins SET is_super=true WHERE username=?`。
  - (2) `resolveDefaultAdminCredentials`：环境变量优先（trim），为空再取 `bootstrap.default_admin_username/password`；release 模式且最终密码为空 → 跳过初始化并警告（不使用弱默认密码）。
- **对我们实现的要求**:
  - (1) 初始化逻辑：admins 表为空 → 用配置的用户名/密码创建且 is_super=true；非空 → 确保配置中的 bootstrap 用户名（trim 后）为超管。不要以用户名字面量决定权限。
  - (2) 同样优先级；release 模式禁止使用内置默认密码创建管理员。
- **必测用例**:
  - (1) 配置 username="root" 空库启动 → root.is_super=true 且能访问 RBAC 保护接口；已有 root(is_super=false) 重启 → 被修复为 true；username 为空 → 创建 admin。
  - (2) env 空 + config 有密码 → 以 config 创建；release 模式二者都空 → 不创建管理员；env 与 config 都有 → 用 env。

#### ADM-04 后台列表排序/筛选参数必须白名单与严格解析

- 提交: e0d15df6 2026-06-10, 7674221a 2026-06-10, 068d6719 2026-06-09, a07bc1b8 2026-06-01, 0df1f02a 2026-06-01, 087d750d 2026-06-01, 4f30a0cf 2026-06-01, 51022332 2026-05-17
- 严重度: **中**
- 问题现象: 用户列表支持按 created_at/last_login_at/wallet_balance 排序，需防 ORDER BY 注入且分页稳定；从未登录用户在 PG/SQLite 中 NULL 排序位置不同；无钱包账户用户余额为 NULL；活动价名称搜索大小写行为在 SQLite 与 PG 不一致；bool 查询参数非法值被静默当 false；page_size 越界。
- 根因: 数据库方言差异与宽松参数解析。
- 原修复方式: applyUserSort 白名单，方向只接受 asc 否则 DESC，追加 `users.id DESC` 二级排序；last_login_at 先 `ORDER BY last_login_at IS NULL`（NULL 垫底）；wallet_balance LEFT JOIN wallet_accounts + COALESCE(balance,0)；非法字段回退 id DESC。活动价名称 `LOWER(name) LIKE LOWER(?)`。ParseQueryBoolPtr：缺省/空白=nil，非法值返回错误（400）；ParsePaginationWithBounds：page<1→1，page_size<1 或 >max(默认200) 回退默认值；时间范围 RFC3339 解析失败报错。用户列表支持 user_id 精确搜索。
- **对我们实现的要求**: 排序字段使用 enum 映射到列，不拼接用户输入；跨 SQLite/MySQL/PG 统一 NULL 排序与大小写不敏感 LIKE；参数解析严格化。
- **必测用例**: sort_by=`id;DROP` → 按 id DESC；last_login_at asc 时未登录用户在最后；page_size=10000 → 默认 20；is_active=abc → 400；name=SUMMER 能搜到 "summer sale"。

#### ADM-05 超管密码 CLI 重置需使旧会话失效；支付财务路由合规门禁；用户管理

- 提交: 9b676e1a 2026-05-16, 04aa745d 2026-05-16, c1351204 2026-05-26, 5320a55b 2026-05-26, df970c76 2026-05-20, 41e828e1 2026-05-20
- 严重度: **中**
- 问题现象: 运维需要重置超管密码；重置后旧 JWT 必须立即失效。支付/财务相关后台接口需在超管确认合规声明前禁止使用。
- 根因: —
- 原修复方式: `server reset-password --username x [--password y]`（未提供时终端隐藏输入两次，非终端从 stdin 读一行；最小长度 8），bcrypt 哈希，UpdatePassword 同时 `token_version = token_version + 1`、`token_invalid_before = now`，写 admin_login_log(event=password_reset_by_cli)。合规：ComplianceService 启动时加载，atomic 标志；确认需三段文本逐字匹配；PaymentComplianceRequired 中间件包住 payment-channels、payments、钱包调整、充值、对账、推广佣金/提现、分销财务等路由，未确认时超管返回 403 "compliance_required"、其他管理员 "compliance_required_by_super_admin"。后台可设置用户 email_verified（true 且原为空时写当前时间，false 清空）。
- **对我们实现的要求**: 修改管理员密码的所有路径都递增 token_version / 设置 token_invalid_before，JWT 校验比对；合规门禁作为 axum 路由层中间件，状态缓存于 AtomicBool，确认时加互斥防止并发重复写。
- **必测用例**: CLI 重置后旧 token 请求 → 401；未确认合规时 GET /admin/payments → 403 compliance_required；确认文本少一个字 → 拒绝。

#### ADM-06 内置角色种子必须覆盖所有后台路由（覆盖率测试防漏）

- 提交: 3b94e249 2026-04-13, e48521f0 2026-04-26, 164d06bd 2026-04-28; 78eb79e8 2026-03-30
- 严重度: **中**
- 合并条目: (1) 新增后台路由必须同步进内置角色种子（覆盖率测试） ｜ (2) 内置角色的权限点要覆盖所有新接口
- 问题现象:
  - (1) 新增的 `/admin/media/:id` PUT/DELETE、`/admin/procurement-orders/stats` GET 等接口没有加进 `authz.BuiltinRoleSeeds()`，非超管角色即使分配了相关模块权限也访问不了（前端报 403）。
  - (2) 新增接口没有加入内置角色的策略（Casbin 按 path+method 匹配），例如会员等级管理、`/admin/users/:id/wallet/adjust`、`/admin/orders/:id/refund-to-wallet`、`/admin/orders/:id/fulfillment/download`、api-credentials approve/reject、site-connections status/reapply-markup、procurement upstream-payload download、`/admin/wallet/recharges`，以及各类 settings 路由。非超管的运营或财务角色调用返回 403。readonly_auditor 甚至改不了自己的密码（`PUT /admin/password`）。
- 根因:
  - (1) 路由表和 RBAC 种子分开维护，靠人工同步。
  - (2) 新增路由时没有同步维护 RBAC 种子。
- 原修复方式:
  - (1) 逐个补种子。164d06bd 新增 `TestAllAdminRoutesCoveredByBuiltinRoles`：静态扫描 router.go 里的 `authorized.METHOD("/path")`，用 casbin `KeyMatch2` 与种子策略逐条比对，有未覆盖的路由就让测试失败。
  - (2) 在 `BuiltinRoleSeeds` 中补全各角色策略，并新增 `system_admin` 角色（settings/*、authz 管理）。readonly_auditor 增加 `/admin/password PUT` 和 `/admin/ads/impression POST`。
- **对我们实现的要求**:
  - (1) Rust 的路由注册用宏或集中表统一声明 (method, path, permission)，启动时或单测中校验每条 admin 路由都有权限定义并被至少一个内置角色覆盖。
  - (2) 写一个测试遍历 axum 路由表，确保每个 admin 路由至少被一个内置角色或超管覆盖，并且策略里没有已失效的路由（例如 `sync-sku` 已改名为 `sync`）。审计角色通过 `/admin/* GET` 拥有全部只读权限，其中包括卡密下载（`/orders/:id/fulfillment/download`）和上游交付下载，需要评估这些是否应当对只读审计开放（涉及敏感数据）。
- **必测用例**:
  - (1) 遍历 axum admin router 的所有路由，断言每条都能在内置角色策略中匹配到。
  - (2) ①路由覆盖测试：每个 admin 路由都能被某个角色匹配；②readonly_auditor 可以 PUT /admin/password，不能 POST /admin/users/:id/wallet/adjust；③customer_service 可以 POST refund-to-wallet。

## 14. 验证码/限流/风控（RISK，7 条）


#### RISK-01 订单风控：黑名单、待支付上限、下单频率、游客/会员分策略、按 risk_ip 计数与锁串行化

- 提交: 513619eb 2026-08-05; aacf587e/5113c31e 2026-04-08, 2146a4d0 2026-04-08
- 严重度: **高**（高；中）
- 合并条目: (1) 订单风控：游客/会员分策略、按 risk_ip 计数、锁键串行化、trusted_proxies ｜ (2) 订单风控：黑名单、待支付数量上限、下单频率，以及 Bot/下游豁免
- 问题现象:
  - (1) 原风控按 client_ip 统计待支付单，攻击者可伪造 X-Forwarded-For（Gin 默认信任所有代理）绕过；IPv6 用户每个地址都不同；待支付单计数在并发下单时竞态（同一 IP 并发多单都读到 count<上限）；游客可大量锁库存（下单不付款占库存）。
  - (2) 恶意用户批量下单占库存。上线风控后，Bot（服务器 IP 共用）和下游 API 订单被 IP 维度误伤。
- 根因:
  - (1) 未配置可信代理；IP 未归一；计数检查无锁。
  - (2) 新功能；不同来源的 ClientIP 语义不同。
- 原修复方式:
  - (1) 配置 `server.trusted_proxies`（默认 127.0.0.1/32、::1/128），启动时校验每项为 IP 或 CIDR，拒绝 /0（0.0.0.0/0、::/0），错误则 panic。`NormalizeRiskIP`：IPv4 原样，IPv6 聚合到 /64（`xxxx::/64`）；orders 新增 risk_ip 并在迁移中回填 pending 订单。新表 `order_risk_lock_keys(key_hash varchar(64) PK)`：`LockRiskKeys` 对 key（如 `guest:ip:<riskIP>`、`member:user:<id>`、`member:ip:<riskIP>`）做 SHA-256，排序去重后逐个 `INSERT ... ON CONFLICT DO NOTHING` + `SELECT ... FOR UPDATE`，在订单事务内、创建父订单与锁库存前执行，然后 `CountPendingGuestByRiskIP`（user_id=0、parent_id IS NULL、pending_payment）、`SumPendingGuestQuantityByRiskIP`（按商品汇总子订单 pending 数量）、`CountPendingByUserID`、`CountPendingMemberByRiskIP`。游客策略：MaxPendingOrdersPerIP、MaxPendingQuantityPerIPProduct（已挂单数量+本次>上限 → ErrPendingProductQuantityLimit）、MaxQuantityPerProductPerOrder、RateLimit、PaymentExpireMinutes（游客单独的支付超时）；需要 IP 而取不到 → ErrClientIPUnavailable。IP 黑名单解析时规范化（`net.ParseIP(...).String()`）。
  - (2) `OrderRiskControlService.CheckOrderAllowed` 在锁库存之前执行：①IP 黑名单（支持精确 IP 和 CIDR，解析结果按列表哈希缓存）→403 `risk_ip_blacklisted`；②游客邮箱黑名单（转小写后比较）→403；③待支付父订单数（`status=pending_payment AND parent_id IS NULL`，分用户/IP/游客邮箱三个维度，默认 3/5/2，0=不限）→429；④频率限制：Redis Lua 固定窗口 INCR+EXPIRE，超限时 `EXPIRE block_seconds`（默认窗口 60s、最多 5 次、封禁 120s），分 IP 和用户两个维度，返回 429 并带 `Retry-After` 头，消息中包含秒数。配置读取失败或 Redis 不可用时放行。`SkipIPRiskControl`（渠道/Bot 订单：跳过全部 IP 维度检查，用户维度仍生效）；`SkipRiskControl`（下游 API 订单：已签名、钱包扣款，完全跳过）。
- **对我们实现的要求**:
  - (1) axum 取客户端 IP 时只在 peer 地址属于 trusted_proxies 时才解析 X-Forwarded-For/X-Real-IP，禁止配置 /0；风控 IP 归一（IPv6 /64）；待支付配额检查在事务内通过"锁键表"串行化（兼容 SQLite/MySQL/Postgres），锁按哈希排序避免死锁。
  - (2) Rust 风控模块按以上规则实现，错误码与 Retry-After 头保持一致；来源标记（web/channel/upstream）由 handler 显式传入，不能从请求参数读取。
- **必测用例**:
  - (1) trusted_proxies 配 `0.0.0.0/0` → 启动失败；非可信来源携带 XFF → 使用 TCP 源地址；同一 IPv6 /64 下两个地址共享配额；游客 MaxPendingOrdersPerIP=2 并发 10 单 → 只成功 2 单；同 IP 同商品挂单数量 3+本次 3 > 上限 5 → 拒绝。
  - (2) CIDR 10.0.0.0/8 拦截 10.1.2.3；用户已有 3 个待支付订单时第 4 次下单返回 429；60 秒内第 6 次下单返回 429 且 Retry-After≈120；渠道订单的 IP 在黑名单中仍能下单，但用户维度超限仍被拦截；上游 API 订单完全不受影响。
- ⚠ 注：风控锁曾导致死锁（041c0e40，issues#271），见 DB-01。

#### RISK-02 支付回调、上游回调、渠道 API、礼品卡兑换必须限流

- 提交: 19654a38 2026-09-15
- 严重度: **中**
- 问题现象: `/api/v1/payment/callback/*`、webhook、`POST /api/v1/upstream/callback`、`/api/v1/channel/*`（Telegram Bot 等渠道 API）、用户礼品卡兑换接口此前无任何限流，可被刷（礼品卡码可被暴力枚举；回调接口可被用来打满验签/DB）。
- 根因: 限流只覆盖了登录/游客查单/上游 API 等，漏掉了这些公开或高价值入口。
- 原修复方式: router.go 新增三条规则：`channelAPIRule`（60s/600 次，封 30s，key=IP|`Dujiao-Next-Channel-Key` header，挂在 ChannelAPIAuthMiddleware 之前）；`callbackRule`（60s/120 次，封 60s，key=IP，覆盖支付回调、webhook、上游回调）；`giftCardRedeemRule`（60s/10 次，封 300s，key=`KeyByUserIDAndIP`）。
- **对我们实现的要求**: axum 路由层给以上四组路由挂限流 layer，参数与 key 维度保持一致；礼品卡兑换必须按 user_id+IP 严格限流；限流位于鉴权前时，key 不能只用未校验的 header。
- **必测用例**: 同一用户 60s 内第 11 次礼品卡兑换返回 429(error.rate_limited) 且 300s 内持续被拒；同一 IP 第 121 次回调返回 429；渠道 API 同 IP+Key 第 601 次被拒。

#### RISK-03 鉴权前限流 key 必须绑定来源 IP；last_used_at 只更新单列

- 提交: 9f8579ca 2026-09-15
- 严重度: **中**
- 问题现象: (1) 上游 API 限流 `KeyByUpstreamApiKey` 仅以 `Dujiao-Next-Api-Key` header 为 key，限流在鉴权之前，攻击者每次换一个随机 header 值即可绕过限流；反过来也可以伪造他人的 key 消耗对方配额。(2) UpstreamAPIAuthMiddleware 每次请求 `go credRepo.Update(cred)` 用 `Save` 整行回写，并发请求会用旧快照覆盖管理员刚改的 status/其他字段（丢失更新），且每请求一次写库。
- 根因: 限流 key 使用未验证输入；GORM Save 全列写回 + 异步 goroutine 使用过期对象。
- 原修复方式: 新增 `KeyByIPAndHeader(header)`：key=`ClientIP|header值`（header 截断到 128 字节，空则只用 IP）；上游 API Key、渠道 Key 都用它。新增 `TouchLastUsedAt(id, at)`：`UPDATE api_credentials SET last_used_at=? WHERE id=? AND deleted_at IS NULL`（UpdateColumn 单列），并 60 秒节流（`LastUsedAt==nil || now-LastUsedAt>1min` 才写）。
- **对我们实现的要求**: 所有"鉴权前"限流 key 必须含 IP；header 值需截断长度防止 Redis key 膨胀。记录 last_used_at 一律用 `update_many().col_expr(last_used_at).filter(id).filter(deleted_at.is_null())` 单列更新，不得 ActiveModel 全量 save；加 60s 节流。
- **必测用例**: 同一 IP 轮换 1000 个随机 API Key 请求上游 API，仍在第 N+1 次被限流（N=规则上限）；管理员禁用凭证与该凭证并发请求后，凭证状态仍为禁用；连续两次请求间隔 <60s 只写一次 last_used_at。

#### RISK-04 限流 Redis 不可用时降级为进程内计数（不再 fail-open/500），容量耗尽拒绝新 key；429 状态码

- 提交: 4e4d5bbc 2026-07-27
- 严重度: **中**
- 问题现象: Redis 未配置时限流中间件直接放行（client==nil 跳过）；Redis 故障时返回 500 使所有接口不可用；超限响应 HTTP 状态非 429。
- 根因: 限流完全依赖 Redis。
- 原修复方式: rate_limit.go 增加 `localRateLimiter`（map + mutex，最多 10000 key，过期清理；满了之后不淘汰窗口内计数，而是对新 key 直接判超限，防高基数 key 绕过；超限第 MaxRequests+1 次时把过期时间延长到 BlockSeconds）；Redis 为空或脚本执行失败/返回异常时降级本地并节流告警（1 分钟一次）；超限响应 `ErrorWithHTTPStatus(429, ...)`。
- **对我们实现的要求**: 限流实现提供 Redis + 本地内存（如 governor/moka）双模式，Redis 故障自动降级；超限返回 HTTP 429；本地模式容量上限与"满则拒绝新 key"。
- **必测用例**: 不配 Redis 时登录接口第 N+1 次返回 429；Redis 中途宕机 → 仍限流而不是 500。

#### RISK-05 签名鉴权中间件读取 body 要设上限

- 提交: 4a4abb1c 2026-03-31
- 严重度: **中**（中（DoS））
- 问题现象: `ChannelAPIAuthMiddleware`、`UpstreamAPIAuthMiddleware` 为了验签会 `io.ReadAll(c.Request.Body)`，攻击者发送几个 GB 的 body 会把内存耗尽。AppError.Err 被 JSON 序列化后可能泄露内部错误。
- 根因: 读取无上限；内部错误字段被序列化。
- 原修复方式: `http.MaxBytesReader(..., 10<<20)`，超过时返回 400。`AppError.Err` 加 `json:"-"`。
- **对我们实现的要求**: axum 全局加 `DefaultBodyLimit`，签名路由单独设置（10MB），支付回调同样限制。错误响应只输出 code/msg，内部 error 只写日志。
- **必测用例**: ①向 channel API 发 11MB body，返回 400/413，进程内存不暴涨；②内部数据库错误的响应体里不含 SQL 或错误堆栈。

#### RISK-06 CORS / JWT / 限流中间件的边界行为

- 提交: a6645ea0 2026-02-26, 7ce223e9 2026-02-26, aa31fcb7 2026-03-10
- 严重度: **中**
- 问题现象: allow_credentials=true 时返回 `Access-Control-Allow-Origin: *` 浏览器会拒绝；JWT secret 未配置时不能放行；限流 key 以邮箱区分时大小写/空格可绕过，且读取 body 后未恢复导致 handler 读不到 body；Redis 未配置时的行为。
- 根因: 中间件边界情况未定义。
- 原修复方式: `resolveAllowedOrigin(origin, allowed, credentials)`：通配且带凭证 → 回显请求 Origin；通配不带凭证 → `*`；白名单命中回显、未命中返回空；默认方法加入 PATCH。`JWTAuthMiddleware("")` → 返回 status_code 401；JWT 解析统一 `WithValidMethods([HS256])`。`KeyByIPAndJSONField("email")` → `lower(trim(email))|ip` 且恢复 body。`RateLimitMiddleware(nil client)` → 放行；Redis 执行出错 → 500 `rate_limit_unavailable`（fail-closed）；Channel API 路径（/api/v1/channel）返回 channel 格式错误体（429/500）。
- **对我们实现的要求**: CORS 带凭证时绝不输出 `*`；JWT 仅接受 HS256 且空 secret 启动时报错/请求 401；限流 key 归一化（邮箱小写去空格）；在 axum 中读取 body 做 key 时要重建 Request body；明确 Redis 缺失/故障的策略并测试。
- **必测用例**: Origin=https://a.com、配置 ["*"]、credentials=true → ACAO=https://a.com；白名单外 Origin → 无 ACAO；alg=none 或 HS512 token → 401；邮箱 " Test@Example.com " 与 "test@example.com" 共享同一限流计数；限流后 handler 仍能读到完整 JSON body。
- ⚠ 演进说明：本条中“Redis 执行出错 → 500 fail-closed / 未配置 Redis → 放行”已被 4e4d5bbc（2026-07-27）取代为“降级到进程内限流”，见 RISK-04。CORS/JWT/邮箱归一化部分仍有效。

#### RISK-07 Redis 登录限流：超过阈值后按 block_seconds 延长封禁

- 提交: efaf3dfe 2026-02-14
- 严重度: **中**
- 问题现象: 配置了 `security.login_rate_limit.block_seconds` 但未生效，超限后窗口一过即可继续暴力尝试。另 CORS 允许方法缺 PATCH，PATCH 接口预检失败。
- 根因: Lua 脚本只在首次 INCR 时设置窗口过期，未使用 block 参数。
- 原修复方式: `router/rate_limit.go` Lua：`INCR`；`current==1` 时 `EXPIRE window`；若 `max>0 && block>0 && current==max+1` 则 `EXPIRE block`；返回 `{current, ttl}`，超限时提示等待 ttl 秒（ttl<1 用 window，最少 1）。登录/相关路由传入 BlockSeconds；config 示例 CORS 加 PATCH。
- **对我们实现的要求**: 限流实现（Redis 或内存）：窗口计数，首次超限时（第 max+1 次）把 key TTL 设为 block_seconds；返回剩余等待秒数；Redis 出错时返回 500 rate_limit_unavailable（原实现 fail-closed）。CORS 允许 GET/POST/PUT/PATCH/DELETE/OPTIONS。
- **必测用例**: max=5, window=60, block=600：第 6 次请求被拒且 TTL≈600；第 7 次仍被拒；PATCH 预检请求返回允许。

## 15. 设置/公共配置/回调路由（SET，5 条）


#### SET-01 自定义回调路由：隐藏默认路径 + 路径冲突/重复校验 + 缓存失效

- 提交: 79dd297d 2026-04-04, 047acdc6 2026-04-04, 563b6796 2026-04-04
- 严重度: **高**（高（支付回调入口，配置错误可劫持/屏蔽已有 API 或导致回调全部 404））
- 问题现象: 管理员需要把 `/api/v1/payments/callback`、`/api/v1/payments/webhook/paypal`、`/api/v1/payments/webhook/stripe`、`/api/v1/upstream/callback` 换成自定义路径以免被扫描。若不校验，管理员可能把回调路径设成 `/api/v1/admin/...` 或 `/api/v1/public/...`，路由中间件会把这些已有接口的请求全部转给支付回调 handler（覆盖/劫持已有路由）；两个回调配置成同一路径时只有第一个生效；配置了自定义路径后默认路径仍然可用就失去了隐藏的意义。此外，未匹配任何回调格式时旧代码返回 400 + "fail" 文本，泄露了"这里是回调入口"。
- 根因: 动态路由没有保留前缀黑名单和去重；默认路径未屏蔽；无法识别的回调返回可识别的报文。
- 原修复方式: 新增 `callback_routes_config` 设置项及 `normalizeCallbackRoutePath`：trim，去掉 `?`/`#` 之后的部分和尾部 `/`，必须以 `/api/` 开头，且 `path+"/"` 与保留前缀（`/api/v1/public/`、`/admin/`、`/auth/`、`/guest/`、`/channel/`、`/upstream/api/`、`/user/`）双向都不能是前缀关系，不合法的一律置空（回退为默认）。`deduplicateCallbackRoutes` 把后出现的重复路径清空。`CallbackRouteMiddleware` 为全局中间件，只处理 GET/POST 和 `/api/` 前缀：命中自定义路径就分发给对应 handler（PayPal/Stripe/upstream 只接受 POST），对应的自定义路径已配置时默认路径返回 404。路由配置放在进程内缓存（TTL 5 分钟，RWMutex 双重检查），管理员保存该设置时调用 `InvalidateCallbackRoutesCache`。无法识别的支付回调改为 `AbortWithStatus(404)`。前端做同样的校验（以 /api/ 开头、与系统前缀冲突、重复）。
- **对我们实现的要求**: axum 中用一个 fallback/middleware 层做动态分发。保存时在服务端做与上述一致的归一化（前端校验不能替代）。保留前缀列表必须覆盖我们所有的 API 前缀（包括 admin、channel、upstream API 等），比较时双向检查 `starts_with`。默认路径只在对应自定义路径已配置时才屏蔽（按字段单独判断）。设置更新后立即让缓存失效（多实例部署需通过 Redis pub/sub 或短 TTL）。无法识别的回调返回 404，不带正文。
- **必测用例**: ①保存 `payment_callback="/api/v1/admin/x"`，结果落库为空，`/api/v1/admin/*` 仍走原 handler；②保存 `/api/pay/cb/?a=1#x`，归一化为 `/api/pay/cb`；③两个字段都填 `/api/cb`，第二个被清空；④配置 stripe_webhook 后 POST 默认 `/api/v1/payments/webhook/stripe` 返回 404，自定义路径能进入 Stripe 验签；未配置的 paypal 默认路径仍可用；⑤对自定义 stripe 路径发 GET，不分发（落到 404）；⑥更新设置后下一次请求立即使用新路径；⑦未知格式回调返回 404。

#### SET-02 首页公告弹窗：归一化、排期与 XSS

- 提交: 75a2a82c 2026-05-20, 3d1198fc 2026-05-20, d52b0205 2026-05-20
- 严重度: **中**
- 问题现象: （新功能）公告内容为富文本，前台 v-html 渲染；需要排期与“内容变化后重新弹出”。
- 根因: —
- 原修复方式: 后端归一化：type 仅 normal/info/warning（否则 normal）；start_at/end_at 必须 RFC3339 否则置空；未启用/不在排期/各语言内容均为空则不下发；version = FNV-32a(type + 各语言 title/content) 8 位 hex（仅改时间不变）。前端 DOMPurify.sanitize 后 v-html，按 version 记录已读。
- **对我们实现的要求**: 同样归一化；前端必须 DOMPurify；注意公开配置 60 秒缓存会导致排期边界最多延迟 60 秒（可接受或在缓存命中时再判断排期）。
- **必测用例**: content 含 `<img onerror=alert(1)>` → 渲染后无事件属性；end_at 已过 → 不返回 announcement；只改 start_at → version 不变。
  跳过（已检查、与 bug/安全无关或纯样式/文案/构建/重构/字段暴露）: f1835684 dd536914 dd530914 486e71a5 b6ef6e43 7bafd7f3 9e91ee1a 8b4a5123 0a478b0e 57c26c10 1694c64b 9e80430e 71cf0c43 6b401967 6f559252 e35fa35c 676bb03c 73263cc1 fca83d12 8ad13160 f79839ef c30f4c59 16402a77 e9de36f4 9cc540b1

#### SET-03 sitemap/robots 的基础 URL 不要信任 X-Forwarded-Host

- 提交: df16ce1c/12aad5a0 2026-05-07
- 严重度: **中**
- 问题现象: `resolveSitemapBaseURL`：优先用后台 `brand.site_url`，没有配置时取 `X-Forwarded-Host`/`X-Forwarded-Proto`/Host。攻击者可以伪造 Host 生成指向恶意域名的 sitemap；缓存 key 为 `sitemap:xml:{baseURL}`，任意 Host 都会生成新的缓存条目（缓存膨胀）。
- 根因: 信任了客户端可控的请求头。
- 原修复方式: （原实现如上，未修复）。sitemap 只收录启用的分类、上架且分类启用的商品、已发布的文章，缓存 5 分钟；robots 禁止抓取 /api/、/admin/、/me/、/cart、/checkout、/pay、/orders/、/guest/、/auth/ 等。
- **对我们实现的要求**: Rust 只使用配置的 site_url；未配置时只在受信代理配置下使用 forwarded 头，或者直接返回不带 Sitemap 行的 robots。缓存 key 不包含请求头。
- **必测用例**: 配置了 site_url 时，伪造 X-Forwarded-Host: evil.com，sitemap 的 loc 仍为配置的域名；sitemap 不含下架商品和草稿文章。

#### SET-04 设置/筛选参数白名单与范围钳制

- 提交: 840d117e 2026-06-20, 1906bfc1 2026-06-23, adccc6fe 2026-07-06, 5335970d 2026-07-14, 5bb3406c 2026-07-14
- 严重度: **低**
- 问题现象: 上游同步 pageSize/maxPages/concurrency 由硬编码改为后台可配；店面模板 storefront_template；商品映射列表 upstream_status/product_status 筛选；分销站客服 Telegram 链接只接受 `https://t.me/`，`https://telegram.me/` 被拒（部分地区 t.me 被屏蔽）。
- 根因: 新增可配置项需要防御非法值；链接前缀白名单过窄。
- 原修复方式: `NormalizeUpstreamSyncConfig` 对三个参数做最小/最大钳制（低于最小回默认、高于最大取最大）；`normalizeStorefrontTemplate` 仅 "vault" 保留，其余归一为 "classic"；映射列表 upstream_status 仅 active/inactive/deleted、product_status 仅 active/inactive，否则 400（error.invalid_upstream_status / invalid_product_status），搜索用方言兼容的本地化 LIKE；Telegram 前缀接受 `https://telegram.me/`、`https://t.me/`、`tg://`，默认联系方式与 Mini App 入口链接改用 `https://telegram.me/{bot}/webapp`。
- **对我们实现的要求**: 所有从设置表读出的数值参数在使用前统一 normalize（钳制），枚举类参数白名单；Telegram 链接校验同时接受 telegram.me / t.me / tg://。
- **必测用例**: sync_page_size=0 → 默认值，=100000 → 上限；storefront_template="evil" → classic；upstream_status=foo → 400；support.telegram="https://telegram.me/alice" → 保存成功；"http://t.me/x" → 拒绝。

#### SET-05 公开商品 DTO 缺少 seo_meta

- 提交: 75724cb0 2026-05-04, 12aad5a0 2026-05-07
- 严重度: **低**
- 问题现象: 商品详情页 SEO meta 标签缺失（#159）。
- 根因: `publicProductView.toProductResp` 没有输出 `SeoMetaJSON`。
- 原修复方式: `ProductResp` 增加 `seo_meta`。前端 `usePageSeo` 由页面负责 canonical/og/twitter 标签，全局只设置 title/description/keywords/favicon/html lang，避免重复标签。
- **对我们实现的要求**: 公开商品 DTO 包含 seo_meta；前端各页面自行设置页面级 SEO 标签，不在全局重复设置。
- **必测用例**: GET 公开商品详情，响应包含 seo_meta 字段。

## 16. 上传/素材/SVG/XSS（UPL，5 条）


#### UPL-01 SVG 上传：XML 解析器校验 + 危险元素/属性拦截 + /uploads 下 SVG 强制下载与 CSP sandbox + 默认关闭 SVG

- 提交: bbe1b278 2026-09-15; 68bf69f5 2026-03-25
- 严重度: **高**
- 合并条目: (1) SVG 上传用 XML 解析器校验 + /uploads 下 SVG 强制下载与 CSP sandbox + 默认关闭 SVG ｜ (2) SVG 上传的安全检查
- 问题现象:
  - (1) 旧 `validateSVGSafety` 基于字符串黑名单：`onload<Tab>=`、`onload\n=`、`onload  =`、未列入名单的 `onbegin`、实体编码 `&#106;avascript:`、`<?xml-stylesheet?>`、`<!ENTITY>` 均可绕过，导致存储型 XSS（直接打开 /uploads/x.svg 在站点源执行脚本）。
  - (2) 支持上传 SVG（`http.DetectContentType` 识别不了 SVG，会识别成 text/xml/text/plain），SVG 可内嵌脚本，若同源直出即存储型 XSS（可盗后台 token）。
- 根因:
  - (1) 黑名单字符串匹配无法覆盖浏览器 XML 解析的等价写法。
  - (2) SVG 是可执行文档而非位图。
- 原修复方式:
  - (1) upload/application/service.go 用 `encoding/xml` Decoder（Strict=true）逐 RawToken 检查：元素名 script/foreignobject 拒绝；任何以 `on` 开头的属性拒绝；属性值去掉 ≤0x20 控制/空白字符后小写，前缀 `javascript:`、`data:text/html`、`data:application`、`data:image/svg` 拒绝；ProcInst 仅允许 target=`xml`；Directive 含 `<!entity` 拒绝；解析失败（格式错误）拒绝。router.go `/uploads` 对 `.svg` 请求加 `Content-Disposition: attachment`、`Content-Security-Policy: sandbox; script-src 'none'`、`X-Content-Type-Options: nosniff`。config.yml.example 默认移除 image/svg+xml 与 .svg。
  - (2) 扩展名 `.svg` 且内容以 `<?xml`/`<svg` 开头或包含 `<svg` 时 contentType=image/svg+xml；跳过尺寸解码；`validateSVGSafety`（小写后子串黑名单）：`<script`、on{load,click,error,mouseover,mouseout,mousemove,focus,blur,change,submit,animationstart,animationend,animationiteration}=、`javascript:`、`data:text/html`、`data:application`、`<foreignobject`。配置 allowed_types 加 image/svg+xml、allowed_extensions 加 .svg。
- **对我们实现的要求**:
  - (1) 默认不允许 SVG；允许时用 quick-xml 等严格解析器按上述规则逐 token 校验（注意实体解码后的属性值再检查）；静态文件服务对 .svg 响应附加上述三个头（其实所有 uploads 都应加 nosniff）。
  - (2) 原实现黑名单不完整（漏 `onbegin`/`onend` 等 SMIL 事件、`<use href=外部>`、实体编码 `&#106;avascript:`、`<iframe>`/`<embed>`、xlink:href、`<!ENTITY` XXE），Rust 版应：(1) 用 XML 解析器做白名单净化（只保留安全元素/属性，删除所有 `on*` 属性、外部 href、DOCTYPE/ENTITY），或直接拒绝可疑内容；(2) 上传文件服务时对 SVG 加 `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'`、`X-Content-Type-Options: nosniff`，最好 `Content-Disposition: attachment` 或走独立域名；(3) 以扩展名+内容双重判定 MIME。
- **必测用例**:
  - (1) 表中各 payload（tab/换行/双空格 onload、onbegin、`&#106;avascript:`、xlink:href javascript、xml-stylesheet PI、ENTITY 声明、未闭合标签）→ 拒绝；带 DOCTYPE PUBLIC 与 `<style>` 的正常 SVG → 通过；GET /uploads/a.svg 响应含 attachment 与 CSP sandbox 头。
  - (2) 安全 `<svg><circle/></svg>` 上传成功扩展名 .svg；`<script>`、`onload=`、`href="javascript:"`、`<foreignObject>`、`data:text/html` 均拒绝；另测 `onbegin=`、`&#106;avascript:`、`<!DOCTYPE svg [<!ENTITY ...>]>` 也拒绝；`.svg` 扩展但内容为 `<html>` → 拒绝。

#### UPL-02 后台渲染远端 Markdown（Release Notes）必须 DOMPurify 白名单净化；自更新回滚安全闸门

- 提交: 87cc68b5 2026-07-26, 85dc20d9 2026-07-26
- 严重度: **高**
- 问题现象: 后台"一键升级"对话框用 marked 把 GitHub Release 正文渲染后 v-html 输出，marked v5+ 不再 sanitize，`<img onerror>`、`<a href="javascript:">` 会在管理员浏览器执行，窃取 localStorage 中的 admin_token。另外升级后回滚在新版本已开始数据库迁移后仍允许，导致旧二进制读不懂新 schema。
- 根因: 未净化远端 HTML；回滚缺少安全判断。
- 原修复方式: releaseNotes.ts `renderReleaseNotes`：marked(gfm, breaks) 后 DOMPurify：ALLOWED_TAGS 限排版标签（p, br, hr, strong, em, del, s, code, pre, blockquote, ul, ol, li, a, img, span, h1-h6, table 系列），ALLOWED_ATTR 只 href/title/src/alt，禁 style/class/id/data-*，URI 仅 `^(?:https?:|mailto:|#)`；输出 DOM fragment 后给外链加 target=_blank rel=noopener noreferrer。selfupdate：升级元数据（前后版本、MigrationStarted、NewVersionStarted）原子落盘（临时文件+rename）；元数据缺失/损坏视为"未知"=不安全；迁移已开始或未知时回滚返回 `error.update_rollback_unsafe`，需 force；下载→校验→替换全程持跨进程 flock；启动闸门与回滚共用锁，回滚标记使尚未迁移的新进程退出；下载上限 100MB、解压二进制单独上限。
- **对我们实现的要求**: 前端任何 v-html 渲染外部/用户可控 Markdown 都必须经过 DOMPurify 同等白名单；若实现自更新，照搬回滚闸门规则。
- **必测用例**: release body 含 `<img src=x onerror=alert(1)>`、`[x](javascript:alert(1))`、`<a style=...>` → 输出中无 onerror、无 javascript: href、无 style。

#### UPL-03 素材库：上传落库、删除物理文件、上游图片入库

- 提交: de7e906e 2026-04-01, baeb1c5d 2026-04-01, 4898367d 2026-04-01, cd57f7a0 2026-04-01, a057ed5c 2026-04-04, 2f87457a 2026-04-04, 89a2d1a4 2026-04-04, 846bf9a6 2026-04-04
- 严重度: **中**（中（文件删除涉及路径安全；SVG 仍然需要安全检查））
- 问题现象: 最初删除素材只软删 media 记录，磁盘文件永远不会被清理；上游克隆商品下载的图片没有进素材库；会员等级图标改成可以用素材图片后，前台 PersonalCenter 仍然按文本渲染 `{{ icon }}`，把 `/uploads/...` 路径当成文字显示出来。
- 根因: 素材记录与文件的生命周期不一致；图标字段语义扩展了（emoji 或 URL），但渲染没有跟着区分。
- 原修复方式: `UploadService.SaveFileWithMeta` 返回 URL/文件名/MIME/尺寸，上传后调用 `MediaService.RecordMedia`，按 path 去重。`RecordLocalFile` 为上游下载的文件补录（用 `http.DetectContentType` 取 MIME，SVG 不解码尺寸）。`MediaService.Delete` 在删除记录后执行 `os.Remove(strings.TrimPrefix(media.Path,"/"))`，文件不存在时忽略。上传仍然保留大小、扩展名白名单、DetectContentType 白名单、图片宽高上限和 `validateSVGSafety`。前端用 `isImagePath(icon)`（以 `/uploads/` 或 `http` 开头）决定渲染 `<img>` 还是文字。
- **对我们实现的要求**: 删除物理文件前必须把 path 规范化，并确认它位于配置的 uploads 根目录内（canonicalize 后做 starts_with 检查），拒绝 `..` 和绝对路径，否则 media.path 被篡改时会造成任意文件删除。删除前最好检查引用（商品图片、banner、富文本），或者至少提示。上传必须保留：大小限制、扩展名白名单、魔数检测、图片尺寸限制，SVG 脚本和外部引用检查（或者直接禁止 SVG，或用 CSP/Content-Disposition 返回）。上游下载的图片同样走这些校验。图标字段渲染时按 URL/emoji 区分，URL 只允许 `/uploads/` 或 http(s)。
- **必测用例**: ①上传 png 后 media 表新增一条，width/height 正确；②同一路径重复 Record 不产生重复记录；③删除素材后磁盘文件不存在；④把 media.path 改成 `/../../etc/passwd` 再删除，拒绝，文件不被删除；⑤上传带 `<script>` 的 SVG 被拒绝；⑥会员等级图标为 `/uploads/a.png` 时前台渲染 img，为 emoji 时渲染文字。

#### UPL-04 telegram 场景上传绕过扩展名/类型白名单

- 提交: 43462172 2026-03-11
- 严重度: **中**
- 问题现象: 为让 Bot 群发附件可发 zip 等文件，`SaveFile` 在 `scene=="telegram"` 时跳过 AllowedExtensions 与 AllowedTypes 检查（仍限制大小），文件落在公开 `/uploads/` 目录。若该场景可被非管理员调用或允许 .html/.svg，则可上传可执行/XSS 文件并同域访问。
- 根因: 以场景名放宽白名单，缺少替代约束。
- 原修复方式: 仅按 scene 跳过白名单（测试 `TestUploadServiceSaveFileAllowsArchiveForTelegramScene`：.zip 可保存）。
- **对我们实现的要求**: 若实现同类"附件"场景：仅限具备权限的管理员路由；使用单独黑名单禁止 .html/.htm/.svg/.js/.php 等可渲染/可执行类型；强制 `Content-Disposition: attachment` 或存放到不可直接浏览的目录；文件名一律 uuid。
- **必测用例**: 普通用户以 scene=telegram 上传 → 403；管理员上传 .zip → 成功；管理员上传 .html/.svg → 拒绝；访问已上传附件响应头含 attachment 与 nosniff。

#### UPL-05 上传校验错误返回 500 且前端看不到原因；批量删除素材

- 提交: 983ce46a 2026-05-25, 113f4f32 2026-05-25, a0146689 2026-06-02, 42ad93e1 2026-06-02
- 严重度: **低**
- 问题现象: 文件超限、扩展名/Content-Type 不允许、图片尺寸超限、SVG 不安全等校验失败统一返回 500 error.upload_failed，管理员不知原因；前端 blob 下载接口出错时返回的是 JSON 却被当文件处理。
- 根因: 业务校验错误未与内部错误区分。
- 原修复方式: UploadValidationError 类型，handler 用 IsUploadValidationError 判断返回 400 + 具体消息（SVG 安全校验 validateSVGSafety 失败同样 400）。前端 request 对 blob 响应：content-type 为 application/json 时解析 msg 并报错；上传前按 10MB 过滤并提示。素材批量删除 BatchDelete 逐个复用单删逻辑（保证文件清理一致），返回 success_count/failed_ids。
- **对我们实现的要求**: 上传服务返回枚举错误（Validation vs Internal），校验类→400 带消息；SVG 仍需安全校验；blob 接口前端判断 JSON 错误体；批量删复用单删。
- **必测用例**: 上传 11MB → 400 且消息含 "最大 10 MB"；含 script 的 SVG → 400；导出接口返回 JSON 错误时前端提示 msg。

## 17. 通知/邮件/Telegram Bot（NTF，11 条）


#### NTF-01 订单状态邮件被滥用为邮件轰炸；自动发货订单重复邮件

- 提交: faacb3f5 2026-05-14, 5288b841 2026-05-18
- 严重度: **高**
- 问题现象: 攻击者用伪造/他人邮箱批量游客下单不付款，订单过期取消触发“订单已取消”邮件，形成退信/轰炸；SMTP 故障时任务无限重试堆积；关闭 SMTP 或订单通知后，队列中旧任务（重试/计划中）仍发送。全自动发货订单支付后先发“已支付”再发“已完成（含卡密）”，重复打扰。
- 根因: 消费端未二次校验开关；游客取消通知无价值却可被滥用；重试无上限。
- 原修复方式: EnqueueOrderStatusEmail 默认 MaxRetry(3)、Retention(24h)；worker handleOrderStatusEmail 先读 SMTP 设置，读失败 fail-closed（不发不重试），!Enabled 或 !OrderNotificationEnabled 直接跳过；order.UserID==0 且状态为 canceled 时不发。支付成功时 isOrderFullyAutoFulfill（父订单所有子订单或单订单 shouldAutoFulfill）则跳过 paid 邮件。
- **对我们实现的要求**: 任务队列（Rust 端自实现或 apalis 等）对邮件任务设置重试上限与保留期；消费时再次检查开关（fail-closed）；游客订单取消不发邮件；全自动发货订单不发“已支付”邮件。
- **必测用例**: 游客订单过期取消 → 无邮件任务执行发送；登录用户取消 → 发送；SMTP 关闭后执行积压任务 → 不发送；设置读取失败 → 不发送且任务成功结束；全自动卡密订单支付 → 只发 completed 邮件。

#### NTF-02 白标（分销商站点）邮件品牌隔离，禁止回退到主站品牌

- 提交: a619ffbc 2026-07-28
- 严重度: **中**
- 问题现象: 分销商域名下的用户收到的验证码邮件、订单状态邮件使用主站 site_name/site_url/发件人名称，泄露白标背后的主站身份。
- 根因: 邮件品牌只读取全局设置。
- 原修复方式: 新增 `mailbrand.Brand{SiteName, SiteURL, FromName, ReplyTo}` 与 Resolver：订单带 ResellerID 或请求上下文租户为分销商时，读取分销商 SiteConfig（site_name → SiteName/FromName，support_json.email 经 `mail.ParseAddress` 规范后作为 Reply-To）；无配置时 `ResellerFallback(host)`（名称=域名，URL=https://域名），绝不回退主站；resolver 缺失时分销订单也走 fallback（fail closed）；品牌解析失败则任务返回错误重试。SMTP：From 名称可被覆盖、写 Reply-To 头（规范化防头注入），验证码邮件主题加 `站点名 - ` 前缀、正文附站点与网址。分销站点公共配置 brand.site_url 也改为分销域名。
- **对我们实现的要求**: 发送任何邮件前按订单/请求租户解析品牌；分销商作用域永不使用主站品牌；Reply-To 需经地址解析防 CRLF 注入。
- **必测用例**: 分销域名 shop.example 下注册 → 验证码邮件主题含分销站点名、正文网址为 https://shop.example、From 名为分销站名；分销商未配置站点名 → 使用域名；主站用户 → 主站品牌。

#### NTF-03 SMTP 认证机制选择、连接关闭、RFC 5322 邮件头

- 提交: 0fb63ad3 2026-04-14, 249cc8ec 2026-04-15, 0423e4d4 2026-04-16, 51a43d84 2026-04-21
- 严重度: **中**
- 问题现象: Office365 等服务器只支持 AUTH LOGIN，Go 的 `smtp.PlainAuth` 认证失败；成功发送后又 defer Close，产生 "use of closed network connection" 噪音日志；邮件缺少 Date 和 Message-ID 头，被部分服务商判为垃圾邮件或直接拒收。
- 根因: 只实现了 PLAIN；关闭逻辑重复；邮件头不完整。
- 原修复方式: `authenticateSMTPClient`：没有用户名和密码时跳过认证；服务器不支持 AUTH 扩展时跳过；否则优先 LOGIN、再退回 PLAIN，两者都不支持时返回错误。LOGIN 实现要求 `server.TLS` 为真且 host 匹配，按 challenge 中的 "username"/"password" 回送对应值。成功路径只调用 QUIT，QUIT 失败或流程出错时才 Close，忽略"已关闭"类错误。`writeStandardHeaders`：写入 Date(RFC1123Z)、Message-ID(`<随机16字节hex@发件域名>`)、From、To、Subject(Q 编码)、MIME-Version。
- **对我们实现的要求**: Rust 使用 lettre，配置 `authentication(vec![Mechanism::Login, Mechanism::Plain])`；确保生成 Date 和 Message-ID（lettre 默认会生成，需要验证）；主题使用 RFC2047 编码；明文连接不发送凭据。
- **必测用例**: 模拟 SMTP 服务器只声明 `AUTH LOGIN` 时发送成功；声明 `AUTH PLAIN LOGIN` 时选择 LOGIN；邮件原文包含 Date、Message-ID 头，Message-ID 域名取自 From。

#### NTF-04 库存告警发送间隔失效（alert_type 被本地化后比较不上）

- 提交: 7ca90c27 2026-03-31, f462e7cb 2026-03-29, 541495b8 2026-04-02
- 严重度: **中**（中（告警刷屏））
- 问题现象: 7ca90c27 为了让模板显示中文，把 payload 里的 `data["alert_type"]` 从 `low_stock_products` 改成本地化标签"低库存商品"/"Low Stock"。`acquireInventoryAlertInterval` 用 `alert_type` 判断 `isInventoryAlertType`，结果永远为 false，直接放行，库存告警的 interval 去重（Redis SetNX）失效，每轮巡检都会发送。
- 根因: 把用于逻辑判断的机器可读 key 和展示用的 label 放在了同一个字段。
- 原修复方式: payload 增加 `alert_type_key`（原始常量），`alert_type`、`alert_type_label` 作为展示字段。新增 `resolveInventoryAlertTypeKey`：优先读 `alert_type_key`，没有时把 `alert_type` 与三种 locale（zh-CN/zh-TW/en-US）的标签反查回 key，兼容队列中的旧任务。测试 `TestResolveInventoryAlertTypeKey`。
- **对我们实现的要求**: 通知 payload 里的逻辑字段（type key、biz id）和展示字段（label）必须分开，限流/去重只用 key。队列任务的结构变更要兼容旧格式（serde 字段加 default 并实现回退解析）。
- **必测用例**: ①interval=3600，连续两次触发 low_stock 告警，第二次被 SetNX 拦截；②payload 只有 `alert_type="低庫存商品"`（旧任务）时仍能解析为 low_stock_products 并受间隔约束；③非库存类告警（pending_orders）不受该间隔影响。

#### NTF-05 SMTP 关闭时不入队、不可恢复错误不重试

- 提交: c610cdde 2026-03-24
- 严重度: **中**
- 问题现象: 未启用 SMTP 时，每个订单状态变化仍入队 `order_status_email` 任务，worker 发送返回 ErrEmailServiceDisabled → asynq 按错误重试多轮，队列堆积、日志刷屏。
- 根因: 入队前不检查 SMTP 设置；worker 对"配置型/不可恢复"错误也返回 error。
- 原修复方式: `enqueueOrderStatusEmailTaskIfEligible` 增加 settingService + 默认邮件配置参数，`GetSMTPSetting(default)` 未启用则 skipped=true 不入队（fulfillment/order/payment/procurement 全部调用点改造）；worker `handleOrderStatusEmail` 对 `ErrEmailServiceDisabled`、`ErrEmailServiceNotConfigured`、`ErrInvalidEmail` 记 debug 并返回 nil（不重试），其它错误才返回 err。
- **对我们实现的要求**: 队列任务区分可重试/不可重试错误（不可重试直接 ack）；入队前按"当前生效设置（DB 覆盖配置文件）"判断 SMTP 是否启用；Telegram 占位邮箱等无效收件人也直接跳过。
- **必测用例**: SMTP disabled → 订单支付后队列无 email 任务；worker 收到任务但 SMTP 被关闭 → handler 返回 Ok 不重试；收件人为非法邮箱 → Ok；SMTP 超时 → Err（进入重试）。

#### NTF-06 邮件/Bot 通知：真实换行、Telegram 占位邮箱与空邮箱不发邮件

- 提交: 08b459c4 2026-02-23, 10517663 2026-02-23; ef5c9513 2026-03-11, 6b637bf6 2026-03-12, 2b96b2e6 2026-03-11, 4877c384 2026-03-12, aa31fcb7 2026-03-10, d28083c7 2026-03-10
- 严重度: **中**（中；低）
- 合并条目: (1) 邮件交付内容拼接用真实换行；Telegram 占位邮箱/空邮箱不发邮件 ｜ (2) Bot 通知与 Telegram 占位邮箱
- 问题现象:
  - (1) (1) 父订单邮件拼子订单卡密用 `fmt.Sprintf("[%s]\\n%s")`，邮件里出现字面 `\n`。(2) Telegram 登录用户邮箱是占位 `telegram_<id>@login.local`，各状态邮件会发往该假地址（投递失败/被 SMTP 标记）；游客订单无邮箱时也入队。
  - (2) ①Bot 通知直接 POST 到 channel client 的 `CallbackURL` 原样地址，新增事件类型后路径需要区分；充值成功无 Bot 通知；②Telegram 登录用户的占位邮箱 `telegram_<id>@login.local` 仍被发送 SMTP 邮件（退信、浪费额度）；③群发图片以 document 发送，客户端不显示预览；④Channel API 鉴权对"key 不存在/签名错误/时间戳过期"返回不同错误码，可被用来探测有效 key。
- 根因:
  - (1) 转义错误；入队前未判断收件人有效性。
  - (2) 事件扩展与身份占位未统一处理。
- 原修复方式:
  - (1) `worker/asynq_worker.go::buildOrderFulfillmentEmailPayload`：优先父订单 fulfillment.payload（trim），否则拼接子订单 `"[<OrderNo>]\n<payload>"`，跳过 nil/空白，块之间 `"\n\n"`。`order_status_email_queue.go::enqueueOrderStatusEmailTaskIfEligible`：通过 `OrderRepository.ResolveReceiverEmailByOrderID`（user_id=0 取 guest_email，否则取 users.email）解析收件人，空或 `telegram_*@login.local`（大小写不敏感）→ 跳过；查询失败则仍入队（降级）。worker 消费时再次检查占位邮箱。所有入队点（取消、超时取消、paid、completed、delivered、父订单）统一走该函数。
  - (2) `BotNotifyPayload` 增 `event_type`（order_fulfilled / wallet_recharge_succeeded），`buildBotNotifyRequestURL` 解析 CallbackURL 必须有 scheme+host，重写 path 为 `/internal/order-fulfilled` 或 `/internal/wallet-recharge-succeeded`，清空 query/fragment，签名路径与之对应；未知事件跳过；`telegramidentity.IsPlaceholderEmail`（前缀 telegram_ + 后缀 @login.local，大小写不敏感）在 sendTextEmail、状态邮件入队、worker 三处跳过；`isTelegramPhotoAttachment`（jpg/jpeg/png/webp/gif 或 mime image/*）用 sendPhoto，其余 sendDocument；Channel 鉴权失败统一 401 `channel_client_unauthorized`（禁用为 403），头名 `Dujiao-Next-Channel-Key/Timestamp/Signature`。
- **对我们实现的要求**:
  - (1) 实现统一的 `enqueue_order_status_email_if_eligible`；占位邮箱判定 `lower(trim(email))` 以 `telegram_` 开头且以 `@login.local` 结尾；worker 侧也做防御性检查。邮件交付内容拼接使用真实换行。
  - (2) 所有发信入口统一过滤占位邮箱；Bot 通知 URL 用解析+重建而非字符串拼接；鉴权失败不区分具体原因。
- **必测用例**:
  - (1) 收件人 "telegram_123@login.local" → skipped=true；"   " → skipped；"buyer@example.com" → 入队；查询出错 → 入队；子订单 [DJ-CHILD-01: "  SECRET-01  ", DJ-CHILD-02: nil, DJ-CHILD-03: "    ", DJ-CHILD-04: "L1\nL2"] → "[DJ-CHILD-01]\nSECRET-01\n\n[DJ-CHILD-04]\nL1\nL2"；父订单有 payload 时仅返回父订单 payload。
  - (2) 用户邮箱 TELEGRAM_123@login.local → 不调用 SMTP 且返回成功；CallbackURL="https://bot.x/cb?a=1" + 充值事件 → 请求 https://bot.x/internal/wallet-recharge-succeeded；错误 key 与错误签名 → 同样的 401 响应体。

#### NTF-07 上游交付（upstream）订单误发“待人工交付”通知

- 提交: 3784e474 2026-07-02
- 严重度: **低**
- 问题现象: 订单/子订单状态为 fulfilling 且商品 fulfillment_type=upstream（由采购流程自动交付），支付后 `enqueueOrderPaidAsync` 仍发送 manual_fulfillment_pending 提醒。
- 根因: 只用 `status == fulfilling` 判断是否需要人工交付。
- 原修复方式: 新增 `hasManualFulfillmentItems(order)`：任一 item 的 fulfillment_type 归一化后为 manual（空串视为 manual）才返回 true；父订单和每个子订单都加此条件。
- **对我们实现的要求**: “待人工交付”通知的触发条件 = 订单 fulfilling 且至少含一个 manual 类型商品项；upstream/auto 不触发。
- **必测用例**: nil → false；仅 upstream → false；仅 auto → false；manual → true；fulfillment_type="  " → true；upstream+manual 混合 → true。

#### NTF-08 支付订单告警按时间窗口统计并限制发送间隔

- 提交: 48fbad76/b67c3e0c 2026-05-04
- 严重度: **低**
- 问题现象: 待支付订单数、支付失败数告警原先复用仪表盘"今日"累计数据，每次触发都会推送，告警刷屏，且累计数据不代表当前异常。
- 根因: 告警口径与节流机制缺失。
- 原修复方式: 新增 `GetPaymentOrderAlertCounts(now - PaymentOrderAlertCheckSeconds, now)`，统计窗口内父订单 pending_payment 数和线上支付 failed 数，与阈值比较；发送前用 `SetNX("notification:payment_order_interval:{type}", ttl=interval)` 节流；库存类告警与支付类告警分开配置。
- **对我们实现的要求**: Rust 告警按独立的检查窗口和发送间隔实现，节流 key 按告警类型区分。
- **必测用例**: 间隔 600 秒内连续触发两次，只发送一条；窗口外的旧 pending 订单不计入。

#### NTF-09 Telegram Bot 内置菜单缺项回填

- 提交: 31078ffa 2026-04-28
- 严重度: **低**
- 问题现象: 老数据中 bot 菜单只有 3 项内置菜单（shop_home/my_orders/contact_support），后台看不到 my_wallet/affiliate/gift_card/switch_language 的开关，管理员无法开启或隐藏这些菜单。
- 根因: 默认 seed 只有 3 项，读取时也没有补齐缺失项。
- 原修复方式: `builtinMenuKeysOrder` 固定 7 项（shop_home, my_orders, my_wallet, affiliate, gift_card, switch_language, contact_support），各有多语言默认 label。`ensureBuiltinMenuItems` 在读取（GetTelegramBotConfig）和保存（normalizeMenuItems）时补齐缺失 key：新补的项默认 enabled=true，order 接在当前最大值之后；已有项保留原来的 enabled/label/order。
- **对我们实现的要求**: 读取设置时与默认结构做合并，缺失的内置项回填默认值，已存在的字段不能被覆盖。
- **必测用例**: 老配置中 my_orders.enabled=false，读取后仍为 false，并且 affiliate 被补齐且 enabled=true；默认配置有 7 项且顺序正确。

#### NTF-10 交付内容过大时订单邮件改为附件

- 提交: 298f2bb8 2026-03-29
- 严重度: **低**
- 问题现象: 大量卡密直接写进邮件正文时，邮件过大、被拒收或排版混乱。
- 根因: 没有区分正文和附件。
- 原修复方式: 超过 20 行（`FulfillmentPayloadMaxEmailLines`）时，正文不放交付内容，而是生成 multipart/mixed 邮件，附件为 `order_{order_no}_delivery.txt`（text/plain，base64），正文追加 `fulfillment_attachment_tip`（模板中可配置）。占位邮箱（Telegram 生成的）直接跳过。
- **对我们实现的要求**: 使用 lettre 的 MultiPart 构造（它会自动把 base64 按 76 字符折行。原实现把整段 base64 写成一行，超过 SMTP 规定的 998 字符行长上限，可能被部分服务器拒收，不要照搬）。附件文件名用 RFC 2231 编码。阈值为 20 行。
- **必测用例**: ①21 行 payload 时邮件带附件，正文不含卡密；②20 行时内嵌在正文；③附件解码后与 payload 完全一致；④收件人为占位邮箱时不发送。

#### NTF-11 拆单父订单通知变量需聚合子订单商品

- 提交: 8023d717 2026-03-23, ae4de74b 2026-03-23, 6a3d7d2f 2026-03-23
- 严重度: **低**
- 问题现象: 父订单拆分为多个子订单（按交付类型 upstream/manual 等）时，父订单 `Items` 为空，通知中心模板的 `items_summary`/`fulfillment_items_summary` 渲染为"暂无商品明细"，管理员收到的通知没有商品信息。
- 根因: `buildOrderNotificationPayload` 只读 `order.Items`。
- 原修复方式: 构建 payload 前调用 `fillOrderItemsFromChildren(order)`；payload 增加 customer_email/customer_label/customer_type、items_summary（"标题 / 规格 xN [交付类型]"）、各交付类型计数、payment_channel（钱包全额支付时补 wallet/balance）。
- **对我们实现的要求**: 所有面向订单的通知/邮件/Bot 渲染，若父单 items 为空则汇总子单 items；钱包全额支付的"支付渠道"变量不能为空。
- **必测用例**: 父单无 items、两个子单分别 upstream x1、manual x2 → items_summary 同时包含两行；钱包全额支付 → payment_channel="wallet/balance"。

## 18. 数据库/迁移/并发/事务/SQLite/Postgres 差异（DB，10 条）


#### DB-01 事务内一律使用事务连接（SQLite 单连接池自锁/死锁）；行锁事务内不做无关读取；外部请求设超时

- 提交: 3176b9c9 2026-02-22; 9afcc4e2 2026-04-21; d171e6be 2026-05-15, f54ee787 2026-06-01; 041c0e40 2026-08-05, f9409e10 2026-08-12
- 严重度: **高**
- 合并条目: (1) 事务内读取必须使用事务连接，否则单连接池自锁；外部支付请求要有超时 ｜ (2) SQLite 下事务内再用另一个连接读库导致死锁 ｜ (3) SQLite 单连接下事务内使用非事务句柄导致死锁；关键读取加行锁 ｜ (4) 事务内禁止经由非事务连接读库（SQLite 单连接池死锁）
- 问题现象:
  - (1) SQLite（MaxOpenConns=1）下 `CreatePayment` 在 `DB.Transaction` 中用非事务的 `s.channelRepo.GetByID` 取渠道，需要第二个连接，而唯一连接被事务占用 → 永久等待（死锁）。微信支付 HTTP 调用无超时，ctx 为 background 时可无限挂起并持有事务。
  - (2) SQLite 部署下，后台手动退款和退到钱包请求卡死（事务死锁）。
  - (3) 生产 SQLite max_open_conns=1，service 在事务回调内通过非 tx 的 repo/DB 或外部服务再申请连接 → 永久阻塞；卡密发货、订单支付/退款读取无行锁，PG 并发下可重复发货。Redis SetNX 改用 SET NX 后，key 已存在时返回 redis.Nil，需要视为 false 而非错误。
  - (4) (1) issue#271：下单事务内 `CheckPendingOrderAllowed` 调用 settings 服务读取风控配置，settings 仓储使用独立的 db 连接；SQLite `MaxOpenConns=1` 时事务占住唯一连接，第二次取连接永久阻塞，下单卡死。(2) 退款手续费计算在事务内通过 `s.payments.ListByOrderID`（非事务仓储）查支付，同样在单连接下死锁。
- 根因:
  - (1) 仓储未绑定 tx；外部调用无默认超时。
  - (2) `AdminManualRefund` / `AdminRefundToWallet` 在 `orderRepo.Transaction(func(tx){...})` 内部调用 `settingService.GetOrderRefundConfig()`，它用的是全局 db 而不是 tx。SQLite 只能有一个写者，或者连接池 max_open=1，于是事务持有锁/连接，另一个查询在等待 → 互相等待。
  - (3) 事务边界内混用连接；缺少 SELECT ... FOR UPDATE。
  - (4) 事务中混用"事务句柄"和"全局连接池"的仓储。
- 原修复方式:
  - (1) `PaymentChannelRepository.WithTx(tx)`；`CreatePayment` 事务内 `channelRepo := s.channelRepo.WithTx(tx)`；`wechatpay.withDefaultTimeout(ctx)`：ctx 无 deadline 时 `WithTimeout(15s)`，用于 CreatePayment/QueryOrderByOutTradeNo/VerifyAndDecodeWebhook。
  - (2) 把读取配置挪到事务开始**之前**。
  - (3) 所有事务内写操作走 repo.WithTx(tx)；新增 GetByIDForUpdate / GetByIDForUpdateWithChildren（Preload Items/Children/Children.Items）、FindByOrderIDForUpdate（发货防重复）、ListAvailableByProductForUpdate、UpdateFieldsWhereWalletPaid（wallet_paid_amount>0 才退回余额，乐观锁）、UpdateChildrenStatus；并发测试 TestSQLiteConcurrentTransactionsDoNotDeadlock（MaxOpenConns=1，N goroutine 并发 Recharge 5 秒内完成且余额正确）；HTTP 与 asynq 任务 panic 恢复中间件（断连类 panic 只 warn 不写响应）。cache.SetNX 对 redis.Nil 返回 (false,nil)。
  - (4) 风控：`CheckOrderAllowed`（事务外）读取配置并放入 `CheckResult.ConfigSnapshot`，事务内 `CheckPendingOrderAllowed(input, prepared, gate)` 只用快照，不再读 settings；下单（ConsumeRateLimit=true）时读配置失败 fail-closed 返回错误，预览时 fail-open。退款：事务外 `loadPaymentFeeRefundSnapshot` 预取支付快照，事务内校验根订单 ID 未变（`paymentFeeRefundRootOrderID(order) != snapshot.rootOrderID → ErrOrderFetchFailed`）后使用。测试用 `SetMaxOpenConns(1)` + 超时检测死锁。
- **对我们实现的要求**:
  - (1) sea-orm 中事务内所有查询必须使用 `&txn`（DatabaseTransaction）而不是 `&db`；SQLite 连接池 max=1 时要有集成测试覆盖。所有外部 HTTP（支付网关）请求设置默认超时（15s），尽量不要在持有数据库事务时发起网络请求。
  - (2) Rust/sea-orm 中，事务闭包内的所有 DB 访问必须使用传入的 `&DatabaseTransaction`，不能调用会用全局 `DatabaseConnection` 的 service 方法。配置类数据在事务外预先读好。SQLite 连接池配置与 busy_timeout 要统一规划。可以加 lint 或代码审查规则：service 方法接收 `&impl ConnectionTrait`。
  - (3) sea-orm 中事务闭包内只能使用 &DatabaseTransaction，禁止使用全局 DatabaseConnection；事务内不得调用外部 HTTP；SQLite 连接池 max=1 时要有并发测试；PG/MySQL 关键读取用 lock_exclusive()；Redis SET NX 返回 Nil 视为未设置成功；axum 加 CatchPanicLayer，返回统一 500 JSON。
  - (4) sea-orm 中凡在 `txn` 闭包内的所有查询必须使用 `&txn`（`DatabaseTransaction`），绝不能用 `AppState.db`；配置类数据在事务开始前读取并以快照传入。CI 中对 SQLite 用 max_connections=1 跑集成测试以发现此类死锁。
- **必测用例**:
  - (1) SQLite max_connections=1 下创建支付（含渠道查询）在 5 秒内完成不挂起；模拟网关不响应 → 15 秒左右返回超时错误。
  - (2) 用 SQLite（池大小为 1）跑退款集成测试，在超时限制内完成；并发发起两个退款请求，不死锁，并且其中一个因超额被拒绝。
  - (3) SQLite 单连接 20 并发充值 5 秒内全部成功、余额正确；同一订单并发两次发货 → 仅 1 条 fulfillment；SET NX 已存在 key → Ok(false)。
  - (4) SQLite 连接池大小 1，开启风控后下单、带手续费返还的手动退款、切换手续费返还开关：均在 2~5 秒内完成不阻塞；风控配置读取失败时下单返回错误、预览仍正常。

#### DB-02 SQLite 时间范围查询：TEXT 格式与参数格式不一致导致统计缺漏

- 提交: c6d539ed 2026-09-11
- 严重度: **中**
- 问题现象: SQLite 中 created_at 以 TEXT 存储为 `2026-09-10 14:03:07+00:00`（空格分隔），GORM 绑定 time.Time 参数为 RFC3339（`T` 分隔），字符串比较 `' ' < 'T'` 导致当天/边界数据被漏算；仪表盘总览、趋势、商品/渠道排行、利润、退款冲回、新用户数都受影响（issue #301）。
- 根因: SQLite 无原生时间类型，按字符串比较。
- 原修复方式: dashboard/gormstore/sql.go `timeRangeQuery(db, column, start, end)`：Postgres 保持 `col >= ? AND col < ?`；其它（SQLite）改为 `datetime(col) >= datetime(?) AND datetime(col) < datetime(?)`，参数 `Format(time.RFC3339Nano)`。所有 dashboard 时间范围查询改用它。
- **对我们实现的要求**: sea-orm 在 SQLite 下时间列存储格式必须全局统一（建议统一存 UTC、固定格式），并且所有范围查询在 SQLite 下要么保证参数与存储格式一致，要么用 `datetime()` 包裹；MySQL/Postgres 走原生比较。写一个统一 helper 供所有模块（不止 dashboard）使用。注意 datetime() 会截断到秒且会把带时区的值转成 UTC。
- **必测用例**: SQLite 下插入 created_at=`2026-09-10 00:00:00+00:00` 的订单，查询区间 [2026-09-10T00:00:00Z, 2026-09-11T00:00:00Z) → 计入；[2026-09-09, 2026-09-10) → 不计入；三种数据库结果一致。

#### DB-03 迁移补外键：先检查孤儿行；SQLite 重建表会丢索引需重跑 AutoMigrate；Preload 需过滤软删除

- 提交: 1581d986 2026-07-26
- 严重度: **中**
- 问题现象: cart_items 对 products/product_skus、procurement_orders 对 orders 缺少外键；直接创建外键在存在孤儿行时失败；SQLite 的 Create/DropConstraint 通过重建表实现，会丢失原有索引；返利/分销列表 `Preload("Order")`、`Preload("Profile")` 会把已软删除的关联带出来。
- 根因: 迁移缺失与 GORM/SQLite 行为。
- 原修复方式: migrations.go `ensureCartForeignKeyConstraints`：缺约束时先 `LEFT JOIN ... WHERE target.id IS NULL` 统计孤儿行，>0 则报错拒绝；创建 `fk_cart_items_product`/`fk_cart_items_sku`；无论是否新建都再 AutoMigrate 恢复索引。`ensureProcurementOrderForeignKeyConstraint`：删除旧名 `fk_procurement_orders_local_order_reference`，建 `fk_procurement_orders_local_order`（ON UPDATE/DELETE NO ACTION），再 AutoMigrate。`Preload("Order", "deleted_at IS NULL")` 等。reseller 迁移移入统一注册表。
- **对我们实现的要求**: sea-orm 迁移中建立上述外键；SQLite 下变更约束后校验索引存在；所有关联加载（find_also_related/loader）都要过滤软删除。
- **必测用例**: 迁移前插入引用不存在商品的购物车行 → 迁移报出孤儿数量；SQLite 迁移后 cart_items 索引仍在；软删除订单后返利列表不显示其订单信息。

#### DB-04 ORM 零值陷阱：bool=false / 0 被默认值吞掉或被后续整行 Save 覆盖

- 提交: ffc6d9be 2026-06-16; d1b0f1f9 2026-03-28; a551e8f8 2026-06-05
- 严重度: **中**（中（商品本应下架却在分销站上架销售）；中）
- 合并条目: (1) ORM 零值 bool 被默认值吞掉：新建 is_listed=false 的分销商品配置被存成 true ｜ (2) 布尔字段 DB 默认值为 true 的零值陷阱（映射商品被意外上架） ｜ (3) Telegram 新用户默认会员等级被后续 Save 用零值覆盖
- 问题现象:
  - (1) `UpsertSetting` 新建记录时 IsListed=false，GORM 对带 `default:true` 的字段在零值时省略该列，DB 写入默认值 true。
  - (2) products.is_active 定义为 `gorm:"default:true"`。导入上游商品时代码设置 `IsActive=false`（希望默认下架，等管理员确认），但 gorm 在 Create 时会忽略零值字段，于是使用了数据库默认值 true，映射商品一导入就上架销售（价格和库存还没确认）。
  - (3) findOrCreateTelegramUser 创建用户后 AssignDefaultLevel(user.ID) 直接写库，但内存 user.MemberLevelID 仍为 0；调用方随后 Update(Save) 整行保存，把 member_level_id 覆盖回 0，Telegram 用户没有默认等级。
- 根因:
  - (1) GORM Create 忽略零值字段并使用列默认值。
  - (2) Go 零值与 DB default 冲突。
  - (3) 全字段 Save 覆盖其它流程写入的字段。
- 原修复方式:
  - (1) `Create` 改为 `Select("*").Create`，并在 IsListed=false 时额外 `Update("is_listed", false)` 兜底。
  - (2) 默认值改为 `default:false`。
  - (3) 分配后 GetByID 刷新，把 MemberLevelID 同步到内存对象。
- **对我们实现的要求**:
  - (1) sea-orm ActiveModel 插入时所有带 DB 默认值的 bool/数值字段必须显式 `Set(...)`，不要依赖 `NotSet`；对“默认 true 的开关”写回读测试。
  - (2) sea-orm ActiveModel 在插入时显式 `Set(false)`，不依赖 DB 默认值。迁移中 is_active 的默认值要与业务预期一致。导入的上游商品默认下架。
  - (3) sea-orm 更新时只 set 变更列（ActiveModel 仅 Set 修改字段），不要把整个 Model 转 ActiveModel 全量写回；创建用户后需要默认等级的所有路径（邮箱注册、OAuth、Telegram、后台创建）结果一致。
- **必测用例**:
  - (1) 首次保存 SKU 级设置 is_listed=false → 重新读取 is_listed=false；分销站该 SKU 不可见、下单被拒。
  - (2) 导入上游商品后 products.is_active=false；手动创建商品时传 is_active=false，落库也是 false。
  - (3) Telegram 首次登录创建用户 → 数据库 member_level_id = 默认等级 ID；随后更新头像/昵称不改变等级。

#### DB-05 时间统一存 UTC；按日分组要按方言转换时区

- 提交: ce3d0310 2026-03-30, 69fc3327 2026-04-01
- 严重度: **中**
- 问题现象: gorm 默认 NowFunc 使用本地时区，不同部署时区写入的 created_at 混杂，SQLite 按字符串比较时区间查询出错。仪表盘趋势原来把所有行拉到内存再按天聚合，改成 SQL 分组后需要把 UTC 转换为展示时区：PG 用 `TO_CHAR(col AT TIME ZONE 'Asia/Shanghai','YYYY-MM-DD')`；SQLite 不支持命名时区，用 refTime 的固定偏移 `strftime('%Y-%m-%d', col, '+8 hours')`。原实现的 default 分支就是 SQLite 语法，没有 MySQL 分支，在 MySQL 上会出错。另外有夏令时的时区用固定偏移会有误差。
- 根因: 时间写入口径不统一；SQL 方言差异。
- 原修复方式: `NowFunc: time.Now().UTC()`。新增 `dateGroupExpr(db,column,loc,refTime)`。多个 COUNT 合并成一条带 `SUM(CASE WHEN ...)` 的查询。
- **对我们实现的要求**: 所有时间戳以 UTC 写入（chrono::Utc），使用 timestamptz 或 datetime(UTC)。按日聚合时为三种方言各写一个表达式：PG 用 `AT TIME ZONE`；MySQL 用 `DATE_FORMAT(CONVERT_TZ(col,'+00:00','+08:00'),'%Y-%m-%d')`（没加载时区表时只能用偏移量）；SQLite 用 strftime 加偏移。拼进 SQL 的时区名必须校验（白名单或 chrono-tz 解析），防止注入。
- **必测用例**: ①UTC 2026-03-30 17:00 的订单在 Asia/Shanghai 下归到 03-31；②三种数据库上趋势接口返回的天数和计数一致；③时区参数 `UTC'); drop` 被拒绝。

#### DB-06 后台订单列表：count 复用查询、商品关键字 SQL 列名错误、排序注入白名单

- 提交: f462e7cb 2026-03-29, 8119565f 2026-03-31, 9dee934b 2026-03-31, 6c5af297 2026-03-31
- 严重度: **中**
- 问题现象: ①f462e7cb 新增商品关键字搜索时引用了不存在的列 `order_items.product_title`（实际列是 title_json），并且外层子查询别名与条件不匹配（两处都用同一个 cond，别名分别是 oi 和 oi2），搜索时 SQL 报错。②`query.Count(&total)` 之后在同一个 gorm 链上继续 Offset/Limit/Preload，gorm 链式状态污染会导致分页或 count 结果异常。③排序参数 sort_by 直接拼进 ORDER BY 有注入风险。
- 根因: 手写 SQL 片段与表结构不一致；查询构建器状态复用。
- 原修复方式: `buildLocalizedLikeCondition` 分别针对 `oi.title_json` 和 `oi2.title_json` 各生成一份条件和参数。父订单匹配条件为 `id IN (父订单直接的 items) OR id IN (子订单 items 对应的 parent_id)`。Count 和数据查询分别使用 `query.Session(&gorm.Session{})`。`resolveAdminOrderSort` 白名单只允许 created_at/updated_at/total_amount，方向只能是 asc/desc，默认 `id desc`。
- **对我们实现的要求**: sea-orm 中 count 和分页分别从同一个 Select 克隆（`.clone()`）后执行。多语言 JSON 字段的 LIKE 必须按方言生成（PG 用 `title_json->>'zh-CN'`，MySQL 用 `JSON_EXTRACT`，SQLite 用 `json_extract`），带 OR 的条件整体加括号后再和其他过滤条件 AND。排序字段用枚举白名单。
- **必测用例**: ①按中文商品名关键字搜索，在三种数据库上都能返回包含该商品（含子订单商品）的父订单；②关键字 + status 过滤同时生效，OR 条件没有逃出括号；③第 2 页数据与 total 正确；④sort_by="id;drop table" 时回退为 id desc。

#### DB-07 ORM 列名映射陷阱：SKUID 被映射为 s_k_uid

- 提交: 18b68b81 2026-03-28
- 严重度: **中**（中（库存告警一直不正确））
- 问题现象: 仪表盘 `GetStockStats`、`GetInventoryAlertItems` 用临时 struct `{ProductID; SKUID; Total}` 接收 GROUP BY product_id, sku_id 的结果。gorm 的命名策略把 `SKUID` 映射为 `s_k_uid`，扫描时对不上 `sku_id` 列，SKUID 全部为 0，各 SKU 的卡密计数全部归到 0，低库存和售罄告警出错。
- 根因: ORM 的隐式命名规则。
- 原修复方式: 加 `gorm:"column:sku_id"`。
- **对我们实现的要求**: sea-orm 的 FromQueryResult 或自定义 select 使用显式列别名（`column_as`）和显式字段名。SQL 聚合结果的每个字段都要有单测覆盖，确保非零值被正确扫描。
- **必测用例**: 两个 SKU 各有 3 张和 0 张可用卡密，统计结果按 sku_id 正确区分，阈值 5 时两个都在低库存列表中且 sku_id 正确。

#### DB-08 启动迁移幂等标记与 SQLite WAL/busy_timeout

- 提交: e3363df5 2026-03-27, 66d9dedc 2026-03-25
- 严重度: **中**
- 问题现象: (1) 每次启动都跑 SKU 回填/完整性校验、category parent 回填等数据迁移，大库启动很慢；(2) SQLite 默认 journal 模式 + 无 busy_timeout，并发写（回调+worker+下单）时 SQLITE_BUSY 自旋，CPU 飙高、请求失败。
- 根因: 数据迁移无完成标记；SQLite 连接参数缺省。
- 原修复方式: settings 表写 `migration/product_sku_v1`、`migration/category_parent_v1` = {done:true, migrated_at}，已完成则跳过；SQLite DSN 追加 `_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)&_pragma=synchronous(NORMAL)` 并在连接后再 Exec 一遍。
- **对我们实现的要求**: 数据修复类迁移用 sea-orm-migration 的版本表天然幂等；SQLite 连接（sqlx SqliteConnectOptions）设置 journal_mode=WAL、busy_timeout=5s、synchronous=NORMAL、foreign_keys=ON，且写连接池考虑单写者。
- **必测用例**: 连续两次启动，第二次不执行回填 SQL；SQLite 下 20 个并发写事务（下单+回调）无 "database is locked" 错误。

#### DB-09 跨方言 SQL：PostgreSQL 无 json_extract/date 返回类型差异，SQLite JSON 路径含 '-' 需加引号

- 提交: 70a0aaa4 2026-02-20
- 严重度: **中**
- 问题现象: PostgreSQL 执行 `json_extract(title_json, '$.zh-CN')` 报 `SQLSTATE 42883 function does not exist`（商品/文章/Banner 搜索、仪表盘 Top 商品），仪表盘按 `date(created_at)` 分组返回的 day 类型不一致导致扫描失败。
- 根因: 直接写 SQLite 专有函数。
- 原修复方式: `repository/sql_dialect.go`：`jsonTextExprByDialect`：postgres → `(col::jsonb ->> 'key')`，sqlite → `json_extract(col, '$."key"')`（键加引号避免 `-`）；`localizedJSONCoalesceExpr` = `COALESCE(zh-CN, zh-TW, en-US, '')`；`buildLocalizedLikeCondition` 构造 `name LIKE ? OR <json expr> LIKE ?...`；仪表盘用 `CAST(date(created_at) AS TEXT)` 分组。新增 postgres 集成测试。
- **对我们实现的要求**: 多语言 JSON 字段搜索/排序按后端（SQLite/MySQL/Postgres）生成表达式：SQLite `json_extract(col,'$."zh-CN"')`，MySQL `JSON_UNQUOTE(JSON_EXTRACT(col,'$."zh-CN"'))`，Postgres `col::jsonb ->> 'zh-CN'`；按日期分组统一转文本 'YYYY-MM-DD'。三种数据库都跑集成测试。
- **必测用例**: 三种数据库上以 "测试" 搜索商品标题（title_json.zh-CN）能命中；仪表盘近 7 天订单趋势返回 day 字符串 "2026-02-20" 格式。

#### DB-10 跨库 LIKE 大小写差异与后台查询参数校验

- 提交: a6645ea0 2026-02-26, 4d0289fe 2026-02-26, 2b96b2e6 2026-03-11, e1b3fd8b 2026-03-06
- 严重度: **低**
- 问题现象: Postgres 下 `LIKE` 区分大小写，SQLite/MySQL 默认不区分，同一搜索在不同库结果不同；后台支付列表 `order_id=bad`/`channel_id=abc` 被静默忽略返回全量；按用户筛选支付时用 INNER JOIN orders，钱包充值支付（order_id=0）被漏掉；用户/订单关键字无法搜到第三方账号（Telegram username/provider_user_id）；采购单 `created_to=2026-03-06` 以 `created_at <= '2026-03-06'` 比较，当天记录被排除。
- 根因: 方言差异未抽象；参数解析宽松；JOIN 类型错误。
- 原修复方式: `likeOperatorByDialect`：postgres → `ILIKE`，其余 `LIKE`（`buildLocalizedLikeConditionByDialect` 覆盖普通列与多语言 JSON 列）；`buildAdminPaymentFilter` 非法数字返回错误 → 响应 status_code 400；`ListAdmin` 用户筛选改为 `LEFT JOIN orders` + `LEFT JOIN wallet_recharge_orders ON payment_id` 且 `(orders.user_id=? OR wallet_recharge_orders.user_id=?)`；用户/订单关键字增加 `EXISTS (SELECT 1 FROM user_oauth_identities WHERE provider/provider_user_id/username LIKE ?)`；列表支持 `SkipCount/Lightweight`。
- **对我们实现的要求**: sea-orm 查询封装一个跨库大小写不敏感匹配（PG 用 ILIKE 或 lower() 双边）；多语言 JSON 字段搜索三库各自实现；admin 查询参数用强类型解析，非法即 400；日期上界按"次日 00:00 之前"(<) 处理。
- **必测用例**: PG 中搜索 "abc" 命中 "ABC"；`?order_id=bad` → 400；按 user_id 查支付包含其充值支付；关键字为 Telegram username → 命中该用户及其订单；created_to=当天 → 包含当天 23:59 的记录。

## 19. 前端（storefront/admin）UI/交互（FE，26 条）


#### FE-01 自动跳转收银台必须在当前标签页打开；wap/page 交互模式按跳转处理

- 提交: 1b761b91 2026-08-07
- 严重度: **中**
- 问题现象: 创建支付异步返回后调用 `window.open` 打开收银台，已脱离用户点击事件，被浏览器当弹窗拦截；支付宝 wap/page 模式被当成二维码模式展示。
- 根因: 异步上下文中打开新窗口；只识别 'redirect' 一种跳转模式。
- 原修复方式: paymentResumePolicy.ts：`REDIRECT_PAYMENT_INTERACTION_MODES = {redirect, wap, page}`；`resolvePaymentLinkNavigationTarget(automatic)`：自动跳转 → `window.location.assign(payLink)`（当前标签页，也保留 sessionStorage），用户点击 → 新窗口；充值详情同理；标题/标签按 wap/page 区分。
- **对我们实现的要求**: Vue 前端自动跳转支付一律 location.assign；只有用户手势触发时才 window.open；interaction_mode 判定集合包含 redirect/wap/page，其余按 qr。
- **必测用例**: interaction_mode=wap 创建支付后自动在当前页跳转；qr 模式显示二维码；手动点击"打开支付页"用新窗口。

#### FE-02 购物车本地缓存价格需随商品刷新同步

- 提交: 7d694695 2026-08-05
- 严重度: **中**
- 问题现象: 后台修改商品价格后，购物车（localStorage 持久化）仍显示旧价（issue#268）。
- 根因: 刷新库存快照时只更新库存，未更新价格与批发阶梯。
- 原修复方式: frontend/user/src/utils/cartPricingSnapshot.ts `resolveCartPricingSnapshot(product, sku)`：总是用最新 `product.wholesale_prices`（无则置 undefined 清空旧阶梯）；`sku.price_amount` 转分后 >0 才覆盖 priceAmount（round 到 2 位，'12.345'→'12.35'），无效/0/负数保留旧值；在 `refreshCartStockSnapshots` 中合并。注释：活动价/会员价/券仍以服务端结算预览为准。
- **对我们实现的要求**: 前端购物车打开/刷新时拉取商品最新价与批发阶梯覆盖本地缓存；最终金额始终以后端预览为准（后端下单不得信任前端价格）。
- **必测用例**: 缓存价 10，后台改为 12 → 打开购物车显示 12；商品删除批发阶梯 → 购物车阶梯被清空；接口返回价格 '0' → 保留旧价。

#### FE-03 SPA 部署：index.html no-cache、hash 资源强缓存、保留路径 404、内嵌 SPA 路由兜底、运行时 base

- 提交: 048d8688 2026-07-27, 7ec666fa 2026-07-26, 02d7f901 2026-07-26, 87cc68b5 2026-07-26; 7cb67985 2026-02-23, 592135a6 2026-02-23, 8dfa66d3 2026-02-23, bfd06ec0 2026-02-23; 9a7a34d9/afa26c2a 2026-04-25
- 严重度: **中**
- 合并条目: (1) SPA 入口 no-cache、hash 资源强缓存；保留路径 404；后台路径校验与运行时 base ｜ (2) 发版后旧 chunk 404：index.html 禁止缓存、assets 长缓存、Cloudflare Rocket Loader 排除 ｜ (3) 单二进制内嵌 SPA 的路由兜底
- 问题现象:
  - (1) (1) 内嵌 SPA 的 index.html 无 Cache-Control，被 CDN（如 Cloudflare）长期缓存，升级后浏览器仍加载旧 chunk，与新后端契约错配（如旧前端仍用查询参数传游客凭据）。(2) `/api/xxx` 不存在的接口被 SPA fallback 返回 200 index.html，客户端 JSON 解析报错。(3) 可配置的后台路径 web.admin_path 若包含 `:`、`*` 等 Gin 路由元字符，会变成动态路由吞掉前台路径或启动 panic。(4) 后台前端的 base 路径在构建期写死，无法随配置变化。
  - (2) 新版本发布后用户浏览器/CDN 缓存旧 index.html，请求已不存在的旧 hash chunk（404），页面白屏；Cloudflare Rocket Loader 改写 `type="module"` 脚本导致加载异常。
  - (3) fullstack 模式下访问 admin 深层路由并刷新，资源解析为 `/admin/orders/admin/assets/...`，返回 404（base href 不是绝对路径）；admin_path 配置成 `/api` 或 `/` 会与 API 或用户端 SPA 冲突。
- 根因:
  - (1) 缓存头缺失；fallback 范围过大；路径未校验。
  - (2) index.html 被缓存；module 脚本被 CF 异步改写。
  - (3) base 占位符替换时没有加前导 `/`；admin_path 没有校验。
- 原修复方式:
  - (1) web/handler.go：index 与非 assets/ 文件 `Cache-Control: no-cache`；`assets/` 下 `public, max-age=31536000, immutable`；`reservedPaths = ["/api", "/uploads", "/health"]`，命中前缀（等于或以 `r+"/"` 开头）且未匹配路由 → 404 纯文本；`ValidateAdminPath` 每段必须匹配 `^[A-Za-z0-9._~@-]+$`，拒绝空段、`.`、`..`；admin index.html 中 `__DJ_ADMIN_BASE__` 在启动时替换为实际路径，前端 `adminBase.ts` 运行时读取。
  - (2) nginx.conf：`location ^~ /assets/ { try_files $uri =404; Cache-Control "public, max-age=31536000, immutable" }`；`location = /index.html { Cache-Control "no-store, no-cache, must-revalidate" }`；index.html 脚本加 `data-cfasync="false"`；vite 插件 `transformIndexHtml` 给所有 `<script type="module"` 加 `data-cfasync="false"`（user 与 admin 两个前端）。
  - (3) 把 `__DJ_ADMIN_BASE__` 替换为 `"/"+trim(prefix)`；`ValidateAdminPath`：必须以 / 开头、不能是 /、不能以 / 结尾，也不能与 /api、/uploads、/health 相同或互为前缀。admin 挂在 `prefix/*filepath`：文件存在就返回文件，否则返回 index.html；用户端 SPA 挂在 NoRoute，同样规则。原实现中 NoRoute 会让未匹配的 `/api/...` 也返回 200 + index.html，index.html 也没有设置 no-cache。
- **对我们实现的要求**:
  - (1) Rust 内嵌静态资源服务实现同样的缓存策略与保留前缀 404；admin 路径配置同样白名单校验；前端运行时解析 base。
  - (2) 如果 Rust 后端托管 SPA：`/assets/*` 不存在返回 404（不要 fallback 到 index.html），带 immutable 长缓存；index.html 及 SPA fallback 响应 `Cache-Control: no-store`。构建产物 module script 带 `data-cfasync="false"`。可选：前端捕获动态 import 失败后刷新页面。
  - (3) Rust 中 axum `fallback` 对 `/api/*` 未匹配的请求必须返回 JSON 404，不能回落到 SPA；index.html 响应设置 `Cache-Control: no-cache`，带哈希的 assets 设置长缓存；admin 路径配置按上述规则校验。
- **必测用例**:
  - (1) GET / → Cache-Control: no-cache；GET /assets/index-abc.js → immutable；GET /api/not-exist → 404 非 HTML；admin_path=`/:x` → 启动失败。
  - (2) GET `/assets/not-exist.js` → 404（非 index.html）；GET `/` 与 `/products/x` → Cache-Control 含 no-store；构建后 index.html 中 script 有 data-cfasync="false"。
  - (3) GET /admin/orders/123 返回 index.html 且 base 为 "/admin/"；GET /api/v1/not-exist 返回 JSON 404；admin_path="/api/x" 时启动失败。

#### FE-04 提现提交成功后刷新失败被误判为“提现失败”诱导重复提交；加载失败被显示为“暂无数据”

- 提交: 396cd6d8 2026-06-21
- 严重度: **中**（中（可导致重复提现申请））
- 问题现象: `submitWithdraw` 成功后 `Promise.all([loadDashboard, loadBalances, loadWithdraws])`，任一刷新请求失败即 reject，调用方提示失败，用户再次提交。订单列表/详情请求失败时显示“暂无数据/订单不存在”。
- 根因: 成功后的“尽力刷新”与主操作共用错误通道；load 函数未兜底。
- 原修复方式: 刷新改用 `Promise.allSettled`；各 load* 内部 catch 并降级为空；订单列表/详情新增 `error`/`detailError` 状态展示后端本地化消息与“重试”按钮；金额统一用 `formatResellerConsoleAmount(amount, currency)` 格式化。
- **对我们实现的要求**: 前端写操作成功后的附带刷新必须与写操作结果解耦（allSettled / 独立 try）；区分“加载失败”与“空数据”两种状态。后端提现接口也应有幂等保护。
- **必测用例**: mock applyWithdraw 成功、balanceAccounts 500 → UI 提示成功、按钮恢复、不抛未处理 rejection；orderDetail 请求 500 → 显示加载失败+重试，而非“订单不存在”。

#### FE-05 收银台“切换支付方式”后又被自动恢复上一笔支付并反复跳转网关

- 提交: 4d8bad45 2026-06-15, ab29b5c3 2026-05-27, 8ed97eb0 2026-05-26
- 严重度: **中**
- 问题现象: Payment.vue 中点“更换支付方式”调用 resetPayment()，它把 latestLoaded 置 false，下一次 loadOrder 又调用 loadLatestPayment() 恢复刚才那笔 redirect 支付并 openPayLinkInCompatibleWindow()，用户永远切不到其它渠道，一直被拉回第三方收银台。另外 loadOrder 中 `loading=false` 在取消/过期提前 return 路径外，异常时 loading 卡住（ab29b5c3 改成 try/finally）。BEPUSDT 回跳参数 `bepusdt_return` 不在 paymentReturnMarkers 里，回跳后不识别为“支付返回”。
- 根因: reset 逻辑不区分原因（路由切换 vs 用户主动换渠道）；自动打开支付链接的条件散落多处。
- 原修复方式: 新增 utils/paymentResumePolicy.ts：`getPaymentResetPolicy('change_payment_method')` => {resumeLatestPayment:false, clearSelectedChannel:true, stopActivePaymentWatch:true}（停止轮询/倒计时/取消 debounce）；路由切换 => resumeLatestPayment:true。restoreCachedPayment 只恢复轮询+capture，不自动开收银台（autoOpenPayLink:false）。`shouldAutoOpenPaymentLink` 仅 interaction_mode==='redirect' 且 pay_url.trim()!=='' 时为真。loading 放入 finally。markers 补 bepusdt_return。
- **对我们实现的要求**: Vue3 收银台实现显式的 reset 原因枚举；用户主动换渠道时设置 latestLoaded=true（不再恢复最近支付）、清空所选渠道、停止轮询；仅 redirect 模式且 pay_url 非空才自动打开；从缓存恢复不自动弹窗。所有 return marker 列表覆盖全部网关（epay/alipay/wechat/epusdt/bepusdt/tokenpay/okpay/pp/stripe/dujiaopay）。
- **必测用例**: 1) 创建 redirect 支付 → 点更换方式 → 断言未再次 open 窗口且渠道选择清空、轮询停止；2) interaction_mode='qr' 或 pay_url='   ' 不自动打开；3) 带 `?bepusdt_return=1` 回跳被识别；4) loadOrder 抛错后 loading=false。

#### FE-06 自定义导航和外链：URL 要限制协议

- 提交: eb506d8f 2026-03-28, 79805ada 2026-03-28, 5d8d124e 2026-03-28, 01739c46 2026-04-01, e2e84e7c 2026-04-01, 5e55a226 2026-04-01
- 严重度: **中**（中（存储型 XSS 面，需要后台权限才能写入））
- 问题现象: 新增 `nav_config`（builtin 开关 blog/notice/about + custom_items）。后端 `normalizeNavConfig` 限制最多 10 项、标题 120 字、URL 2000 字，link_type 只允许 internal/external，target 只允许 _self/_blank，但不校验 URL 协议。前台 Navbar 用 `<a :href="item.path">` 渲染外链，`javascript:alert(1)` 会被原样输出，点击后执行。站点描述 site_description 按多语言归一化，前台用插值渲染（安全）。
- 根因: 只限制了长度和枚举，没有限制协议。
- 原修复方式: （原项目仅做了长度和枚举归一化，加 `rel="noopener noreferrer"`。）
- **对我们实现的要求**: 后端归一化时 external 链接只允许 `http://`、`https://`，internal 链接必须以 `/` 开头且不能以 `//` 开头；其他情况丢弃该项。前端渲染时再做一次协议检查。所有用户或管理员配置的文本一律用插值渲染，禁止 v-html 或 innerHTML。public config 缓存在设置更新时失效。
- **必测用例**: ①保存 url=`javascript:alert(1)` 的 external 项，该项被丢弃；②internal url=`//evil.com` 被拒绝；③第 11 个 custom_item 被截断；④builtin.blog=false 时前台导航不显示博客。

#### FE-07 Telegram SDK 按需加载，防止被墙地区整站白屏

- 提交: 2014b231 2026-03-27, c3e8ab7a 2026-03-22
- 严重度: **中**
- 问题现象: index.html 同步引入 `https://telegram.org/js/telegram-web-app.js`，在无法访问 telegram.org 的地区脚本阻塞/超时，storefront 长时间白屏或无法加载。
- 根因: 第三方阻塞脚本放在 head。
- 原修复方式: 移除 head 中的 script；`isTelegramUrlEnvironment()`（location.hash 包含 tgWebAppData/tgWebAppVersion/tgWebAppPlatform）时才动态插入 script，单例 Promise，失败则忽略 Telegram 功能；`init()` 改为 async，完成后 `app.mount`。
- **对我们实现的要求**: 不在 index.html 放任何第三方阻塞脚本；按环境动态加载且必须有失败兜底与超时（原实现 mount 等待 script onload，没有超时，建议加 3s 超时）。
- **必测用例**: 模拟 telegram.org 不可达、URL 无 tgWebAppData → 页面正常渲染且不请求 telegram.org；带 tgWebAppData 且脚本加载失败 → 页面仍渲染。

#### FE-08 立即购买/快速购买：不影响购物车、合并"下单并支付"接口、数量限制

- 提交: d1a2c8eb 2026-03-25, 6f94616a 2026-03-25, ec890e36 2026-03-25, 66d9dedc 2026-03-25, 3ca13fdc 2026-03-25, 2b4e423a 2026-03-25, 827aae56 2026-03-26
- 严重度: **中**
- 问题现象: 需要在不破坏购物车的前提下"立即购买"；数量可超过库存/限购。
- 根因: 新功能。
- 原修复方式: 独立 `buyNow` store，Checkout 通过 `?mode=buynow` 只结算该单品，下单成功只清 buyNow（不清购物车）；新增 `POST 订单并支付` 合并接口：先 CreateOrder，channel_id=0 且不使用余额则只返回订单；支付创建失败时返回 order + `payment_error`（订单保留 pending 可稍后去支付页继续）；返回 pay_url/qr_code/expires_at 等。数量选择器上限 = min(max_purchase_quantity, SKU 可用库存)，输入非法→1，超限→截断，限值变化时回收；epay 渠道仅显示 wechat/wxpay/alipay/qqpay。
- **对我们实现的要求**: 合并接口的订单创建与支付创建分两个事务，支付失败不回滚订单但要返回可继续支付的信息；不要把内部 err 原文（`err.Error()`）直接返回给前端，应映射为 i18n 错误码；前端数量限制仅是体验，后端必须再校验 max_purchase_quantity 与库存。
- **必测用例**: 购物车有 2 件，立即购买 1 件并下单 → 购物车仍为 2 件；channel 配置错误时合并接口返回 order_no 与错误码且订单为 pending；quantity 超过 max_purchase_quantity → 后端 400。

#### FE-09 编辑支付渠道时 watch 覆盖已保存的交互模式

- 提交: 9124e5d2 2026-03-21
- 严重度: **中**
- 问题现象: 打开已有渠道编辑弹窗时，provider_type 的 watch 触发 `form.interaction_mode = pickDefaultInteractionMode()`，把已保存的 redirect 重置为默认 qr，管理员未察觉保存后支付方式被改。
- 根因: watch 在初始化回填时也执行重置。
- 原修复方式: 仅当当前 interaction_mode 不在该 provider 允许列表中时才重置为默认值。
- **对我们实现的要求**: 表单联动字段"仅在值非法时"才重置；编辑回填不触发副作用（或用 `immediate:false` + 回填标记）。
- **必测用例**: 编辑 interaction_mode=redirect 的 epay 渠道，不改任何字段直接保存 → 仍为 redirect；把 provider 从 epay 切到 stripe → mode 被重置为 stripe 支持的值。

#### FE-10 使用服务器时间校正倒计时与推广码过期

- 提交: d9af7998 2026-03-17, 40b7c78c 2026-03-17
- 严重度: **中**
- 问题现象: 客户端时钟偏差（快/慢几分钟）导致支付页倒计时错误（显示已过期或迟迟不过期）、推广码本地缓存 TTL 判断错误。
- 根因: 前端直接用 `Date.now()`。
- 原修复方式: 公共配置接口返回 `server_time`（毫秒，缓存命中时也要实时写入）；前端 offset = server_time + RTT/2 - 响应时刻；`getServerTime()` 用于支付倒计时、过期判断、affiliate 过期。
- **对我们实现的要求**: `/public/config` 每次响应带实时 server_time（不能被缓存成旧值）；前端所有与后端时间比较的逻辑用校正时间。
- **必测用例**: 客户端时钟快 10 分钟、支付单剩余 5 分钟 → 页面仍显示约 5 分钟而非"已过期"；连续两次获取 config（命中缓存）server_time 不同。

#### FE-11 支付回跳参数解析：兼容 `&amp;` 被转义的 query 与 out_trade_no

- 提交: fd1af4cf 2026-02-23, 532eccd6 2026-02-23
- 严重度: **中**
- 问题现象: 部分网关（易支付等）回跳 URL 把 `&` 转义为 `&amp;`，得到 `?order_no=X&amp;guest=1&amp;epay_return=1`，前端读 `route.query.guest` 为空 → 游客订单被当会员、不触发状态检查；部分网关只回 `out_trade_no` 不带 `order_no`；数组形式 query 未处理；watch 未监听全部参数导致不刷新。
- 根因: 直接读取 `route.query.xxx`。
- 原修复方式: `Payment.vue` 新增 `readRouteQueryValue(key)`：依次尝试 `key`、小写 key、`amp;key`、`amp;小写key`，再遍历所有 key 去掉前缀 `(amp;)+` 做大小写不敏感匹配；数组取首个非空；`readRouteQueryFlag` 接受 1/true/yes；`orderNo` 回退 `out_trade_no`；`pp_return/token/payer_id|PayerID/stripe_return/session_id/epay_return` 全部通过该函数；watch 改为 `route.fullPath`。
- **对我们实现的要求**: Vue3 支付结果页实现同样的容错 query 读取工具并有单测；后端生成回跳 URL 时不要 HTML 转义 `&`。
- **必测用例**: URL `/payment?order_no=DJ1&amp;guest=1&amp;epay_return=1` → orderNo=DJ1、isGuest=true、epayReturn=1；`?out_trade_no=DJ2` → orderNo=DJ2；`?guest=yes` → true。

#### FE-12 金额计算用整数分，避免浮点误差

- 提交: d537b6d2 2026-02-22
- 严重度: **中**
- 问题现象: 前端用 `Number` 计算手续费、钱包抵扣与在线支付额（如 `base*rate/100`、`balance - total`），出现 0.1+0.2 类误差，显示金额与后端不一致（如 手续费 10.005 四舍五入差 1 分）。
- 根因: JS 浮点。
- 原修复方式: `src/utils/money.ts`：`amountToCents`（字符串按 `^[+-]?\d+(\.\d+)?$` 解析为整数分，第三位小数 ≥5 进位）、`rateToBasisPoints`、`centsToAmount`、`calculateFeeCents = round(base*bp/10000)`；Cart/Checkout/Payment/订单详情等全部改用。
- **对我们实现的要求**: 前端所有金额运算使用整数分（或 decimal 库），展示前再格式化；最终金额以后端返回为准。后端用 rust_decimal，round(2) 使用与原实现一致的舍入（shopspring decimal Round 为 half away from zero → rust_decimal `RoundingStrategy::MidpointAwayFromZero`）。
- **必测用例**: amountToCents("10.005") = 1001；calculateFeeCents(1000 分, 150 bp) = 15；余额 "0.30"、总额 "0.10"+"0.20" 计算钱包抵扣=30 分、在线支付=0。

#### FE-13 vault 模板需渲染后台自定义导航与页脚配置

- 提交: d98a78a1 2026-07-26, 898e284a 2026-09-03, 2feb2aab 2026-09-10
- 严重度: **低**
- 问题现象: vault 模板忽略后台配置的自定义导航/页脚；登录后仍显示"游客查单"入口；手机菜单按钮内 SVG 拦截点击导致点击区域失效。
- 根因: 模板未接入配置；缺条件渲染；图标未设 pointer-events。
- 原修复方式: 抽出 `useNavConfig` 供默认与 vault 模板共用；游客查单入口 `v-if="!userAuthStore.isAuthenticated"`；菜单图标加 `pointer-events-none`。
- **对我们实现的要求**: TSX 模板复用同一导航配置 composable；已登录隐藏游客查单入口；按钮内图标不拦截点击。
- **必测用例**: 后台新增导航项 → vault 顶栏出现；登录后顶栏/更多菜单/页脚无游客查单链接。

#### FE-14 游客订单凭据存储受限（隐私模式/禁用存储）时仍可打开订单详情

- 提交: a3450c06 2026-07-29
- 严重度: **低**
- 问题现象: 浏览器禁用 storage 时访问 `window.sessionStorage` 本身即抛异常，游客订单详情页无法打开。
- 根因: 在 try 之外访问 storage 对象。
- 原修复方式: guestOrderAuth.ts：通过 `window[storageName]` 在 try 内访问；增加模块内存变量 `volatileGuestOrderAuth` 作为兜底；只存 sessionStorage，不回退 localStorage（避免长期保存凭据），并迁移清除 localStorage 旧值；详情页状态机 loading/auth/detail/empty。
- **对我们实现的要求**: 前端所有 storage 访问封装 try/catch 并有内存兜底；游客凭据只放 sessionStorage。
- **必测用例**: 模拟 sessionStorage getter 抛异常 → 输入凭据后能查看详情。

#### FE-15 后台杂项功能性 bug：支付渠道列表链/币种显示、OKPay 编辑回填、批发价分页、TG MiniApp 检测

- 提交: 35ed880a 2026-06-15, 98f76fb2 2026-06-14, 61e8fbd5 2026-05-18, 2707324f 2026-06-14, 0a014581 2026-05-16
- 严重度: **低**
- 问题现象: PaymentChannels 列表 bepusdt/okpay 显示错误（应取 config_json.trade_type / coin）；编辑 OKPay 渠道时 channel_type 为空未从 config.coin(usdt/trx) 回填，保存后类型错；BEPUSDT 在支付记录/用户详情的 provider 下拉缺失；WholesalePrices.vue 分页事件名用错（@change/@page-size-change vs 组件的 @change-page/@change-page-size）导致翻页无效，且刷新时清空行闪烁；Telegram MiniApp 参数可能在 query 而非 hash，initData 在 SDK ready 后延迟出现，导致误判非 MiniApp。
- 根因: 前端对各 provider 配置字段的解析分散。
- 原修复方式: paymentChannelDisplay.ts 统一解析；分页事件修正+边界判断；MiniApp 检测同时看 search+hash，并在 1.5s 内每 100ms 轮询 initData。
- **对我们实现的要求**: Vue3 TSX 中 provider 显示/回填集中到一个工具模块；分页组件事件名统一；MiniApp 检测兼容 query 与延迟 initData。
- **必测用例**: okpay 配置 {coin:"TRX"} 编辑回填 channel_type=trx；列表显示 TRX；批发价页点第 2 页触发请求；URL `?tgWebAppData=...` 被识别为 MiniApp。

#### FE-16 首次访问 UI 语言与商品多语言取值不一致

- 提交: 945c1592 2026-06-14
- 严重度: **低**
- 问题现象: vue-i18n 用 detectLocale()（localStorage → 浏览器语言）而 appStore.locale 默认 'zh-CN'，英文浏览器首访 UI 英文、商品标题中文。
- 根因: 两处语言检测逻辑不同。
- 原修复方式: 导出 detectLocale 并让 app store 复用。
- **对我们实现的要求**: 前端全局只有一个 locale 来源。
- **必测用例**: 清空 localStorage、navigator.language='en-US' → i18n 与商品 getLocalizedText 均取 en-US。

#### FE-17 个人中心订单统计卡片只统计当前页

- 提交: efd62683 2026-06-05, 76641c2e 2026-06-05
- 严重度: **低**
- 问题现象: “待支付/已完成”数量由 orders.value（当前页 20 条且受状态筛选影响）filter 计算，数量错误；个人中心订单总数取 recentOrders.length。
- 根因: 前端用分页数据做全量统计。
- 原修复方式: 新增 GET /orders/stats 与 /wallet/recharges/stats：`SELECT status, COUNT(*) ... WHERE user_id=? AND parent_id IS NULL [AND order_no LIKE ?] GROUP BY status`（不应用状态筛选与分页），返回 {total, by_status}；finished=delivered+completed+partially_refunded+refunded；总数取 pagination.total。
- **对我们实现的要求**: 提供按状态聚合接口，仅统计父订单、仅复用关键词筛选；前端卡片使用该接口。（后续分销商版本还需加租户 scope。）
- **必测用例**: 用户 25 单（3 待支付分布在第 2 页）→ stats.pending_payment=3；带 status 筛选时 stats 不变；子订单不计入。

#### FE-18 shadcn/reka Select 不能用空字符串作为选项值；Checkbox 的 indeterminate 值

- 提交: 5a5c0762 2026-04-26, 9b292c55 2026-04-26
- 严重度: **低**
- 问题现象: 迁移到 shadcn-vue 后，筛选下拉框中 `<SelectItem value="">全部</SelectItem>` 报错或无法选中；Checkbox 的 `update:modelValue` 可能传出 'indeterminate'，被当作 true 处理，导致批量选择出错。
- 根因: reka-ui 的 Select 保留空字符串值用于清空；Checkbox 的值是三态。
- 原修复方式: 用哨兵值 `'__all__'`，请求时转换为 undefined；`toggleArrayMember(arr, item, v)` 只在 `v===true` 时加入，其余情况移除。
- **对我们实现的要求**: Vue3 TSX 中筛选"全部"项使用哨兵值，发请求前映射为 undefined；三态复选框显式处理。
- **必测用例**: 选择"全部"后请求参数中不包含该字段；点击 indeterminate 状态的复选框后，列表选择状态正确。

#### FE-19 购物车库存快照：结算前刷新、未知库存不当 0；最终以服务端下单事务为准

- 提交: 356f58dd 2026-02-23; 9d2802d5 2026-04-06
- 严重度: **低**
- 合并条目: (1) 购物车库存快照：结算前刷新库存并阻止超量提交 ｜ (2) 结账提交前的二次库存同步被移除
- 问题现象:
  - (1) 购物车 localStorage 中的库存数据是加入时的旧值（且缺失字段被归一为 0），结算时显示可买但后端报库存不足，或误判为 0 库存。
  - (2) Checkout 提交前调用 `syncCartStockSnapshots()` 多发一轮请求，提交变慢；库存以服务端下单时的锁定为准。
- 根因:
  - (1) 购物车只保存加入时的库存快照、无刷新。
  - (2) 前端重复校验。
- 原修复方式:
  - (1) `utils/cartStock.ts::refreshCartStockSnapshots`：按 slug 拉取商品详情，匹配 SKU（按 id → sku_code → 唯一启用 SKU），patch 库存字段与 `skuStockSnapshotAt`；cart store 中库存字段缺失保持 undefined 而非 0（`normalizeOptionalStockNumber`）；Checkout 在同步中/任一项超库存时禁用提交、不请求 preview，并高亮提示。
  - (2) 删除提交前的同步调用。
- **对我们实现的要求**:
  - (1) 进入结算页先刷新库存快照；未知库存视为"不限制"而非 0；超量时禁止提交；后端仍需最终校验库存。
  - (2) 库存正确性只依赖服务端下单事务（条件更新或行锁），前端快照仅作提示。
- **必测用例**:
  - (1) 购物车数量 5，刷新后 SKU 可用 3 → 结算按钮禁用并提示；旧购物车数据无库存字段 → 不被判为缺货。
  - (2) 前端快照过期（显示有货），但服务端已无库存时，下单返回库存不足且不产生订单。

#### FE-20 fetch 客户端的 401 处理与超时

- 提交: 570c06b3 2026-04-01, 721c485b 2026-04-01, a7e9d303 2026-03-30
- 严重度: **低**
- 问题现象: 从 axios 迁移到 fetch 后，需要保留以下行为：HTTP 401 或业务 status_code=401 时清除 `user_token`、`user_profile` 并跳转到 /auth/login；但 login、register、telegram 登录、forgot-password 这些认证接口本身返回 401 时不能跳转，否则登录失败提示会丢失。请求要有超时（AbortController，10s）；非 JSON 响应要按 HTTP 状态码给出提示；blob 下载需要单独处理错误。
- 根因: 迁移时容易漏掉拦截器逻辑。
- 原修复方式: 用 `isAuthEndpoint` 正则排除认证接口；`X-Lang` 请求头传递语言；`silentBusinessError` 选项。
- **对我们实现的要求**: Vue3 TSX 的请求层实现同样的 401 分流逻辑（admin 端同理，使用各自的 token key）。上传大文件的请求不能用 10s 超时（需要单独配置更长时间）。
- **必测用例**: ①登录接口返回 401 时停留在登录页并显示错误；②其他接口返回 401 时清除 token 并跳转；③请求超过 10s 时以网络错误提示结束。

#### FE-21 后台支付渠道弹窗在新建模式下保留了上一次编辑的数据

- 提交: 8d72d127 2026-04-01
- 严重度: **低**
- 问题现象: 先编辑渠道 A 再点"新建"时，由于 watch 只依赖 channelId（从 null 到 null 没有变化），表单没有被重置，新建时带着 A 的配置（包括密钥），可能误保存出一个重复的渠道。
- 根因: 表单重置只挂在 id 变化上，没有挂在弹窗打开事件上。
- 原修复方式: 抽出 `resetFormForCreate()`，并 watch `modelValue`：弹窗打开且 channelId===null 时重置。
- **对我们实现的要求**: 所有"新建/编辑共用"的弹窗在打开时根据模式重新初始化表单，不能依赖 id 变化。
- **必测用例**: 编辑渠道 A → 关闭 → 点新建，所有字段为默认值（provider=epay、fee=0、config 为空）。

#### FE-22 手动交付表单提交内容要按 schema 快照的字段顺序和标签显示

- 提交: 6b47d32f 2026-03-30, 8c907ef1 2026-03-30
- 严重度: **低**
- 问题现象: 订单详情（用户、游客、管理员）中，买家填写的手动表单（manual_form_submission）直接用 `Object.entries(submission)` 显示。JSON 对象的键顺序不可靠（PG jsonb 会重排键，Go map 序列化按字母排序），显示顺序与表单定义不一致；标签取不到时显示 key。
- 根因: 依赖对象键的顺序。
- 原修复方式: 按 `manual_form_schema_snapshot.fields[]` 的顺序输出，标签取本地化的 label；快照中没有的键追加到末尾并显示 key。前端类型增加 `manual_form_schema_snapshot`。
- **对我们实现的要求**: 后端返回订单项时同时带上 schema 快照（下单时保存）。前端 TSX 按 fields 数组顺序渲染。Rust 中 serde_json 默认的 Map 是 BTreeMap（会排序），需要保序时开启 `preserve_order` 或改用数组。
- **必测用例**: 定义字段顺序为 [qq, email, remark]，提交后三个详情页都按该顺序显示对应的本地化标签；多出的键 x 显示在最后。

#### FE-23 弱网下的路由切换加载与重复加载配置

- 提交: 7c8d5fc7 2026-03-22, 4e257a34 2026-03-26, 05f9222c 2026-03-26
- 严重度: **低**
- 问题现象: 弱网下懒加载路由 chunk 期间页面无反馈；chunk 加载失败后 loading 不消失；App.vue onMounted 与 router.beforeEach 重复请求 config；首屏标题闪现 "Dujiao-Next" 品牌。
- 根因: 缺少导航状态管理。
- 原修复方式: beforeEach startNavigating / afterEach stopNavigating / `router.onError` 也 stopNavigating；空闲时按序预热常用路由（saveData/2g 不预热）；config 只在 beforeEach 统一加载；index.html title 置空由配置填充。
- **对我们实现的要求**: 路由错误必须清除全局 loading；config 请求去重（单例 Promise）；不硬编码品牌名到 title/登录页（8278d35b 同类：登录/注册/找回页品牌名取 config.brand.site_name）。
- **必测用例**: 模拟 chunk 404 → loading 消失并提示；首屏只发 1 次 /public/config。

#### FE-24 后台设置子 Tab 只在 setup 时从 props 同步，异步加载的数据不显示

- 提交: 3d2781fc 2026-03-09
- 严重度: **低**
- 问题现象: 拆分出的 SettingsCaptchaTab / SettingsNotificationTab / SettingsSMTPTab 在父组件数据异步返回前已执行 `syncFromProps()`，表单显示默认空值；管理员保存会把空配置覆盖到服务器。
- 根因: 只同步一次，不监听 props 变化。
- 原修复方式: 各 Tab 增加 `watch(() => props.data, syncFromProps, { deep: true })`。
- **对我们实现的要求**: TSX 组件从 props 初始化本地可编辑表单时必须 watch props（或在父组件数据就绪后再渲染子组件，`v-if="loaded"`），防止用空表单覆盖。
- **必测用例**: 模拟设置接口延迟 500ms 返回 SMTP host=smtp.x.com → 渲染后输入框显示 smtp.x.com；未加载完成时保存按钮禁用。

#### FE-25 vue-i18n 消息中的 `{{ }}` 等特殊字符须转义；语言检测与 X-Lang 头

- 提交: 5808b326 2026-02-26, bb6fa53e 2026-03-07, 16e03550 2026-03-07, 3c321458 2026-03-05
- 严重度: **低**
- 问题现象: 通知模板提示文案 `可用变量示例：{{user_id}} ...` 被 vue-i18n 当作插值语法解析报错/渲染为空；首次访问总是 zh-CN，不跟随浏览器语言；后端错误信息始终中文（请求不带语言）；商品 SEO keywords/description 为多语言对象时 meta 输出 `[object Object]`。
- 根因: i18n 消息语法冲突；未传递 locale。
- 原修复方式: 文案改为 `{'{{user_id}}'}` 字面量写法；`detectLocale()`：localStorage → navigator.language 精确匹配 → zh 前缀含 TW/HK/Hant → zh-TW，否则 zh-CN；en → en-US；默认 zh-CN；用户端与后台 axios 请求拦截器注入 `X-Lang: <locale>`；SEO meta 先 `getLocalizedText(seoMeta.x)` 再回退字符串。
- **对我们实现的要求**: i18n 文案中出现 `{ } @ $ |` 须用字面量插值转义；所有 API 请求带 X-Lang，后端 i18n 解析以 X-Lang 优先；多语言 JSON 字段在任何输出点（含 head meta）都要本地化。
- **必测用例**: 渲染含 `{'{{order_no}}'}` 的文案 → 页面显示 `{{order_no}}`；navigator.language='zh-HK' 首访 → zh-TW；切换 en-US 后下单失败 → 错误信息为英文。

#### FE-26 支付二维码缺失时回退为支付链接；钱包充值页同理

- 提交: 358d0517 2026-02-27, 549353d3 2026-03-05
- 严重度: **低**
- 问题现象: interaction_mode=qr 但网关只返回 `pay_url` 不返回 `qr_code`（部分易支付/epusdt）时支付页空白无法支付；无在线渠道但可用余额支付时页面显示"无可用支付方式"。
- 根因: 仅以 qr_code 字段决定展示。
- 原修复方式: `qrDisplayContent = qr_code || (mode==='qr' ? pay_url : '')`，回退时提示 `qrFallbackHint`；充值面板 mode 非 redirect 时同样回退；无渠道且可用余额 → 显示 `channelEmptyUseBalance`/`walletPayOnly`。
- **对我们实现的要求**: 支付页与充值页共用一个"支付展示内容"计算函数，qr 缺失时用 pay_url 生成二维码并保留跳转按钮；余额可覆盖时允许仅余额支付。
- **必测用例**: 返回 {interaction_mode:'qr', qr_code:'', pay_url:'https://p/x'} → 渲染 pay_url 的二维码与提示；渠道列表为空、余额充足 → 显示余额支付入口而非错误。

## 20. 其他（MISC，9 条）


#### MISC-01 运行时密钥弱/默认/重复一律拒绝启动；HTTP 服务器设置超时

- 提交: 4e4d5bbc 2026-07-27
- 严重度: **高**
- 问题现象: 非 release 模式下弱密钥只告警；多个用途的密钥（JWT、用户 JWT、加密密钥、游客凭据密钥等）配置相同值不被发现；`http.Server` 无 ReadHeaderTimeout 等，易受 Slowloris。
- 根因: 校验宽松。
- 原修复方式: cmd/server/main.go 弱密钥在任何模式都 `Fatalf`；新增重复检测：任意两项 trim 后相同即都判弱。http_service.go：ReadHeaderTimeout 10s、ReadTimeout 30s、WriteTimeout 60s、IdleTimeout 120s、MaxHeaderBytes 1MB。
- **对我们实现的要求**: 启动时校验所有密钥强度（长度/非默认值）且互不相同，否则退出；axum/hyper 配置 header 读取超时、请求超时、keepalive 超时与最大 header 大小。
- **必测用例**: 两个密钥配置相同 → 启动失败并列出两者名称；默认示例密钥 → 启动失败。

#### MISC-02 前台 API 必须用白名单 DTO：成本价、自增 ID、管理员备注等敏感字段曾经泄露

- 提交: 6a26d080 2026-03-28, 1c5f70b5 2026-03-30, 91c55d8e 2026-03-30, 1c3998fa 2026-03-30, 35558197 2026-03-31, e4b02e63 2026-03-31, 91212a50 2026-03-31, d0e3f1b7 2026-03-31, ea26759f 2026-04-01
- 严重度: **高**（高（商业机密和数据泄露））
- 问题现象: 新增 cost_price 后，前台商品/SKU 接口和订单接口直接序列化 model，用户抓包能看到 `cost_price_amount`，订单项里能看到 `cost_price`。第一次修复在嵌入结构的 View 上加了一个同名字段 `CostPriceAmount models.Money json:"-"`。Go encoding/json 会先把带 `-` 标签的字段排除掉，然后更深一层嵌入的同名字段重新可见，所以成本价仍然输出。1c5f70b5 改成用 `*struct{} json:",omitempty"` 这个 nil 字段来遮蔽。随后项目整体改为 DTO 白名单（dto/order.go、product.go、user.go、wallet.go、affiliate.go、gift_card.go、banner.go、post.go、payment.go、cart.go），明确排除 PasswordHash、TokenVersion、Status、ProviderPayload、ProviderRef、GatewayOrderNo、FeeRate（充值单后来又需要 FeeRate，重新加入）、AffiliateCommission 的 BaseAmount/RatePercent、Banner 的 Name/IsActive/StartAt、GiftCard 的 BatchID 等。同时前台不再暴露 order.id 自增主键：路由从 `/orders/:id` 改为 `/orders/:order_no`，创建支付请求改为 `order_no`，`/payments/latest` 按 order_no 查询（防止枚举订单量或 IDOR）。新加的 `users.admin_note` 字段带着 json tag，只能通过 DTO 避免泄露。
- 根因: 直接序列化 ORM model，新增字段默认公开。
- 原修复方式: 如上：统一的 `dto.NewXxxResp` 白名单构造，并用单测断言敏感字段不在 JSON 中（dto/*_test.go："敏感字段不应出现"）。
- **对我们实现的要求**: Rust 端前台和 channel 接口一律使用独立的 Resp struct（不要直接对 sea-orm Model 派生 Serialize 后返回）。每个 Resp 写单测：序列化后的 key 集合等于允许列表。前台路由和参数只用 order_no、recharge_no 等不可枚举的业务号，不接受自增 id。admin_note、cost_price、provider_payload、guest_password 永不出现在前台响应中。
- **必测用例**: ①GET /public/products/:slug 的 JSON 中不存在 `cost_price_amount` 键（包括 skus[]）；②用户订单详情 items[] 中不存在 `cost_price`，fulfillment_type=upstream 显示为 manual；③用户资料中没有 password_hash、admin_note、token_version；④GET /user/orders/123（数字 id）返回 404；⑤latest payment 响应中没有 provider_payload 和 amount 以外的敏感字段。

#### MISC-03 仪表盘利润/统计口径：折扣不重复扣减、零成本商品、退款冲回成本、时区分桶

- 提交: b6331434 2026-03-27, c91ad875 2026-03-27, 6168db9d 2026-03-27, 7bc829fe 2026-03-30, 7db7fa83 2026-04-03, 0aef1716 2026-03-31; 1d0bf5f9 2026-08-09; 58c68528 2026-03-24, 8f24e2cb 2026-03-24, 99538b1a 2026-03-27, 4a486aa1 2026-03-27, f3b5b75b 2026-03-24
- 严重度: **中**（中（财务报表失真）；中；低）
- 合并条目: (1) 仪表盘利润：promotion_discount 被重复扣减、没有成本价的商品计入成本 0、空区间币种为空 ｜ (2) 仪表盘利润：零成本商品收入不应被排除；退款可按比例冲回成本 ｜ (3) 仪表盘统计口径（时区分桶、启用 SKU、转化率）
- 问题现象:
  - (1) ①收入按 `total_price - coupon_discount - promotion_discount` 计算，但 order_items.total_price 本来就是按活动价算出的小计，promotion_discount 被扣了两次（测试中 paid_amount 应为 190，原来算出 170）。②没有录入成本价（cost_price=0）的商品也计入收入，利润被高估；原来还用 `parent_id IS NULL` 过滤，漏掉了挂在子订单上的 items。③所选时间范围内没有订单时 currency 为空，前端金额单位显示错误。
  - (2) 利润查询带 `order_items.cost_price > 0` 过滤，成本为 0 的商品收入完全不计入营收，利润偏低；退款只冲减收入不冲减成本。
  - (3) (1) 趋势图用 SQL `date(created_at)`（UTC）分桶，东八区凌晨订单被算到前一天；(2) 手动库存统计用商品级 manual_stock_total，多 SKU 商品/禁用 SKU 库存被算错；(3) 支付转化率 = 成功支付数/订单数（一个订单多次支付会 >100%）；(4) 已支付 GMV 窗口按 paid_at，与订单数窗口不一致；(5) 卡密告警把禁用 SKU 的库存算进商品库存。
- 根因:
  - (1) 金额字段语义不清；统计口径不一致。
  - (2) 错误的过滤条件；缺少成本冲回。
  - (3) 聚合口径不统一、依赖数据库日期函数。
- 原修复方式:
  - (1) 收入改为 `total_price - coupon_discount`。利润概览和趋势只统计 `order_items.cost_price > 0` 的行（不再限制 parent_id）；排行榜中 total_cost 用 `CASE WHEN cost_price>0`。下单时把 `sku.CostPriceAmount` 快照到 order_items.cost_price。商品的 cost_price_amount 取启用 SKU 中的最低成本。区间内没有订单时，币种回退到最近一笔 `currency<>''` 的父订单币种。新增全站用户余额总和 `SUM(wallet_accounts.balance)`。
  - (2) 去掉 cost_price>0 过滤。`getRefundAdjustments`：按退款日和订单聚合退款额，成本基数=订单自身订单项成本+直接子订单成本（父订单无 items），冲回成本=成本基数×退款额/订单 total_amount；设置 `dashboard.accounting.refund_reverses_cost`（默认 false）开启时总成本=成本-冲回成本（|x|<1e-6 视为 0）；缓存 key 带上该开关。后续 c6e59679 又把 `payment_fee_refunded_amount` 纳入手续费冲回。
  - (3) 查询 created_at 后在应用层按请求时区 `value.In(loc).Format("2006-01-02")` 分桶；手动库存按启用 SKU 汇总（任一 unlimited 即无限）；转化率 = PaidOrders/OrdersTotal；GMV 按 created_at 窗口；卡密库存只统计 `sku_id=0 OR sku_id IN (启用 SKU)`，sku_id=0 的遗留库存归入第一个启用 SKU；新增 SKU 维度缺货/低库存计数。
- **对我们实现的要求**:
  - (1) 明确文档化 order_item 的金额语义：unit_price 为活动或会员价后的单价，total_price=unit_price×qty，只有 coupon_discount 是另外分摊的。收入 = total_price - coupon_discount。成本使用下单时的快照（cost_price×qty），不回查当前商品成本。统计金额用 Decimal（SQL 中 SUM 后按字符串解析），不要用 f64。
  - (2) 利润统计不得过滤零成本行；退款冲回成本按父+子订单成本基数比例计算；缓存键包含影响口径的开关。
  - (3) 跨 SQLite/MySQL/PG 的日期分桶在 Rust 侧按请求时区做；库存告警只看启用 SKU；转化率用订单维度。
- **必测用例**:
  - (1) ①单价 100、活动价 95×2、优惠券 10 → 收入 180（不是 170）；②cost_price=0 的 item 不参与利润计算；③items 在子订单上时仍被统计；④空区间的 currency 取最近订单的币种。
  - (2) 成本 0 售价 10 的订单 → 营收 10、利润 10；订单 100 成本 60 退款 50 且开关开启 → 冲回成本 30。
  - (3) Asia/Shanghai 下 2026-03-24 00:30(+08) 的订单计入 03-24；订单 2 个其中 1 个支付了 2 次 → 转化率 50%；禁用 SKU 有 100 库存、启用 SKU 0 → 告警缺货。

#### MISC-04 后台商品详情必须返回全部 SKU（含禁用）

- 提交: 5a190aae 2026-03-22
- 严重度: **中**
- 问题现象: 后台编辑商品用的 `GetAdminByID` 复用了前台 `GetByID`（只预加载启用 SKU），禁用 SKU 在后台看不到、无法重新启用；保存时提交列表不含禁用 SKU，服务端按 code 匹配/新建，可能新建同码 SKU 冲突或误改。
- 根因: 前台/后台查询共用。
- 原修复方式: 新增 repo `GetAdminByID`：Preload 全部 SKUs，`ORDER BY sort_order DESC, id ASC`。
- **对我们实现的要求**: 后台查询与前台查询分开实现，后台包含禁用 SKU 并返回 is_active；前台只返回启用 SKU。
- **必测用例**: 商品含启用 A、禁用 B → 后台详情返回 2 个且 B.is_active=false；前台详情只返回 A。

#### MISC-05 库存告警覆盖对接(upstream)商品：按 sku_mappings 的上游库存计算

- 提交: a9f64fe9 2026-09-15, 6b669a1f 2026-09-15
- 严重度: **低**
- 问题现象: 开启库存告警后，fulfillment_type=upstream 的商品缺货不告警（先是显式跳过，随后改为支持）。
- 根因: GetInventoryAlertItems 只处理 auto/manual。
- 原修复方式: dashboard gormstore/inventory.go 新增 `collectUpstreamInventoryAlertRows`：通过 `sku_mappings JOIN product_mappings ON pm.id=sm.product_mapping_id WHERE pm.local_product_id IN ? AND 未软删` 取 upstream_stock/upstream_is_active；无启用 SKU 时对所有 upstream_is_active 的映射库存求和（负数按 0）；有 SKU 时逐 SKU 取映射，未映射或上游未启用则跳过；按阈值 classifyInventoryAlertType。5d27b849 仅把 Table("...") 改成 Model(&domain)。
- **对我们实现的要求**: 库存告警对 upstream 商品读取映射表的上游库存；负库存归零；上游未启用的 SKU 不告警。
- **必测用例**: upstream 商品 SKU 映射 upstream_stock=0 且 active → out_of_stock 告警；stock=2 阈值 5 → low_stock；upstream_is_active=false → 不告警。

#### MISC-06 文章分类：后台树缺失禁用分类；禁用分类仍可被新文章挂载

- 提交: 0e03e546 2026-06-25, dab47887 2026-07-06, 152aceee 2026-07-06
- 严重度: **低**
- 问题现象: 后台 `GET /admin/post-categories?tree=1` 调用 `ListActiveTree` 只返回启用分类，被禁用的分类在后台消失无法再启用/编辑；同时文章可以挂载到已禁用分类。
- 根因: 后台与前台共用“仅启用”查询；挂载校验未检查 is_active。
- 原修复方式: 仓储改为 `ListTree()`（全部，按 sort_order, id 排序）供后台使用；公开接口仍只返回启用。`validateCategoryAssignment`：分类不存在 → ErrPostCategoryInvalid；分类禁用且与文章当前分类不同 → ErrPostCategoryInvalid（已挂载后才被禁用的保持不受影响）；分类必须是末级（CountChildren==0）；仅 blog 类型可设置分类，notice 不可；category_id=0 归一化为 nil。
- **对我们实现的要求**: 后台列表接口必须包含禁用数据；“禁止新挂载禁用对象，但保留历史挂载”这一规则在更新时比较新旧 ID。
- **必测用例**: 创建文章挂禁用分类 → 拒绝；文章已挂 A，A 被禁用后更新文章其它字段（category 不变）→ 成功；挂父分类 → 拒绝；notice 带分类 → 拒绝；后台 tree=1 返回含禁用分类。

#### MISC-07 公开博客关联商品需过滤下架商品（原实现遗漏）

- 提交: ec4449a5/b79f5eff/6fc2f771 2026-04-26
- 严重度: **低**
- 问题现象: 新增商品与博客互相关联。`ListRelatedProducts` 只按 post_products JOIN，没有过滤 `products.is_active` 和分类启用状态，公开博客详情会带出已下架商品（标题、slug、价格）；`ListPostsForProduct` 限定 type=blog 且已发布。
- 根因: 新功能遗漏。
- 原修复方式: 关联关系按传入顺序保存 sort，去重并跳过 0（整体替换放在事务中）；`ProductIDs *[]uint` 为 nil 表示不修改，空数组表示清空。
- **对我们实现的要求**: 公开接口返回的关联商品必须走"商品可售"统一过滤（上架、分类启用、未删除）；关联文章只返回已发布的 blog。
- **必测用例**: 关联的商品下架后，博客详情中不再出现该商品；PATCH 文章不带 product_ids 时关联关系不变，带空数组时清空。

#### MISC-08 博客/公告发布时间

- 提交: 8dd3ca52/5d2bff46 2026-04-20
- 严重度: **低**
- 问题现象: 文章发布后 `published_at` 为空，前台显示的是 created_at（草稿创建时间）。
- 根因: Create/Update 没有设置 published_at。
- 原修复方式: 创建时如果 is_published，设置 `PublishedAt=now`；更新时从未发布变为发布且 `PublishedAt==nil` 时设置（再次发布不覆盖原发布时间）。前端显示 published_at，为空时显示空字符串。
- **对我们实现的要求**: 按相同规则实现；公开列表按 published_at DESC 排序。
- **必测用例**: 草稿 → 发布时设置 published_at；下架后再发布时 published_at 不变；直接以发布状态创建时有值。

#### MISC-09 排序规则统一为 sort_order 越大越靠前

- 提交: 09558256 2026-03-19, d3ad83b7 2026-03-19
- 严重度: **低**
- 问题现象: 后台提示"数值越小越靠前"，但仓储查询 `sort_order DESC`，管理员设置与实际相反。
- 根因: 文案与实现不一致。
- 原修复方式: 文案改为"数值越大越靠前"，补测试锁定分类/商品/SKU 均为 `sort_order DESC, id ASC`；默认 SKU = sort_order 最高的启用 SKU。
- **对我们实现的要求**: 分类、商品、SKU 所有列表 `ORDER BY sort_order DESC, id ASC`（ID 作确定性 tie-break）。
- **必测用例**: sort_order=100 与 1 → 100 在前；相同 sort_order → id 小在前。

---

## 21. 回归测试清单

按严重度排序（高 → 中 → 低），同严重度内按“资金/支付 → 订单 → 退款 → 钱包 → 定价 → 分销 → 发货 → 上游 → 认证 → 权限 → 风控 → 上传 → 设置 → 数据库 → 通知 → 前端 → 其他”排序。每行对应上文同 ID 的教训，“关键断言”为该条必测用例的第一条摘要，完整用例见正文。

| # | ID | 严重度 | 模块 | 描述 | 关键断言（摘要） | 状态 |
|---|---|---|---|---|---|---|
| 1 | PAY-01 | 高 | 支付/通用 | 验签常量时间比较、拒绝空密钥、sign_type 不信任请求、商户号必须匹配 | 渠道密钥为空时，用空密钥签名的回调 → 拒绝 | ✅ |
| 2 | PAY-02 | 高 | 支付/通用 | 金额守恒：支付只有覆盖订单当前在线应付额才可履约；欠付入余额；余额全额支付作废遗留在线链接；余额分配轮次幂等键 | 余额5+A在线10 → 切 B（应付15）→ A 回调 10 成功：订单仍待支付、异常码 underpaid、余额 +10（合计 15），重复回调不重复入账，再用余额补齐可完成 | ✅ |
| 3 | PAY-03 | 高 | 支付/通用 | 手续费承担策略快照（none/merchant_absorbed/customer_surcharge/legacy）+ 新链接作废旧链接 | 未开启加收：订单 100、费率 3% → amount=100、fee=3、merchant_absorbed | ✅ |
| 4 | PAY-04 | 高 | 支付/通用 | 回调事实校验（渠道/业务单号/币种/金额）在锁内复核 + 成功回调必须带币种与正金额 | 成功回调缺 currency → 拒绝 | ✅ |
| 5 | PAY-05 | 高 | 支付/通用 | 支付渠道金额区间/角色/会员等级/付款类型限制：服务端强制，写路径（创建支付时）复核 | 渠道 payment_roles=[member] 时游客直接 POST 创建支付返回拒绝 | ✅ |
| 6 | PAY-06 | 高 | 支付/通用 | 分销站/自定义域名下单，支付完成回跳到主站（return_url 未按 tenant 域名生成） | ①主站 tenant、无 tenant、nil ctx → "" | ✅ |
| 7 | PAY-07 | 高 | 支付/通用 | Webhook URL 无 channel_id 时回调全部失败 | 两个 active 微信渠道，回调用第二个渠道密钥加密且无 channel_id → 命中第二个并入账 | ✅ |
| 8 | PAY-08 | 高 | 支付/通用 | Provider 重构引入的参数错位：capture/查单/webhook 被 interaction_mode 校验阻断；跨币种金额未回写 | interaction_mode=qr 的 stripe 渠道 capture 不返回配置错误 | ✅ |
| 9 | PAY-09 | 高 | 支付/通用 | 每笔支付使用独立 gateway_order_no：避免重复发起冲突、不泄露内部 payment_id | ①同一订单连续创建两次 epusdt 支付 → 两次发给网关的 order_id 不同，均以 "DJP" 开头且不等于 `DJP{payment_id}` | ✅ |
| 10 | PAY-10 | 高 | 支付/通用 | 渠道汇率转换：payment 记录网关实际金额，回调按转换后的金额和币种校验 | ①USD 10、rate 7.2、target CNY → 请求网关 72.00 CNY，payment.amount=72.00、currency=CNY，payload 中 original_amount=10.00 | ✅ |
| 11 | PAY-11 | 高 | 支付/通用 | 商品/钱包充值限定支付渠道：服务端按商品取交集校验 | ①商品 A 限制 [1,2]、商品 B 限制 [2,3]，同一订单 channel=2 成功，channel=1 返回 not_allowed_for_product | ✅ |
| 12 | PAY-12 | 高 | 支付/通用 | 渠道固定手续费（fixed_fee）：范围校验与计算口径 | online=100, rate=2.5, fixed=1 → fee=3.50, payable=103.50 | ✅ |
| 13 | PAY-13 | 高 | 支付/通用 | 出站网关请求绑定在入站 HTTP 请求上下文，客户端断开即取消创建/查询 | 模拟网关延迟 2s，客户端在 0.5s 断开 → 网关调用仍完成，payment 记录 pay_url/provider_ref 并为 pending（非 failed） | ✅ |
| 14 | PAY-14 | 高 | 支付/通用 | 删除未签名的通用支付回调入口（任何人可伪造支付成功） | POST `/api/v1/payments/callback` Content-Type: application/json, body `{"payment_id":1,"status":"success"}` … | ✅ |
| 15 | PAY-15 | 高 | 支付/通用 | 按站点币种校验渠道：官方微信/支付宝只能 CNY | site currency=USD，用官方支付宝渠道支付 → ErrPaymentCurrencyMismatch | ✅ |
| 16 | PAY-23 | 高 | 支付/钱包余额支付 | “仅钱包余额支付”模式服务端强制 + 下单前预校验余额；读设置不放在行锁事务里 | ①wallet_only 开启，带 channel_id=1 创建支付返回 wallet_only_payment_required | ✅ |
| 17 | PAY-25 | 高 | 支付/Stripe | checkout.session.completed 以 payment_status 判定成功 | completed 事件 + payment_status=unpaid → 订单保持待支付 | ✅ |
| 18 | PAY-26 | 高 | 支付/Stripe | 换汇渠道回调金额守恒误判 & 应付金额币种展示 | 订单 1 CNY、rate 0.11、回调 0.11 GBP → 足额，订单履约 | ✅ |
| 19 | PAY-28 | 高 | 支付/PayPal | Webhook 验签必须嵌入原始事件字节 | 含中文、`<`、浮点 `10.50` 的事件体 → 发给 PayPal 的 verify 请求体中 webhook_event 与原始字节逐字节相同 | ✅ |
| 20 | PAY-29 | 高 | 支付/PayPal | 目标货币 + 汇率换算 | 订单 72.00 CNY、target=USD、rate=0.1389 → PayPal 下单 10.00 USD | ✅ |
| 21 | PAY-30 | 高 | 支付/PayPal | Webhook 必须强制验签且成功事件必须带合法金额 | ① 配置无 webhook_id → 保存配置报错 | ✅ |
| 22 | PAY-32 | 高 | 支付/微信支付 | 微信支付 API 应答必须验签；支持平台证书与微信支付公钥两种模式 | 伪造应答（错误签名）查单 → ErrSignatureInvalid | ✅ |
| 23 | PAY-33 | 高 | 支付/微信支付 | 微信主动查单用业务订单号导致查不到；前端需主动 capture 微信支付 | 微信 mock 返回空 provider_ref → 落库 provider_ref == gateway_order_no（DJP 前缀）而非 order_no | ✅ |
| 24 | PAY-34 | 高 | 支付/支付宝 | 回调需校验 app_id 归属，防跨商户回调注入 | form 只有有效签名但 `app_id=2026999999999999`（与配置不同）→ 返回 "fail"，订单不变 | ✅ |
| 25 | PAY-35 | 高 | 支付/易支付 | 易支付回调必须校验 pid 归属本渠道商户（防同平台其他商户注入） | (1) V1 合法签名 + pid=1001（配置 1001）→ 成功 | ✅ |
| 26 | PAY-38 | 高 | 支付/OKPay | OKPay 协议升级 HMAC-SHA256 + timestamp/nonce，回调仅接受 JSON，嵌套键用点号 | 用官方示例参数+token 计算签名与期望值一致 | ✅ |
| 27 | PAY-39 | 高 | 支付/OKPay | JSON 回调无法识别 + 汇率为 1 时 currency 与网关币种不一致导致回调被拒 | JSON 回调正确签名 → 支付成功、订单 paid | ✅ |
| 28 | PAY-40 | 高 | 支付/OKPay | 汇率换算下单后，回调金额必须按换算后金额校验 | 订单 100.00、汇率 0.1389 → 下单 amount="13.89000000" | ✅ |
| 29 | PAY-41 | 高 | 支付/epusdt / BEpusdt | epusdt 签名改为 HMAC-SHA256，数值格式与服务端一致 | 参数 amount=100、currency=cny、network=tron、notify_url、order_id=ORD-1、pid=1000、token=usdt，secret sk-test → 签名等于 … | ✅ |
| 30 | PAY-42 | 高 | 支付/epusdt / BEpusdt | 真 epusdt（GMPay）与 BEpusdt 拆分：共用回调入口的特征识别、响应体、历史数据迁移 | 没有 pid 的 body 不被 epusdt handler 接管而交给 BEpusdt | ✅ |
| 31 | PAY-43 | 高 | 支付/epusdt / BEpusdt | 非 paid 状态的回调不得被处理为成功 | 签名正确但 status=1(等待)/3(过期) 的 epusdt 回调 → 不修改 payment/order | ✅ |
| 32 | PAY-46 | 高 | 支付/TokenPay | 法币币种与加密币种混用导致下单/回调金额币种错误 | 订单 10.00 CNY，渠道 currency=USDT_TRC20 → 请求体 Currency=USDT_TRC20、ActualAmount=10.00 | ✅ |
| 33 | PAY-48 | 高 | 支付/DujiaoPay | 新网关 webhook 验签规则 | 正确签名 → 成功 | ✅ |
| 34 | ORD-01 | 高 | 订单/下单与库存 | 取消订单事务内加锁复核状态；用户取消回滚优惠券；游客订单密码 ≥6 位 | 并发执行"取消订单"与"支付回调成功"100 次，最终不存在 status=canceled 且有成功支付未入异常的情况 | ✅ |
| 35 | ORD-02 | 高 | 订单/下单与库存 | 游客订单凭据：HMAC 摘要存储 + 仅通过 Authorization: Guest 头传递 | 创建游客订单后数据库中 guest_password 以 `hmac-sha256:` 开头且 64 位 hex | ✅ |
| 36 | ORD-03 | 高 | 订单/下单与库存 | 手动库存扣减必须检查 rows_affected | 手动库存 1，两个并发下单各买 1 → 1 成功 1 返回 manual_stock_insufficient，库存=0 不为负 | ✅ |
| 37 | ORD-04 | 高 | 订单/下单与库存 | 订单取消/过期后支付记录仍为 pending，可继续支付 | 父订单+2 子订单各有 pending 支付，超时取消 → 3 条支付均 expired | ✅ |
| 38 | ORD-05 | 高 | 订单/下单与库存 | 删除商品：有库存或成交记录禁止删除，否则在事务内级联清理 | ①商品有 1 张 available 卡密时删除返回 400 product_has_stock，数据不变 | ✅ |
| 39 | ORD-06 | 高 | 订单/下单与库存 | 手动库存语义改为"剩余库存"，-1 表示无限（旧语义 0=不限导致可超卖） | total=0 下单 → 库存不足 | ✅ |
| 40 | RFD-01 | 高 | 退款 | 退款手续费按比例冲回（累计法吸收舍入误差）+ 支持历史退款补录 | 支付 100.00 手续费 3.01，第一次退 33.33 → 手续费 1.00，第二次退 66.67 → 2.01（合计 3.01） | ✅ |
| 41 | RFD-02 | 高 | 退款 | 部分退款/全额退款的金额守卫、状态流转、父子订单同步 | 1) 实付 100，先退 30（状态为 partially_refunded），再退 80 被拒绝（超额），再退 70 后状态为 refunded | ✅ |
| 42 | RFD-03 | 高 | 退款 | 管理员退款到余额必须要求订单已支付(paid_at 非空) | 订单 40 元，状态 canceled、paid_at 为空，退款 15 → ErrOrderStatusInvalid，余额与 refunded_amount 不变 | ✅ |
| 43 | WAL-01 | 高 | 钱包/充值/礼品卡 | 充值回调终态矩阵与超时过期任务 | expired 后收 success → 入账 1 次、状态 success | ✅ |
| 44 | PRC-01 | 高 | 优惠券/活动价/批发价/会员价 定价 | 并发下单绕过优惠券总次数/单用户次数限制 | UsageLimit=1，20 个并发请求下单 → 恰好 1 单成功，其余返回 usage_limit 错误，used_count=1 | ✅ |
| 45 | PRC-02 | 高 | 优惠券/活动价/批发价/会员价 定价 | 批发价（商品级→SKU 级）：门槛数量口径、匹配优先级、写入校验、前后端一致 | SKU A 专属 5 件 70、SKU B 专属 5 件 60，各买 5（基价 100）→ 原价 1000、批发优惠 350、合计 650，A 单价 70/优惠 150，B 单价 60/优惠 200 | ✅ |
| 46 | PRC-03 | 高 | 优惠券/活动价/批发价/会员价 定价 | 优惠券“禁止批发价商品使用”与“固定金额券按件抵扣” | 单商品买 5 件命中批发 80 + 券禁批发 → ErrCouponWholesaleDisabled | ✅ |
| 47 | PRC-04 | 高 | 优惠券/活动价/批发价/会员价 定价 | 价格叠加顺序：活动价/批发价取优 → 会员价 → 优惠券；会员累计只在真实状态迁移时原子累加 | 原价 100、批发档 {min5:80}、数量5：活动 10%（90）→ 批发胜，wholesale_discount=100，券 10% → coupon=40，total=360，unit=80 | ✅ |
| 48 | PRC-05 | 高 | 优惠券/活动价/批发价/会员价 定价 | 批发阶梯必须随门槛严格递减；选档取最低单价 | {5:80,10:90} 与 {5:80,10:80} 保存被拒 | ✅ |
| 49 | PRC-06 | 高 | 优惠券/活动价/批发价/会员价 定价 | 会员累计金额与自动升级的并发丢失更新/降级 | 并发 10 次 OnOrderPaid(10) → total_spent=100 | ✅ |
| 50 | PRC-07 | 高 | 优惠券/活动价/批发价/会员价 定价 | 停用的会员等级仍然享受会员价或折扣 | ①等级 VIP 有 SKU 覆盖价 8.00（原价 10），启用时下单单价 8.00 | ✅ |
| 51 | PRC-08 | 高 | 优惠券/活动价/批发价/会员价 定价 | 阶梯活动价匹配与 SKU 级活动价展示 | 规则 [min 0: 95折, min 100: 9折, min 300: 8折]，单价 50：qty1 → 47.50 | ✅ |
| 52 | PRC-09 | 高 | 优惠券/活动价/批发价/会员价 定价 | 防 0 元购：订单总额必须 >0，原价按折前价累计 | ① 商品 10 元 + percent 100 活动 → ErrProductPriceInvalid | ✅ |
| 53 | RSL-01 | 高 | 分销商/租户/域名 | 多次部分退款累计超扣分销利润 | 130/30 订单先退 52 再退 78 → 累计扣减 = -30.00 | ✅ |
| 54 | RSL-02 | 高 | 分销商/租户/域名 | 提现校验只看正数流水导致超额提现；余额缓存重复扣减 withdrawn | +100、-50 两条 available → 申请 80 被拒（ErrResellerWithdrawInsufficient），申请 50 成功 | ✅ |
| 55 | RSL-03 | 高 | 分销商/租户/域名 | 被禁用分销商的域名仍可解析为可用租户；分销站可访问分销商控制台；域名状态流转 | profile disabled + 域名 active verified → 解析为 Unavailable、ResellerID=nil | ✅ |
| 56 | RSL-04 | 高 | 分销商/租户/域名 | 利润待确认期内发生退款，扣减被记成“可用负余额”误冻结账户；到期确认不刷新余额缓存 | 利润 30 pending（ConfirmDays=7），退 65/130 → 扣减 -15 为 pending_confirm 且有 available_at | ✅ |
| 57 | RSL-05 | 高 | 分销商/租户/域名 | 分销利润记账、退款扣回与提现锁定 | 同一订单支付回调重复 2 次 → 仅 1 条 order_profit | ✅ |
| 58 | RSL-06 | 高 | 分销商/租户/域名 | 租户（分销站）解析与缓存隔离 | Host "Shop.EXAMPLE.com.:8080" 归一化为 shop.example.com | ✅ |
| 59 | RSL-07 | 高 | 分销商/租户/域名 | 分销站定价与订单租户隔离 | 基础价 100、成本 90、max_markup 50%：分销价 99 → 拒绝 | ✅ |
| 60 | DLV-01 | 高 | 发货/卡密 | 卡密出库导出：并发重复出库与数量不足 | 可用 5 张，导出 limit=6 → insufficient 且 5 张仍 available | ✅ |
| 61 | DLV-02 | 高 | 发货/卡密 | 交付使用说明只在付款后可见，并做 HTML 净化 | 未付款订单详情中 instructions=null | ✅ |
| 62 | DLV-03 | 高 | 发货/卡密 | 下单预占卡密：加行锁 + 条件更新并校验影响行数，建复合索引 | ①库存 1 张，两个并发下单：一个成功，一个返回库存不足，卡密只关联一个订单 | ✅ |
| 63 | DLV-04 | 高 | 发货/卡密 | 自动发货多 SKU 商品：卡密必须指定 SKU；禁用仍有卡密库存的 SKU 被拒绝 | auto 商品 SKU A/B 均启用，导入不传 sku_id → 400 sku_required | ✅ |
| 64 | UPS-01 | 高 | 上游对接/采购/下游回调/对账 | 下游回调客户端 SSRF 防护：只连公网 IP、不跟随重定向 | 回调地址 `http://127.0.0.1:xxxx`、`http://10.0.0.1`、`http://169.254.169.254`、`http://100.64.1.1`、解析到 127.0.0.1 的域… | ✅ |
| 65 | UPS-02 | 高 | 上游对接/采购/下游回调/对账 | 上游回调：状态机守卫 + 先写交付记录再推进状态 + body 限 1MB + 密钥解密失败拒绝 | 状态表：accepted→delivered 允许 | ✅ |
| 66 | UPS-03 | 高 | 上游对接/采购/下游回调/对账 | 上游回调归属校验：采购单必须属于本次认证的连接且上游订单号一致 | 同连接同上游单号 → 处理 | ✅ |
| 67 | UPS-04 | 高 | 上游对接/采购/下游回调/对账 | 导入/同步上游批发价时直接使用上游 SKU ID，造成本地阶梯指向错误 SKU | 上游 tier sku_id=5（上游）映射到本地 sku 12 → 本地 tier sku_id=12 | ✅ |
| 68 | UPS-05 | 高 | 上游对接/采购/下游回调/对账 | 站点对接的汇率/加价/取整/自动同步价格；修改连接参数后已映射商品售价联动 | up=10, rate=7.2, markup=20, none → 86.40 | ✅ |
| 69 | UPS-06 | 高 | 上游对接/采购/下游回调/对账 | 上游库存兜底与“上游已删除”误判 | 缓存库存 1、下单 3、实时同步后为 5 → 通过 | ✅ |
| 70 | UPS-07 | 高 | 上游对接/采购/下游回调/对账 | 采购单终态失败时必须把本地订单回退并告警，accepted 超时也要告警 | ①上游返回不可重试错误码后采购单为 rejected，订单从 fulfilling 变为 paid，并产生 1 条告警 | ✅ |
| 71 | UPS-08 | 高 | 上游对接/采购/下游回调/对账 | 上游价格解析失败被当作 0；成本价要按汇率换算 | ①上游 SKU price="abc"，同步后本地价格保持原值，只有库存被更新 | ✅ |
| 72 | UPS-09 | 高 | 上游对接/采购/下游回调/对账 | 映射商品的 fulfillment_type 必须恒为 upstream | 映射商品提交 fulfillment_type=auto 的更新 → 返回与重新读取均为 upstream | ✅ |
| 73 | UPS-10 | 高 | 上游对接/采购/下游回调/对账 | 上游开放 API 安全加固（SSRF、幂等、限流、用户状态、时间窗） | callback_url=http://10.0.0.1/x、http://localhost/、ftp://a.com → 400 invalid_callback_url | ✅ |
| 74 | UPS-11 | 高 | 上游对接/采购/下游回调/对账 | 采购单提交错误分类、轮询到期不判失败、定时巡检 | SKU 映射缺失 → 采购单 rejected、任务不再重试 | ✅ |
| 75 | UPS-12 | 高 | 上游对接/采购/下游回调/对账 | 下游回调：子订单找父引用、重发重置、交付信息取自子订单 | 父子订单子单交付 → 下游收到 order.fulfilled 且含子单 payload | ✅ |
| 76 | AUTH-01 | 高 | 用户认证/2FA/JWT/OAuth | Google 登录：ID Token 严格校验 + 仅权威邮箱自动关联 + redirect state 一次性消费 | aud 为其他 client → 拒绝 | ✅ |
| 77 | AUTH-02 | 高 | 用户认证/2FA/JWT/OAuth | Telegram OIDC 登录：身份 ID 口径与安全校验 | 历史绑定 provider_user_id=sub 的用户 OIDC 登录 → 登录同一用户且记录被改为数字 id | ✅ |
| 78 | AUTH-03 | 高 | 用户认证/2FA/JWT/OAuth | 2FA：挑战 token 与访问 token 隔离（typ），挑战 jti 一次性、失败计数、恢复码、覆盖所有登录入口 | 1) 用挑战 token 调 `GET /admin/xxx` 和 `/api/v1/user/me`，都返回 401 | ✅ |
| 79 | AUTH-04 | 高 | 用户认证/2FA/JWT/OAuth | Telegram Mini App：initData 严格校验（签名/时效/重放），前端先 ready() 且脚本加载超时 | 官方样例 initData 验签通过 | ✅ |
| 80 | AUTH-05 | 高 | 用户认证/2FA/JWT/OAuth | 注册开关/邮箱验证开关必须覆盖所有入口（含 Telegram 自动注册、发验证码、找回密码） | 关闭注册 → 邮箱注册 403、发注册验证码 403、TG 新用户登录 403 且 users 表行数不变 | ✅ |
| 81 | ADM-01 | 高 | 管理员/RBAC/权限 | 支付渠道配置密钥脱敏返回，合并更新保留原值 | GET 渠道 → secret_key 为掩码 | ✅ |
| 82 | ADM-02 | 高 | 管理员/RBAC/权限 | 只读审计员禁止通配策略；内置角色不可通过通用 API 修改，启动时收敛多余策略 | 审计员访问 GET /admin/payment-channels/:id → 403 | ✅ |
| 83 | ADM-03 | 高 | 管理员/RBAC/权限 | 初始管理员：由配置创建者恒为超管（不依赖用户名 admin）；env > config；release 模式禁用默认密码 | 配置 username="root" 空库启动 → root.is_super=true 且能访问 RBAC 保护接口 | ✅ |
| 84 | RISK-01 | 高 | 验证码/限流/风控 | 订单风控：黑名单、待支付上限、下单频率、游客/会员分策略、按 risk_ip 计数与锁串行化 | trusted_proxies 配 `0.0.0.0/0` → 启动失败 | ✅ |
| 85 | UPL-01 | 高 | 上传/素材/SVG/XSS | SVG 上传：XML 解析器校验 + 危险元素/属性拦截 + /uploads 下 SVG 强制下载与 CSP sandbox + 默认关闭 SVG | 表中各 payload（tab/换行/双空格 onload、onbegin、`&#106 | ✅ |
| 86 | UPL-02 | 高 | 上传/素材/SVG/XSS | 后台渲染远端 Markdown（Release Notes）必须 DOMPurify 白名单净化；自更新回滚安全闸门 | release body 含 `<img src=x onerror=alert(1)>`、`[x](javascript:alert(1))`、`<a style=...>` → 输出中无 onerror、无 ja… | ✅ |
| 87 | SET-01 | 高 | 设置/公共配置/回调路由 | 自定义回调路由：隐藏默认路径 + 路径冲突/重复校验 + 缓存失效 | ①保存 `payment_callback="/api/v1/admin/x"`，结果落库为空，`/api/v1/admin/*` 仍走原 handler | ✅ |
| 88 | DB-01 | 高 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | 事务内一律使用事务连接（SQLite 单连接池自锁/死锁）；行锁事务内不做无关读取；外部请求设超时 | SQLite max_connections=1 下创建支付（含渠道查询）在 5 秒内完成不挂起 | ✅ |
| 89 | NTF-01 | 高 | 通知/邮件/Telegram Bot | 订单状态邮件被滥用为邮件轰炸；自动发货订单重复邮件 | 游客订单过期取消 → 无邮件任务执行发送 | ✅ |
| 90 | MISC-01 | 高 | 其他 | 运行时密钥弱/默认/重复一律拒绝启动；HTTP 服务器设置超时 | 两个密钥配置相同 → 启动失败并列出两者名称 | ✅ |
| 91 | MISC-02 | 高 | 其他 | 前台 API 必须用白名单 DTO：成本价、自增 ID、管理员备注等敏感字段曾经泄露 | ①GET /public/products/:slug 的 JSON 中不存在 `cost_price_amount` 键（包括 skus[]） | ✅ |
| 92 | PAY-16 | 中 | 支付/通用 | 同步回跳（return_url）必须携带订单/网关标记参数，返回页对所有网关触发查单 | 分别以五种 marker 回跳 → 前端都调用一次同步接口 | ✅ |
| 93 | PAY-17 | 中 | 支付/通用 | 商品绑定的支付渠道已停用/删除时残留 | 传 [活跃1, 停用2, 已删3, 1, 0] → 存 [1] | ✅ |
| 94 | PAY-18 | 中 | 支付/通用 | 回调 query 使用 `;` 分隔或 `&amp;` 转义时无法解析 | `/payments/callback?pid=2026 | ✅ |
| 95 | PAY-19 | 中 | 支付/通用 | 管理端编辑渠道配置时，清空的字段必须真正删除（汇率残留） | 渠道原来 exchange_rate=7.2，编辑时清空并保存，GET 返回中不再有 exchange_rate，支付金额按 1:1 计算 | ✅ |
| 96 | PAY-20 | 中 | 支付/通用 | 支付返回链接区分订单/充值（biz_type、recharge_no），兼容旧链接 | 游客订单 → query 含 biz_type=order&order_no=DJ..&guest=1&epay_return=1 | ✅ |
| 97 | PAY-21 | 中 | 支付/通用 | 回调全链路可追溯日志（校验失败原因必须可见） | 金额不一致的回调 → 拒绝且日志包含 stored_amount/callback_amount | ✅ |
| 98 | PAY-24 | 中 | 支付/钱包余额支付 | 钱包全额支付也要生成支付记录，但统计需排除 | 余额足额下单 → 生成 1 条 wallet/success payment，订单 paid | ✅ |
| 99 | PAY-27 | 中 | 支付/Stripe | Checkout 启用 wechat_pay 时未声明 client，Stripe 返回 400 | mock Stripe server：types=[card,wechat_pay] → form 含 client=web | ✅ |
| 100 | PAY-31 | 中 | 支付/PayPal | webhook_id 必填导致无法保存渠道；但验签时必须有 | 无 webhook_id 配置保存成功 | ✅ |
| 101 | PAY-36 | 中 | 支付/易支付 | 下单响应兼容"双重 JSON 编码"与压缩 | mock 网关返回 `"{\"code\":1,\"trade_no\":\"T1\",\"payurl\":\"https://x\"}"` → 得到 trade_no=T1、pay_url | ✅ |
| 102 | PAY-37 | 中 | 支付/易支付 | 跳转模式（submit.php）与交互模式校验 | redirect+V1 生成 URL 路径 `/submit.php`、sign=md5(sorted+key) | ✅ |
| 103 | PAY-44 | 中 | 支付/epusdt / BEpusdt | BEpusdt：旧 channel_type 迁移、收银台模式与 trade_type/QR 互斥、回调不得覆盖 display_channel_type | 创建 BEpusdt transaction 支付（trade_type=usdt.arbitrum）→ payload.display_channel_type=usdt.arbitrum | ✅ |
| 104 | PAY-49 | 中 | 支付/DujiaoPay | 收银台（延迟分配）模式的 channel_type/order_mode 一致性 | cashier + allowed_methods="tron-usdt, base-usdc,tron-usdt" → 请求体 allowed_methods 两项且去重 | ✅ |
| 105 | ORD-07 | 中 | 订单/下单与库存 | 游客订单邮箱大小写/空格归一化（前后端一致） | 用 `a@b.com` 下单后，分别用 ` A@B.COM `、`a@B.com` 查询列表、详情和下载，都能找到 | ✅ |
| 106 | ORD-08 | 中 | 订单/下单与库存 | 模糊库存展示：公开接口不得泄露精确库存 | hidden 模式真实库存 37 → auto_stock_available=1、stock_display=hidden | ✅ |
| 107 | ORD-09 | 中 | 订单/下单与库存 | 分类约束：仅叶子且启用分类可挂商品/上架；停用分类前台与 Bot 均不可见不可下单 | category_id=0 的商品 quick 上架 → 400 | ✅ |
| 108 | ORD-10 | 中 | 订单/下单与库存 | 单商品最小/最大购买数量限制（服务端强制） | min=3 时下单 qty=2 返回 min_not_met、qty=3 成功 | ✅ |
| 109 | ORD-11 | 中 | 订单/下单与库存 | 编辑 SKU：删除的 SKU 用软删会和同 sku_code 的新行撞唯一索引 | ①创建 SKU A → 编辑时移除 A → 再次添加 code=A，保存成功 | ✅ |
| 110 | RFD-04 | 中 | 退款 | 退款时效窗口 max_refund_days | days=30 时，paid_at 为 31 天前的订单退款返回 expired，29 天前的可以退 | ✅ |
| 111 | WAL-02 | 中 | 钱包/充值/礼品卡 | 管理员调账必须显式指定操作、填写备注并记录操作人 | 不带 operation → 400 | ✅ |
| 112 | PRC-10 | 中 | 优惠券/活动价/批发价/会员价 定价 | 商品更新未携带 wholesale_prices 时被静默清空 | 创建带 1 档 → PUT 不带 wholesale_prices → 仍 1 档 | ✅ |
| 113 | PRC-11 | 中 | 优惠券/活动价/批发价/会员价 定价 | 金额字段语义：unit_price 已扣活动价/会员价，coupon 单独分摊，避免重复扣减 | 原价 100、活动价 80、会员再减 5（unit_price=75）、qty=2、优惠券 10 → total_price=150、行实付=140、共减=60 | ✅ |
| 114 | PRC-12 | 中 | 优惠券/活动价/批发价/会员价 定价 | 会员等级自动升级：同 sort_order 不升级；管理员手动置为已支付不触发；注册默认等级被覆盖 | ①default(sort0) 和 vip(sort0, 门槛 0.01)，支付 0.01 后升级为 vip | ✅ |
| 115 | PRC-13 | 中 | 优惠券/活动价/批发价/会员价 定价 | 商品展示价应取首个启用 SKU 价，活动价基于展示价计算 | 商品价 59.90，SKU A(active, sort 100, 89.90)、B(active, sort 10, 49.90) → 展示 89.90 | ✅ |
| 116 | RSL-08 | 中 | 分销商/租户/域名 | 分销站公告以错误结构输出、禁用时泄漏主站公告 | 主站 config 有 announcement{version:"main0000"}，分销站公告 disabled → 输出无 announcement 字段 | ✅ |
| 117 | RSL-09 | 中 | 分销商/租户/域名 | 分销展示价因单个 SKU 失效配置导致整页/整商品报错 | SKU11 固定价 130（有效）、SKU12 固定价 80 低于基价 100 → 结果 Visible、HiddenSKUIDs[12]=true、无 12 的价格、DisplaySKUID=11 价格 130 | ✅ |
| 118 | DLV-05 | 中 | 发货/卡密 | 超长交付内容：接口截断 + 单独下载；下载必须校验订单归属（含子订单） | ①3000 行 payload 的详情只返回 100 行，payload_line_count=3000 | ✅ |
| 119 | DLV-06 | 中 | 发货/卡密 | 批量导入卡密数量过多时失败 | ①一次导入 10000 条，在 SQLite、MySQL、PG 上都成功，count=10000 | ✅ |
| 120 | DLV-07 | 中 | 发货/卡密 | 卡密批量操作的目标解析、批次实时计数与搜索 | 批量删除 body `{}` → 400 且无行被删 | ✅ |
| 121 | DLV-08 | 中 | 发货/卡密 | 自动发货商品库存按 SKU 统计卡密（含 sku_id=0 通用卡密），多 SKU 不重复/虚高 | 商品 3 个 SKU（A,B 活跃，DEFAULT 活跃），sku_id=0 卡密 5 张 → 只有 DEFAULT 得 +5 | ✅ |
| 122 | DLV-09 | 中 | 发货/卡密 | 自动发货成功后订单直接为 completed，并发送含卡密的完成邮件 | 自动发货后订单状态 == completed | ✅ |
| 123 | UPS-13 | 中 | 上游对接/采购/下游回调/对账 | 上游同步清空本地批发价、导入非法档位 | 本地已有 {5:80}，上游返回无 wholesale_prices → 同步后仍 {5:80} | ✅ |
| 124 | UPS-14 | 中 | 上游对接/采购/下游回调/对账 | 上游商品同步：SKU 增删、下架/删除识别、真实库存 | 上游返回 is_active=false → 本地商品和 SKU 下架，mapping 状态为 inactive | ✅ |
| 125 | UPS-15 | 中 | 上游对接/采购/下游回调/对账 | Bot/渠道目录对上游映射商品库存误报缺货 | 映射商品有 2 个 SKU，上游库存分别为 5 和 -1 → 商品有货，库存为 -1 | ✅ |
| 126 | UPS-16 | 中 | 上游对接/采购/下游回调/对账 | 上游退款状态同步和对账一致性映射 | 上游回调 status="Cancelled " 时映射为 canceled | ✅ |
| 127 | UPS-17 | 中 | 上游对接/采购/下游回调/对账 | 上游同步任务防重叠 + 按连接批量拉取 + 增量同步 | ①两个 worker 同时触发，只有一个执行 | ✅ |
| 128 | UPS-18 | 中 | 上游对接/采购/下游回调/对账 | 下游 API 凭证申请：待审核空 api_key 撞唯一索引、软删后重新申请、密钥字段长度 | ①用户 A、B 先后申请且都未审核，两次都成功，api_key 不同且非空 | ✅ |
| 129 | UPS-19 | 中 | 上游对接/采购/下游回调/对账 | 多级对接：映射商品按上游“真实交付类型”展示与计算库存，前台隐藏 upstream 类型 | B 映射了 A 的 auto 商品 → B 的 `/upstream/products` 返回 fulfillment_type=auto，stock_count 为同步的上游库存 | ✅ |
| 130 | UPS-20 | 中 | 上游对接/采购/下游回调/对账 | 导入上游商品必须单事务：商品+SKU+映射+SKU映射 | 注入 SKU 映射插入失败 → products/product_skus/product_mappings 三表均无新增行 | ✅ |
| 131 | UPS-21 | 中 | 上游对接/采购/下游回调/对账 | 对账金额比较口径错误与统计口径 | 售价 12、采购价 10、上游返回 10 → 无差异 | ✅ |
| 132 | AUTH-06 | 中 | 用户认证/2FA/JWT/OAuth | 注册与换绑邮箱拒绝 Telegram 占位邮箱 | 注册 `Telegram_1@LOGIN.local` → email_invalid | ✅ |
| 133 | AUTH-07 | 中 | 用户认证/2FA/JWT/OAuth | 注册邮箱域名白名单 | 白名单 [gmail.com]：a@gmail.com 允许 | ✅ |
| 134 | AUTH-08 | 中 | 用户认证/2FA/JWT/OAuth | TOTP 启用流程校验顺序 | pending 过期 → expired 且失败计数不变 | ✅ |
| 135 | AUTH-09 | 中 | 用户认证/2FA/JWT/OAuth | 登录接口的用户枚举时序防护：dummy bcrypt 必须是合法哈希 | ①dummy 哈希能被 `bcrypt::verify` 正常解析（返回 Ok(false)） | ✅ |
| 136 | AUTH-10 | 中 | 用户认证/2FA/JWT/OAuth | 前端 token 过期处理：业务码 401 也需清登录态，但登录接口本身的 401 不能跳转 | 过期 token 请求 /me 返回 status_code 401 → localStorage 清空并跳登录 | ✅ |
| 137 | ADM-04 | 中 | 管理员/RBAC/权限 | 后台列表排序/筛选参数必须白名单与严格解析 | sort_by=`id | ✅ |
| 138 | ADM-05 | 中 | 管理员/RBAC/权限 | 超管密码 CLI 重置需使旧会话失效；支付财务路由合规门禁；用户管理 | CLI 重置后旧 token 请求 → 401 | ✅ |
| 139 | ADM-06 | 中 | 管理员/RBAC/权限 | 内置角色种子必须覆盖所有后台路由（覆盖率测试防漏） | 遍历 axum admin router 的所有路由，断言每条都能在内置角色策略中匹配到 | ✅ |
| 140 | RISK-02 | 中 | 验证码/限流/风控 | 支付回调、上游回调、渠道 API、礼品卡兑换必须限流 | 同一用户 60s 内第 11 次礼品卡兑换返回 429(error.rate_limited) 且 300s 内持续被拒 | ✅ |
| 141 | RISK-03 | 中 | 验证码/限流/风控 | 鉴权前限流 key 必须绑定来源 IP；last_used_at 只更新单列 | 同一 IP 轮换 1000 个随机 API Key 请求上游 API，仍在第 N+1 次被限流（N=规则上限） | ✅ |
| 142 | RISK-04 | 中 | 验证码/限流/风控 | 限流 Redis 不可用时降级为进程内计数（不再 fail-open/500），容量耗尽拒绝新 key；429 状态码 | 不配 Redis 时登录接口第 N+1 次返回 429 | ✅ |
| 143 | RISK-05 | 中 | 验证码/限流/风控 | 签名鉴权中间件读取 body 要设上限 | ①向 channel API 发 11MB body，返回 400/413，进程内存不暴涨 | ✅ |
| 144 | RISK-06 | 中 | 验证码/限流/风控 | CORS / JWT / 限流中间件的边界行为 | Origin=https://a.com、配置 ["*"]、credentials=true → ACAO=https://a.com | ✅ |
| 145 | RISK-07 | 中 | 验证码/限流/风控 | Redis 登录限流：超过阈值后按 block_seconds 延长封禁 | max=5, window=60, block=600：第 6 次请求被拒且 TTL≈600 | ✅ |
| 146 | UPL-03 | 中 | 上传/素材/SVG/XSS | 素材库：上传落库、删除物理文件、上游图片入库 | ①上传 png 后 media 表新增一条，width/height 正确 | ✅ |
| 147 | UPL-04 | 中 | 上传/素材/SVG/XSS | telegram 场景上传绕过扩展名/类型白名单 | 普通用户以 scene=telegram 上传 → 403 | ✅ |
| 148 | SET-02 | 中 | 设置/公共配置/回调路由 | 首页公告弹窗：归一化、排期与 XSS | content 含 `<img onerror=alert(1)>` → 渲染后无事件属性 | ✅ |
| 149 | SET-03 | 中 | 设置/公共配置/回调路由 | sitemap/robots 的基础 URL 不要信任 X-Forwarded-Host | 配置了 site_url 时，伪造 X-Forwarded-Host: evil.com，sitemap 的 loc 仍为配置的域名 | ✅ |
| 150 | DB-02 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | SQLite 时间范围查询：TEXT 格式与参数格式不一致导致统计缺漏 | SQLite 下插入 created_at=`2026-09-10 00:00:00+00:00` 的订单，查询区间 [2026-09-10T00:00:00Z, 2026-09-11T00:00:00Z) → 计入 | ✅ |
| 151 | DB-03 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | 迁移补外键：先检查孤儿行；SQLite 重建表会丢索引需重跑 AutoMigrate；Preload 需过滤软删除 | 迁移前插入引用不存在商品的购物车行 → 迁移报出孤儿数量 | N/A |
| 152 | DB-04 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | ORM 零值陷阱：bool=false / 0 被默认值吞掉或被后续整行 Save 覆盖 | 首次保存 SKU 级设置 is_listed=false → 重新读取 is_listed=false | ✅ |
| 153 | DB-05 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | 时间统一存 UTC；按日分组要按方言转换时区 | ①UTC 2026-03-30 17:00 的订单在 Asia/Shanghai 下归到 03-31 | ✅ |
| 154 | DB-06 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | 后台订单列表：count 复用查询、商品关键字 SQL 列名错误、排序注入白名单 | ①按中文商品名关键字搜索，在三种数据库上都能返回包含该商品（含子订单商品）的父订单 | ✅ |
| 155 | DB-07 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | ORM 列名映射陷阱：SKUID 被映射为 s_k_uid | 两个 SKU 各有 3 张和 0 张可用卡密，统计结果按 sku_id 正确区分，阈值 5 时两个都在低库存列表中且 sku_id 正确 | ✅ |
| 156 | DB-08 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | 启动迁移幂等标记与 SQLite WAL/busy_timeout | 连续两次启动，第二次不执行回填 SQL | ✅ |
| 157 | DB-09 | 中 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | 跨方言 SQL：PostgreSQL 无 json_extract/date 返回类型差异，SQLite JSON 路径含 '-' 需加引号 | 三种数据库上以 "测试" 搜索商品标题（title_json.zh-CN）能命中 | ✅ |
| 158 | NTF-02 | 中 | 通知/邮件/Telegram Bot | 白标（分销商站点）邮件品牌隔离，禁止回退到主站品牌 | 分销域名 shop.example 下注册 → 验证码邮件主题含分销站点名、正文网址为 https://shop.example、From 名为分销站名 | ✅ |
| 159 | NTF-03 | 中 | 通知/邮件/Telegram Bot | SMTP 认证机制选择、连接关闭、RFC 5322 邮件头 | 模拟 SMTP 服务器只声明 `AUTH LOGIN` 时发送成功 | ✅ |
| 160 | NTF-04 | 中 | 通知/邮件/Telegram Bot | 库存告警发送间隔失效（alert_type 被本地化后比较不上） | ①interval=3600，连续两次触发 low_stock 告警，第二次被 SetNX 拦截 | ✅ |
| 161 | NTF-05 | 中 | 通知/邮件/Telegram Bot | SMTP 关闭时不入队、不可恢复错误不重试 | SMTP disabled → 订单支付后队列无 email 任务 | ✅ |
| 162 | NTF-06 | 中 | 通知/邮件/Telegram Bot | 邮件/Bot 通知：真实换行、Telegram 占位邮箱与空邮箱不发邮件 | 收件人 "telegram_123@login.local" → skipped=true | ✅ |
| 163 | FE-01 | 中 | 前端 | 自动跳转收银台必须在当前标签页打开；wap/page 交互模式按跳转处理 | interaction_mode=wap 创建支付后自动在当前页跳转 | ✅ |
| 164 | FE-02 | 中 | 前端 | 购物车本地缓存价格需随商品刷新同步 | 缓存价 10，后台改为 12 → 打开购物车显示 12 | ❌ |
| 165 | FE-03 | 中 | 前端 | SPA 部署：index.html no-cache、hash 资源强缓存、保留路径 404、内嵌 SPA 路由兜底、运行时 base | GET / → Cache-Control: no-cache | N/A |
| 166 | FE-04 | 中 | 前端 | 提现提交成功后刷新失败被误判为“提现失败”诱导重复提交；加载失败被显示为“暂无数据” | mock applyWithdraw 成功、balanceAccounts 500 → UI 提示成功、按钮恢复、不抛未处理 rejection | ❌ |
| 167 | FE-05 | 中 | 前端 | 收银台“切换支付方式”后又被自动恢复上一笔支付并反复跳转网关 | 1) 创建 redirect 支付 → 点更换方式 → 断言未再次 open 窗口且渠道选择清空、轮询停止 | ⚠ |
| 168 | FE-06 | 中 | 前端 | 自定义导航和外链：URL 要限制协议 | ①保存 url=`javascript:alert(1)` 的 external 项，该项被丢弃 | ✅ |
| 169 | FE-07 | 中 | 前端 | Telegram SDK 按需加载，防止被墙地区整站白屏 | 模拟 telegram.org 不可达、URL 无 tgWebAppData → 页面正常渲染且不请求 telegram.org | ❌ |
| 170 | FE-08 | 中 | 前端 | 立即购买/快速购买：不影响购物车、合并"下单并支付"接口、数量限制 | 购物车有 2 件，立即购买 1 件并下单 → 购物车仍为 2 件 | ⚠ |
| 171 | FE-09 | 中 | 前端 | 编辑支付渠道时 watch 覆盖已保存的交互模式 | 编辑 interaction_mode=redirect 的 epay 渠道，不改任何字段直接保存 → 仍为 redirect | ✅ |
| 172 | FE-10 | 中 | 前端 | 使用服务器时间校正倒计时与推广码过期 | 客户端时钟快 10 分钟、支付单剩余 5 分钟 → 页面仍显示约 5 分钟而非"已过期" | ❌ |
| 173 | FE-11 | 中 | 前端 | 支付回跳参数解析：兼容 `&amp;` 被转义的 query 与 out_trade_no | URL `/payment?order_no=DJ1&amp | ✅ |
| 174 | FE-12 | 中 | 前端 | 金额计算用整数分，避免浮点误差 | amountToCents("10.005") = 1001 | ✅ |
| 175 | MISC-03 | 中 | 其他 | 仪表盘利润/统计口径：折扣不重复扣减、零成本商品、退款冲回成本、时区分桶 | ①单价 100、活动价 95×2、优惠券 10 → 收入 180（不是 170） | ✅ |
| 176 | MISC-04 | 中 | 其他 | 后台商品详情必须返回全部 SKU（含禁用） | 商品含启用 A、禁用 B → 后台详情返回 2 个且 B.is_active=false | ✅ |
| 177 | PAY-22 | 低 | 支付/通用 | 网关展示商品名：统一使用订单号；支付宝公共参数放 URL 防乱码 | 创建含中文商品的订单调用支付宝预下单，抓取请求：URL 含 charset=utf-8 与 sign，body 只有 biz_content | ✅ |
| 178 | PAY-45 | 低 | 支付/epusdt / BEpusdt | 不向前端暴露 provider_payload；回调 payload 结构化保存 | 用户创建支付响应 JSON 中不存在 provider_payload 字段 | ✅ |
| 179 | PAY-47 | 低 | 支付/TokenPay | 网关币种代码原样透传，不做大小写转换 | 配置 currency="USDT_TRC20"/"usdt_trc20" → 下单请求体原样 | ✅ |
| 180 | ORD-12 | 低 | 订单/下单与库存 | 后台库存状态筛选：auto/upstream 商品和低库存阈值 | 阈值 5，auto 商品可用 0/3/6 张 → low 包含 0 和 3，normal 只包含 6 | ✅ |
| 181 | WAL-03 | 低 | 钱包/充值/礼品卡 | 钱包充值"检查支付状态"：渠道不支持主动 capture 时回退为查询当前状态 | 易支付渠道充值单调用 capture → 200，返回 status=pending | ✅ |
| 182 | PRC-14 | 低 | 优惠券/活动价/批发价/会员价 定价 | 列表展示价与活动价必须来自同一 SKU | SKU1(排序高) 价 10 无活动、SKU2 价 100 活动价 50 → 列表 promotion_price 为空（或与 SKU1 一致），不能出现 50 | ✅ |
| 183 | PRC-15 | 低 | 优惠券/活动价/批发价/会员价 定价 | 优惠券按适用商品筛选：JSON 数组列需边界匹配 | 优惠券 A scope_ref_ids=[11,21]，B=[1,5] → 按 scope_ref_id=1 仅返回 B | ✅ |
| 184 | AFF-01 | 低 | 推广返利 | 佣金到期确认改为调度任务 | 两个 worker 同时执行确认 → 每条佣金只确认/入账一次 | ✅ |
| 185 | RSL-10 | 低 | 分销商/租户/域名 | 分销订单买家标识脱敏；分销站结算隐藏优惠券 | 会员 buyer-label@example.test → "b***@example.test"（列表与详情一致） | ✅ |
| 186 | DLV-10 | 低 | 发货/卡密 | 卡密导入去重开关（默认去重） | deduplicate 缺省时 "a\na\nb" 导入 2 条 | ✅ |
| 187 | UPS-22 | 低 | 上游对接/采购/下游回调/对账 | 列表状态统计必须基于全量筛选结果；时间筛选要解析成时间类型 | created_to=2026-04-27T23:59:59+08:00 时包含当天 23:00 的记录 | ✅ |
| 188 | UPL-05 | 低 | 上传/素材/SVG/XSS | 上传校验错误返回 500 且前端看不到原因；批量删除素材 | 上传 11MB → 400 且消息含 "最大 10 MB" | ✅ |
| 189 | SET-04 | 低 | 设置/公共配置/回调路由 | 设置/筛选参数白名单与范围钳制 | sync_page_size=0 → 默认值，=100000 → 上限 | ✅ |
| 190 | SET-05 | 低 | 设置/公共配置/回调路由 | 公开商品 DTO 缺少 seo_meta | GET 公开商品详情，响应包含 seo_meta 字段 | ❌ |
| 191 | DB-10 | 低 | 数据库/迁移/并发/事务/SQLite/Postgres 差异 | 跨库 LIKE 大小写差异与后台查询参数校验 | PG 中搜索 "abc" 命中 "ABC" | ❌ |
| 192 | NTF-07 | 低 | 通知/邮件/Telegram Bot | 上游交付（upstream）订单误发“待人工交付”通知 | nil → false | ✅ |
| 193 | NTF-08 | 低 | 通知/邮件/Telegram Bot | 支付订单告警按时间窗口统计并限制发送间隔 | 间隔 600 秒内连续触发两次，只发送一条 | ✅ |
| 194 | NTF-09 | 低 | 通知/邮件/Telegram Bot | Telegram Bot 内置菜单缺项回填 | 老配置中 my_orders.enabled=false，读取后仍为 false，并且 affiliate 被补齐且 enabled=true | ⚠ |
| 195 | NTF-10 | 低 | 通知/邮件/Telegram Bot | 交付内容过大时订单邮件改为附件 | ①21 行 payload 时邮件带附件，正文不含卡密 | ✅ |
| 196 | NTF-11 | 低 | 通知/邮件/Telegram Bot | 拆单父订单通知变量需聚合子订单商品 | 父单无 items、两个子单分别 upstream x1、manual x2 → items_summary 同时包含两行 | ✅ |
| 197 | FE-13 | 低 | 前端 | vault 模板需渲染后台自定义导航与页脚配置 | 后台新增导航项 → vault 顶栏出现 | N/A |
| 198 | FE-14 | 低 | 前端 | 游客订单凭据存储受限（隐私模式/禁用存储）时仍可打开订单详情 | 模拟 sessionStorage getter 抛异常 → 输入凭据后能查看详情 | ⚠ |
| 199 | FE-15 | 低 | 前端 | 后台杂项功能性 bug：支付渠道列表链/币种显示、OKPay 编辑回填、批发价分页、TG MiniApp 检测 | okpay 配置 {coin:"TRX"} 编辑回填 channel_type=trx | ⚠ |
| 200 | FE-16 | 低 | 前端 | 首次访问 UI 语言与商品多语言取值不一致 | 清空 localStorage、navigator.language='en-US' → i18n 与商品 getLocalizedText 均取 en-US | ❌ |
| 201 | FE-17 | 低 | 前端 | 个人中心订单统计卡片只统计当前页 | 用户 25 单（3 待支付分布在第 2 页）→ stats.pending_payment=3 | ❌ |
| 202 | FE-18 | 低 | 前端 | shadcn/reka Select 不能用空字符串作为选项值；Checkbox 的 indeterminate 值 | 选择"全部"后请求参数中不包含该字段 | ⚠ |
| 203 | FE-19 | 低 | 前端 | 购物车库存快照：结算前刷新、未知库存不当 0；最终以服务端下单事务为准 | 购物车数量 5，刷新后 SKU 可用 3 → 结算按钮禁用并提示 | ⚠ |
| 204 | FE-20 | 低 | 前端 | fetch 客户端的 401 处理与超时 | ①登录接口返回 401 时停留在登录页并显示错误 | ⚠ |
| 205 | FE-21 | 低 | 前端 | 后台支付渠道弹窗在新建模式下保留了上一次编辑的数据 | 编辑渠道 A → 关闭 → 点新建，所有字段为默认值（provider=epay、fee=0、config 为空） | ✅ |
| 206 | FE-22 | 低 | 前端 | 手动交付表单提交内容要按 schema 快照的字段顺序和标签显示 | 定义字段顺序为 [qq, email, remark]，提交后三个详情页都按该顺序显示对应的本地化标签 | ✅ |
| 207 | FE-23 | 低 | 前端 | 弱网下的路由切换加载与重复加载配置 | 模拟 chunk 404 → loading 消失并提示 | ❌ |
| 208 | FE-24 | 低 | 前端 | 后台设置子 Tab 只在 setup 时从 props 同步，异步加载的数据不显示 | 模拟设置接口延迟 500ms 返回 SMTP host=smtp.x.com → 渲染后输入框显示 smtp.x.com | ❌ |
| 209 | FE-25 | 低 | 前端 | vue-i18n 消息中的 `{{ }}` 等特殊字符须转义；语言检测与 X-Lang 头 | 渲染含 `{'{{order_no}}'}` 的文案 → 页面显示 `{{order_no}}` | ❌ |
| 210 | FE-26 | 低 | 前端 | 支付二维码缺失时回退为支付链接；钱包充值页同理 | 返回 {interaction_mode:'qr', qr_code:'', pay_url:'https://p/x'} → 渲染 pay_url 的二维码与提示 | ✅ |
| 211 | MISC-05 | 低 | 其他 | 库存告警覆盖对接(upstream)商品：按 sku_mappings 的上游库存计算 | upstream 商品 SKU 映射 upstream_stock=0 且 active → out_of_stock 告警 | ✅ |
| 212 | MISC-06 | 低 | 其他 | 文章分类：后台树缺失禁用分类；禁用分类仍可被新文章挂载 | 创建文章挂禁用分类 → 拒绝 | ✅ |
| 213 | MISC-07 | 低 | 其他 | 公开博客关联商品需过滤下架商品（原实现遗漏） | 关联的商品下架后，博客详情中不再出现该商品 | ✅ |
| 214 | MISC-08 | 低 | 其他 | 博客/公告发布时间 | 草稿 → 发布时设置 published_at | ✅ |
| 215 | MISC-09 | 低 | 其他 | 排序规则统一为 sort_order 越大越靠前 | sort_order=100 与 1 → 100 在前 | ⚠ |
| 216 | PRV-01 | 高 | 供货方兼容（异次元/萌次元） | acg-faka 3.7.x：`app_id` 标量检查 + `hash_equals`（修复 `0e…` magic hash 与数组 app_id 500） | `app_id[]=…` → HTTP 200 `{"code":0}`；`0e…`/大写/改字段后的签名均拒绝 | ✅ |
| 217 | PRV-02 | 高 | 供货方兼容（异次元/萌次元） | acg-faka 3.7.x `api_status` 闸门：按 code 寻址的接口只允许开放对接的商品；本站另限自动发货商品（同步交付） | 人工发货商品：items 不出现，item/trade 返回「该商品未开放对接」且不扣款 | ✅ |
| 218 | PRV-03 | 高 | 供货方兼容（异次元/萌次元） | `request_no`/`trade_no` 唯一：重复请求不得重复扣款 | 同一 request_no 两次 trade → 同一 tradeNo 与卡密，余额只扣一次 | ✅ |
| 219 | PRV-04 | 高 | 供货方兼容（异次元/萌次元） | acg-faka 3.7.x `draftCard` 只允许按 draft 过滤（`search-secret` 卡密盲注） | 带 `search-secret` 的 draftCard → 空列表 | ✅ |
| 220 | PRV-05 | 中 | 供货方兼容（异次元/萌次元） | acg-faka 0 元单拦截（`Order.php:875-889`） | 价格 0 的商品 valuation/trade → 「商品价格异常」，余额不变 | ✅ |
| 221 | PRV-06 | 中 | 供货方兼容（异次元/萌次元） | 协议无时间戳/nonce：按 IP+app_id 限流 | 第 301 次请求 → 「请求过于频繁」 | ✅ |
| 222 | PRV-07 | 高 | 供货方兼容（异次元/萌次元） | 原版鉴权不校验用户状态：本站要求凭证已审核+启用、用户 active、兼容密钥开启、IP 白名单；重新审核作废密钥 | 各状态逐一切换 → 请求被拒 | ✅ |
| 223 | ACG-01 | 高 | 采购方适配器（异次元/萌次元） | 签名按原始字段现算；密钥绝不随请求发送 | 规格 V1–V3 向量一致；所有请求不含 app_key | ✅ |
| 224 | ACG-02 | 高 | 采购方适配器（异次元/萌次元） | request_no 为 char(19) 唯一列、重复即报错：丢失应答的 trade 不重复购买 | 断连后重试 → manual_review 待人工核对（不回退），对方只有 1 笔订单；管理端重试仍只 1 笔，取消后才回退 | ✅ |
| 225 | MCY-01 | 高 | 采购方适配器（异次元/萌次元） | 萌次元 trade_no 去重未验证：丢失应答的 trade 不重试 | 断连 → manual_review(upstream_result_unknown，不回退)，再次 submit 不调用 trade | ✅ |
| 226 | ACG-03 | 中 | 采购方适配器（异次元/萌次元） | 3.7.x items 不含拿货价，须读 item/inventory | SKU 价格 8.50（拿货价）而非 10.00（零售价） | ✅ |
| 227 | ACG-04 | 中 | 采购方适配器（异次元/萌次元） | secret 可能是提示文案：人工发货商品不自动完成 | 人工发货商品：trade/query 为提示文案时仍 accepted；query 内容变为真实卡密后交付且仅一次 | ✅ |
| 228 | MCY-02 | 中 | 采购方适配器（异次元/萌次元） | 保留对方错误原文（不吞成「连接失败#0」） | 余额不足 → error_message 含「余额不足」 | ✅ |
| 229 | MCY-03 | 中 | 采购方适配器（异次元/萌次元） | contents 缺失/售后文案不当交付 | 售后文案 → manual_review（保留单号）、无交付 | ✅ |
| 230 | ACG-05 | 低 | 采购方适配器（异次元/萌次元） | 3.5.9 控件 JSON URL 编码；占位封面 | `%5B…` widget 可解析；/favicon.ico 不作图片 | ✅ |
| 231 | MCY-04 | 低 | 采购方适配器（异次元/萌次元） | 按 SKU 名映射改名即断链：按 sku_id 映射 | 改名调价后映射仍指向同一 sku_id，价格跟随 | ✅ |
| 232 | PRV-08 | 中 | 供货方兼容（异次元/萌次元） | 实机互通发现：SKU `spec_values` 为本地化对象（后台/种子数据的写法）时，种类名被拼成三种语言 `空月祝福 / 空月祝福 / Welkin` | 规格 `{"zh-CN":"空月祝福","zh-TW":…,"en-US":"Welkin"}` → items/item 种类名 `空月祝福`，按该名 trade 成功 | ✅ |
| 233 | PRV-09 | 低 | 供货方兼容（异次元/萌次元） | 实机互通发现：重放 trade（同 request_no）返回 `"stock": null`，原版恒为字符串 | 同一 request_no 第二次 trade → `stock` 为 `"1"` | ✅ |
| 234 | ACG-06 | 高 | 采购方适配器（异次元/萌次元） | 实机互通发现：对方控件 `regex` 未带入本站表单 schema，买家已付款后对方 trade 才以控件 `error` 拒单 | 控件 `^[0-9]{6,12}$` → 本站下单 `account=abc` 被表单校验拒绝（不扣款），`12345678` 正常采购 | ✅ |

## 22. 第三方系统对接（采购方适配器：异次元 acg-faka / 萌次元 mcy-shop）（ACG / MCY，10 条）

来源：`docs/protocol/third-party/acg-faka.md` §6、`mcy-shop.md` §5（对方源码中的历次修复与其自带客户端的缺陷）。
测试：`crates/infra/src/integration/{php_form,acg_faka,mcy_openapi}.rs` 单元测试，
`crates/api/tests/integration_acg_faka_adapter.rs`、`integration_mcy_adapter.rs`（进程内模拟对方服务端）。

#### ACG-01 签名按原始字段现算；密钥绝不随请求发送

- 严重度: **高**
- 问题现象: acg-faka 自带客户端把 `app_key` 明文放进表单（且参与签名）；签名只删顶层空串、嵌套 `sku[..]` 保持插入序，重建 map 再签会算错。
- **对我们实现的要求**: 按 `Str::generateSignature` 语义对发送的字段对签名（稳定排序顶层键），只发 `app_id` + `sign`。
- **必测用例**: 规格 §2.4 V1–V3 向量与 V3 请求体逐字节一致；端到端测试中所有请求都不含 `app_key` 字段且被模拟服务端验签通过。

#### ACG-02 `request_no` 为 char(19) 唯一列、重复即报错且不能按它查单：丢失应答的 trade 绝不能重复购买

- 严重度: **高**
- 问题现象: 规格建议 ≤64 位，但源码 `Install.sql:385` 为 `char(19)`；重复 `request_no` 返回 `The request ID already exists` 而非原订单。
- **对我们实现的要求**: `request_no = "Z" + md5(本地单号)[0..18]`（19 位、重试不变）；传输失败按可重试处理（重试会被对方去重），收到重复报错映射为 `upstream_result_unknown`（不重试、待人工核对）。
- **必测用例**: 对方已扣款但连接中断 → 采购单 failed；重试 → `manual_review` 且提示人工核对（本地订单不回退），对方只有 1 笔订单、余额只扣一次，两次请求 `request_no` 相同；管理端重试仍不重复购买，取消（标记失败）后才回退。

#### ACG-03 3.7.x 的 `items` 不含拿货价：必须读 `item`（3.6.5+ #842）或 `inventory`

- 严重度: **中**
- **必测用例**: 导入与全量同步的 SKU 价格 = `category_factory`（8.50），不是零售价（10.00）。

#### ACG-04 `trade`/`query` 的 `secret` 可能是提示文案（人工发货、售罄、风控审核），acg 自带客户端直接当卡密

- 严重度: **中**
- **对我们实现的要求**: 人工发货商品不按提示文案自动完成（`upstream_order_id=0`，状态 `awaiting_manual_check`）：trade 返回的 `secret` 存为基线，之后 `query` 内容与基线不同且不是已知提示文案 → 视为站长已人工发货；否则继续轮询，24 小时告警；自动发货商品识别提示文案。
- **必测用例**: 人工发货商品下单后轮询仍为 accepted，不写交付（已知提示文案变化也不算）；站长手动发货后 `query` 内容与 trade 时的基线不同 → 交付且只交付一次（interop-report 问题 1）。

#### ACG-05 3.5.9 控件 JSON 以 URL 编码入库；占位封面 `/favicon.ico`

- 严重度: **低**
- **必测用例**: `%5B…` 开头的 widget 能解析为表单 schema；占位封面不作为商品图片。

#### MCY-01 萌次元 `trade_no` 去重未经验证：丢失应答的 trade 不重试

- 严重度: **高**
- **必测用例**: 对方已扣款但连接中断 → 采购单 `manual_review`（`upstream_result_unknown`，本地订单不回退），再次 submit 不再调用 trade，余额只扣一次。

#### MCY-02 acg-faka 的萌次元客户端把所有异常吞成「连接失败#0」

- 严重度: **中**
- **必测用例**: 余额不足 → 采购单 rejected 且 error_message 含对方原文「余额不足」。

#### MCY-03 `contents` 缺失或为售后文案时不能当交付（acg 客户端写死「此商品没有发货信息或正在发货中」）

- 严重度: **中**
- **必测用例**: `contents = 库存不足，请申请售后` → 采购单 `manual_review`（保留 trade_no）、无交付。正常 `contents` 与 trade 单号同一事务落库（`procurement_deliveries`），重启不丢交付。

#### MCY-04 按 SKU 名称映射（acg `shared_mapping`）改名即断链

- 严重度: **低**
- **必测用例**: 对方把 SKU 改名并调价 → 同步后 SKU 映射仍指向同一 `sku_id`，价格跟随。

#### ACG-06 控件 `regex` 由对方在 `trade` 中 `preg_match("/{regex}/")` 校验——此时本站买家已付款（实机互通发现）

- 严重度: **高**
- **现象**: 真实 acg-faka 3.7.9 商品带控件 `{"name":"account","regex":"^[0-9]{6,12}$","error":"账号格式不正确"}`；适配器只把控件映射为必填字段、未带正则，
  买家填 `abc` 仍可钱包付款，随后采购单被对方以「账号格式不正确」拒绝（rejected，需人工退款）。
- **对我们实现的要求**: 控件正则若能被本站表单校验器以相同语义（无分隔符、无修饰符、非锚定搜索）编译，就写入 `manual_form_schema.regex`，在结算时校验；
  PCRE 专有语法（环视、反向引用）或以 `/` 开头的写法仍交给对方校验。萌次元适配器复用同一映射。
- **必测用例**: `acg06_widget_regex_is_copied_when_portable`（`acg_faka.rs`）；实机：`account=abc` 下单被拒且不扣款。

## 23. 供货方兼容：实机互通发现（PRV-08、PRV-09）

来源：`docs/protocol/third-party/interop-report.md`（真实 acg-faka 3.7.9 容器 ↔ 本站）。
测试：`crates/app/src/integration/provide/desk.rs` 单元测试、`crates/api/tests/integration_provider_acg_faka.rs`。

#### PRV-08 本地化规格值被当成「规格名 → 值」映射

- 严重度: **中**
- **现象**: 后台与种子数据把 SKU `spec_values` 存成一个本地化值 `{"zh-CN","zh-TW","en-US"}`，`SupplyDesk` 却按 `{规格名: 值}` 拼接所有值，
  acg-faka 导入后种类名为 `空月祝福 / 空月祝福 / Welkin`（萌次元 SKU 名同样）。
- **对我们实现的要求**: 与前台 `utils/sku.ts` `isLocalizedObject` 一致：键全为语言代码的对象是**一个**值，取 zh-CN（回退 en-US、zh-TW），为空时用 sku_code。
- **必测用例**: `prv08_localized_spec_value_is_one_name`（desk 单测）、`prv08_localized_spec_value_is_one_race`（e2e：items/item 种类名与 trade）。

#### PRV-09 重放 `trade` 的 `stock` 为 `null`

- 严重度: **低**
- **对我们实现的要求**: 原版 `trade` 的 `stock` 恒为字符串；重放时按请求的 code/race 读当前库存，读不到为 `"0"`。
- **必测用例**: `acg_faka_downstream_end_to_end` 中同一 request_no 第二次 trade 的 `stock == "1"`。


## 24. Live QA 2026-09-26（管理后台实机测试，QA-A，22 条）

来源：`docs/qa/live-admin.md`（Playwright 实机测试 store.dot2.com / zs2.dot2.com）。编号 `QA-Axx` 对应报告中的 `I-xx`。
“原项目同样存在”的条目，凡涉及资金或安全，本项目**仍然修复**（偏离原行为），并在此注明。

#### QA-A01 通用设置接口明文返回密钥、接受任意键（P1，安全）

- 严重度: **高**
- **现象**: `GET /admin/settings?key=smtp_config` 原样返回 SMTP 密码（`captcha_config`、`telegram_auth_config`、`notification_center_config` 同理）；`PUT /admin/settings` 可写任意键（遗留 `foo_bar_unknown`），绕过各页的校验。
- **与原项目差异**: 原项目 `admin_handler.go` Get 同样直出；本项目收紧。
- **对我们实现的要求**: 通用接口只接受白名单键（`schema::KNOWN_KEYS`，未知键 400 `error.setting_key_invalid`）；`telegram_bot_runtime_status` 与合规确认只读；含密钥的键按各自的 masked 形状返回（`has_password` 等），通用写入走对应专用接口的补丁逻辑（校验 + 空密钥保留旧值）。遗留的未知键用 `zebra-store admin prune-settings [--apply]` 清理。
- **必测用例**: `content_settings.rs::qa_a01_generic_settings_mask_secrets`、`generic_settings_get_put_normalize`；`schema::tests::qa_a01_key_whitelist`；`zs-infra tests/backup.rs::qa_a01_prune_unknown_settings_keys`。

#### QA-A02 订单状态下拉可直接设为“已退款”，不退钱不退库存（P1，资金）

- 严重度: **高**
- **与原项目差异**: 原项目允许该迁移；本项目禁止。
- **对我们实现的要求**: `PATCH /admin/orders/:id` 拒绝 `refunded` / `partially_refunded`（`error.order_status_refund_required`），父单与子单都一样；退款只能走 `refund-to-wallet` / `manual-refund`（按可退金额限额，不会重复退）。前端状态下拉不提供这两个状态。
- **必测用例**: `order_admin.rs::qa_a02_status_change_cannot_fake_refund`（伪退款被拒 → 全额真实退款成功一次 → 再退被拒）；admin `orderUtils.test.ts` “QA-A02 …”。

#### QA-A03 后台登录限流按 IP 统计所有请求（P1）

- 严重度: **高**（同一出口 IP 的团队互相锁死；2FA “错 5 次失效”规则被 IP 限流抢先触发）
- **与原项目差异**: 原项目同样按 IP 计所有尝试。本项目与前台登录一致按（账号, IP）计，且只计失败。
- **对我们实现的要求**: `/admin/login` 键为 `username|ip`，`/admin/login/verify-2fa` 键为 `ip|2fa:challenge`；先 `blocked()` 检查，业务失败（非 5xx）才 `record_failure()`，成功 `reset()`。
- **必测用例**: `admin_auth.rs::qa_a03_login_limiter_counts_failures_per_account`；`identity::rate_limit::tests::qa_a03_failure_only_limit`；`identity_user_auth.rs::login_rate_limits`（管理员段）。

#### QA-A04 返利设置保存后 `/public/config` 缓存未失效（P2）

- 严重度: **中**
- **对我们实现的要求**: 设置写入统一在 `SettingsService::write` 中执行 `schema::effects`；所有进入 `/public/config` 的键（site、affiliate、payment、smtp、captcha、telegram/google 登录、wallet、registration、nav、公告）都声明 `InvalidatePublicConfig`。
- **必测用例**: `content_settings.rs::qa_a04_affiliate_save_invalidates_public_config`；`schema::tests::qa_a04_public_config_keys_invalidate_cache`。

#### QA-A05 “检测更新”对话框请求不存在的 `/admin/system/version`（P2）

- **对我们实现的要求**: 对话框用 `GET /admin/system/version/check` + `/admin/system/update/capability`（与原项目一致），`can_update=false` 时显示“当前部署方式不支持一键升级”。
- **必测用例**: admin `src/composables/useSystemUpdateInfo.test.ts`（QA-A05 ×2）。

#### QA-A06 手册写 `admin reset-2fa`，程序只认 `reset2fa`（P2）

- **对我们实现的要求**: 子命令名 `reset-2fa`，保留别名 `reset2fa`；CLI 错误输出翻译后的文字加原 key。
- **必测用例**: `zs-server cli::tests::qa_a06_reset_2fa_names`、`qa_a07_cli_errors_are_translated`。

#### QA-A07 41 个 `error.*` key 没有翻译，界面显示原始 key（P2）

- **对我们实现的要求**: 后端源码中出现的每个 `error.*` 在 `messages.json` 三种语言都有文案（新增 key 同步三语）。
- **必测用例**: `zs-api i18n::tests::qa_a07_every_error_key_is_translated`（扫描全部 crate 源码）；`marketing_member_level.rs::backfill_requires_default_level`。

#### QA-A08 前台“自定义脚本”纯 JS 不执行（P2）

- **对我们实现的要求**: 与原项目一致：不含 `<` 的代码包成新建的 `<script>`；HTML 片段中的 `<script>` 重新创建为可执行节点；受管节点在列表变化时清除后重放。
- **必测用例**: storefront `tests/customScripts.test.ts`（QA-A08 ×3）。

#### QA-A09 回调路由与内置路由冲突、`..` 路径被静默接受（P2）

- **与原项目差异**: 原项目对保留前缀/重复路径静默置空并提示保存成功；本项目改为拒绝。
- **对我们实现的要求**: 写入时校验（`validate_callback_routes`）：必须 `/api/` 开头、非保留前缀、段内无 `.`/`..`/空段/非法字符、不等于任一内置回调路径（支付 callback、dujiaopay/paypal/stripe webhook、`/api/v1/upstream/callback`）、互不重复；否则 400 `error.callback_route_invalid`，原值不变。
- **必测用例**: `integration::tests::qa_a09_callback_routes_validation`；`content_settings.rs::set_01_callback_routes_normalized_via_api`。

#### QA-A10 卡密重复导入生成第二份可售卡密（P2，资金）

- **与原项目差异**: 原项目只在单次导入内去重；本项目在 `deduplicate=true`（默认）时同时对该 SKU 现存（未删除、任意状态）卡密去重，全部重复时 400 `error.card_secret_all_duplicate`。`deduplicate=false` 保留原语义（允许同一卡密多份）。
- **必测用例**: `catalog_card_secret.rs::qa_a10_a11_reimport_and_sold_secret_guards`（手动批量与 CSV 导入）；`card_secret::tests::qa_a10_existing_secrets_are_dropped`。

#### QA-A11 已售出卡密可改回“可用”再次出售（P2，资金）

- **与原项目差异**: 原项目同样未拦截；本项目 `used` 为终态。
- **对我们实现的要求**: 单条修改从 `used` 改为其他状态 → 400 `error.card_secret_used_locked`；批量改状态用条件 UPDATE 跳过 `used` 行（返回实际影响行数）。
- **必测用例**: 同 QA-A10 的集成测试；`card_secret::tests::qa_a11_used_secret_is_terminal`。

#### QA-A12 钱包扣减超额提示“支付金额不匹配”（P2）

- **与原项目差异**: 原项目同样映射到 `payment_amount_mismatch`；管理员调整改为 `error.wallet_insufficient_balance`（其他流程不变）。
- **必测用例**: `wallet_account.rs::wal_02_admin_adjust_rules_and_audit`；`wallet::account::tests::wal_02_admin_adjust`。

#### QA-A13 仍被用户使用的会员等级可以删除（P2，数据完整性）

- **与原项目差异**: 原项目同样可删；本项目拒绝（`error.member_level_in_use`），先调整用户等级。
- **必测用例**: `marketing_member_level.rs::qa_a13_level_in_use_cannot_be_deleted`。

#### QA-A14 自定义角色缺少 `GET:/admin/authz/me` 时无法登录（P2）

- **与原项目差异**: 原项目同样需要显式授权；本项目把“自身账号”路由（`authz/me`、`compliance/status`、`2fa/*` 自助、`PUT /admin/password`，即 `readonly_auditor` 的自助部分）对所有已登录管理员放行（`authz::SELF_SERVICE_ROUTES`）。
- **必测用例**: `identity_rbac.rs::qa_a14_self_service_routes_need_no_grant`。

#### QA-A15 上传错误文案硬编码中文；上传失败提示 “Upload failed: 1”（P2）

- **对我们实现的要求**: 上传校验错误改为 i18n key + 参数（`error.upload_*`，zh-CN 文案与原项目一致）；SVG 细节作为参数附带。前端逐个文件显示后端原因。
- **必测用例**: `content_media.rs::upl_05_validation_errors`（en-US / zh-TW）；`content::media::tests::upl_05_validation_messages`；admin `views/content/useMedia.test.ts`（QA-A15 ×2）。

#### QA-A16 Docker 部署无法按手册用 `sqlite3` 备份（P2，运维）

- **对我们实现的要求**: 内置 `zebra-store backup [--output 文件]`（SQLite `VACUUM INTO`，在线、一致、不覆盖已存在文件）；MySQL/PostgreSQL 提示用 `mysqldump` / `pg_dump`。手册 `deploy/backup-upgrade`、`deploy/docker` 已更新。
- **必测用例**: `zs-infra tests/backup.rs::qa_a16_sqlite_online_backup`。

#### QA-A17 受限角色看到无权限的操作（P3）

- **必测用例**: admin `orderUtils.test.ts` “QA-A17 a read-only order role …”、`dashboardUtils.test.ts` “QA-A17 only shows quick links …”。

#### QA-A18 Banner 链接接受 `javascript:`（P3，安全）

- **对我们实现的要求**: 外链必须 `http(s)://`；站内链接不得带协议或以 `//` 开头（先去掉空白/控制字符再判断）。
- **必测用例**: `content::banner::tests::rejects_invalid_links_and_windows`。

#### QA-A20 非法输入被静默修正（P3，部分）

- **已修**: 会员等级折扣率须在 0–100、阈值不得为负；首页公告结束时间早于开始时间被拒。
- **必测用例**: `marketing_member_level.rs::qa_a20_level_values_are_range_checked`；`schema::tests::qa_a20_announcement_window`。

#### QA-A21 / QA-A22 / QA-A23 / QA-A29 管理端小问题（P3）

- 自定义日期范围按本地日期；审计日志角色筛选自动补 `role:`；中文以外用半角冒号；草稿按钮文案；已完成与已退款徽章颜色区分。
- **必测用例**: admin `dashboardUtils.test.ts`（QA-A21）、`authzUtils.test.ts`（QA-A22）、`format.test.ts` / `contentUtils.test.ts`（QA-A23）、`orderUtils.test.ts`（QA-A29）。

#### QA-A24 容器日志带 ANSI 颜色转义（P3）

- **对我们实现的要求**: 仅 stdout 为终端且未设置 `NO_COLOR` 时输出颜色。
- **必测用例**: `zs-server tests::qa_a24_no_ansi_without_terminal`。

#### QA-A27 列表类配置无法用环境变量覆盖（P3）

- **对我们实现的要求**: 所有 `Vec` 配置（`cors.allowed_origins`、`server.trusted_proxies`、`upload.allowed_types/extensions`、`reseller.main_hosts`）都按逗号分隔解析。
- **必测用例**: `zs-server settings::tests::qa_a27_list_values_from_env`。
