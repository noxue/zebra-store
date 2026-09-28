# Zebra Store 对接协议 v1（`zebra-store`）

站点之间"供货方 ↔ 采购方"的对接协议，与原有的 `dujiao-next` 协议**并存**：
对接管理里的「连接协议」可选 `dujiao-next`（兼容原项目及其所有站点）或 `zebra-store`（本协议）。
我们的站点**同时作为供货方提供两套接口**，作为采购方也能用两种协议连接上游。

## 0. 相比 dujiao-next 解决了什么

| 痛点（dujiao-next） | zebra-store 的做法 |
|---|---|
| 接入要分别抄地址 / Key / Secret / 回调地址 / 汇率，易填错 | **连接码**一次粘贴；**握手**自动返回站点名、结算货币、能力、余额，自动填回调地址与建议汇率 |
| 5 分钟轮询 + 全量翻页，库存延迟、易超卖，删除需全量扫描识别（UPS 教训多次修复） | **变更流** `/catalog/changes` 游标增量拉取（含删除标记）；供货方变化时 **webhook 通知**，采购方立即拉取 |
| 下单时价格可能已变 → 失败或多扣；一单只能一个 SKU；余额不足到下单才知道（402） | **报价锁价** `/orders/quote`（锁价 N 分钟、返回余额是否足够）；**多商品订单**；`Idempotency-Key` 防重复下单 |
| 签名用 MD5 摘要、无随机数（窗口内可重放）；换密钥必须两端同时停机 | **HMAC-SHA256 + SHA-256 摘要 + nonce 防重放**；**双密钥轮换**（新旧并行 7 天） |
| 错误码不统一、无可否重试；回调无事件 ID；卡密明文出现在回调中 | 统一错误对象（`code`/`retryable`/`request_id`）；事件带唯一 `id` 去重；卡密 **AES-256-GCM 加密**传输 |

## 1. 基本约定

- 路径前缀：`/api/v1/zs`（与 `/api/v1/upstream`（dujiao-next）并存）。
- 请求/响应 `Content-Type: application/json; charset=utf-8`，时间 RFC 3339 UTC，金额两位小数字符串（`"12.30"`）。
- 成功：`{"ok": true, "data": …}`；失败：`{"ok": false, "error": {"code", "message", "retryable", "request_id"}}`，并使用真实 HTTP 状态码（见 §8）。
- 请求体上限 1 MiB；列表 `limit` 默认 50、最大 200。

## 2. 鉴权与签名

采购方使用供货方为其签发的 API 凭证（沿用 `api_credentials`：审核通过 + 启用 + 用户状态正常）。

请求头：

| 头 | 说明 |
|---|---|
| `ZS-Key` | API Key |
| `ZS-Timestamp` | Unix 秒；与服务器时间偏差 ≤ 300 s |
| `ZS-Nonce` | 16–64 位 `[A-Za-z0-9_-]`，同一 Key 在 10 分钟内不可重复（防重放） |
| `ZS-Signature` | `v1=<hex>` |

签名：

```
canonical = "ZS1\n" + METHOD + "\n" + PATH + "\n" + CANONICAL_QUERY + "\n"
          + TIMESTAMP + "\n" + NONCE + "\n" + hex(SHA256(body))
signature = hex(HMAC_SHA256(secret, canonical))
```

- `PATH` 为不含查询串的路径（如 `/api/v1/zs/catalog/products`）；`CANONICAL_QUERY` 为按键名、再按值排序后以 `&` 连接的 `k=v`（值按 RFC 3986 百分号编码），无查询串为空行；空 body 取空串的 SHA-256。
- 服务端常量时间比较；拒绝空密钥；签名/时间/nonce 任一不通过统一 `401 unauthorized`（不泄露具体原因）。
- nonce 在**验签通过之后**登记（存数据库，多实例共享；未认证请求不会占用 nonce），10 分钟后清理。
- **密钥轮换**：凭证可有 `secret_next`（用户在个人中心「API 对接」点击轮换生成，只显示一次）。7 天内新旧密钥均可验签；采购方首次用新密钥成功签名后，旧密钥立即作废；7 天到期自动晋升。轮换对同一凭证的 `dujiao-next` 接口同样生效。
- 限流：每个 API Key 每分钟 120 次（`limits.requests_per_minute`），超出返回 `429 rate_limited` 并带 `Retry-After`（秒）。

## 3. 连接码（一键接入）

供货方用户在个人中心「API 对接」生成连接码（包含密钥，**只显示一次**，可随时重新生成——重新生成等同密钥轮换）：

```
zsc1_<base64url(JSON)>
JSON = {"v":1,"url":"https://supplier.example.com","key":"…","secret":"…","name":"供货方站点名"}
```

本项目实现：`POST /api/v1/api-credential/connection-code` → `{code, rotation_expires_at}`，新密钥作为 `secret_next`（§2 轮换规则），`url` 取站点设置 `brand.site_url`，未设置时取请求来源（`Origin` / `Host`）。

采购方在「对接管理 → 新建连接」粘贴连接码（`POST /api/v1/admin/site-connections/parse-code`）：自动解析地址/Key/Secret、协议设为 `zebra-store`，立即调用握手（§4）预填名称、货币、建议汇率与回调地址；用户确认后保存。

## 4. 握手与能力协商

`GET /api/v1/zs/handshake` →

```json
{
  "protocol": "zebra-store", "version": "1.0",
  "site": {"name": "…", "url": "https://…", "currency": "CNY"},
  "features": ["changes", "webhooks", "quote", "multi_item", "idempotency", "encrypted_delivery"],
  "limits": {"requests_per_minute": 120, "max_items_per_order": 20, "quote_ttl_seconds": 600, "changes_retention_days": 7},
  "account": {"user_id": 1, "balance": "100.00", "currency": "CNY", "member_level": {…}|null},
  "catalog": {"latest_seq": 1030},
  "server_time": "2026-09-25T06:00:00Z"
}
```

采购方以 `features` 决定使用哪些能力（未来版本新增能力时老客户端自动降级）。
`catalog.latest_seq` 为当前变更流最新序号（`0` 表示尚无变更），采购方全量同步前记录它，全量完成后从这里继续增量（§5）。

> 线上 `features` 使用本协议自己的名称；我们的 `zebra-store` 适配器把它们映射为系统统一的能力 ID
> （`changes→incremental_changes`、`webhooks→push_events`、`quote`、`multi_item`、`idempotency`、`encrypted_delivery`，另恒有 `categories`），
> 管理端接口、连接记录里的 `features` / `capabilities` 一律使用能力 ID（见 §9）。

## 5. 目录与增量同步

- `GET /catalog/categories` → `[{id, parent_id, slug, name{}, icon, sort_order}]`
- `GET /catalog/products?cursor=&limit=` → `{items, next_cursor, has_more, total}`：按 `id` 游标全量遍历（首次建立映射用；包含已下架商品，`is_active=false`）。商品结构同 dujiao-next 的 upstream 商品（含 SKU 真实可售库存 `stock_quantity`，-1 为无限；批发阶梯；`is_active`；会员价已按调用方计算），外加 `version`（商品内容摘要，不含价格与库存）。`next_cursor` 为字符串（最后一个商品 id），无更多时为空串；`total` 为在售+下架的商品总数。
- `GET /catalog/products/{id}` → 单个商品（已删除 → `404 not_found`）。
- `GET /catalog/changes?since=<cursor>&limit=` → `{changes:[…], next_cursor, has_more}`

  ```json
  {"seq": 1024, "type": "product.upserted" | "product.deleted" | "sku.stock" | "sku.price",
   "product_id": 12, "sku_id": 34|null, "at": "…",
   "data": {"product": {…}} | {"stock_quantity": 5, "stock_status": "low_stock"} | {"price_amount": "9.90"}}
  ```

  - `since` 缺省 = 从最早保留的变更开始；游标过期（超出保留期）返回 `410 cursor_expired`，客户端改走一次 `/catalog/products` 全量后以全量**开始前**取得的握手 `catalog.latest_seq` 继续（期间的变更会被重放，按幂等处理）。
  - `next_cursor` 为字符串形式的序号（无新变更时等于 `since`）；`has_more` 表示还有下一页。
  - `sku.price` 的价格为基础价（不含会员价）；采购方收到任何非删除变更后应重新 `GET /catalog/products/{id}` 取自己的价格。
  - 变更由供货方的**目录快照比对任务**产生（每 15 s 比对商品内容摘要、SKU 价格与可售库存），与商品/卡密/订单代码解耦，能捕获卡密消耗、手工库存、上下架、删除等所有来源的变化。内容摘要变化只产生一条 `product.upserted`（带完整商品）；仅价格/库存变化产生 `sku.price` / `sku.stock`。保留期内始终至少保留最新一条变更。

## 6. Webhook 推送

- `PUT /webhooks` `{url, events:["catalog.changed","order.*","account.balance_low"], balance_low_threshold?:"10.00"}` 注册/更新本凭证的推送地址（采购方在建立连接时自动注册其 `/api/v1/zs/events` 地址）；`events` 缺省为上述三项，支持 `order.*` 通配；`GET /webhooks` 查询（未注册为 `null`）；`DELETE /webhooks`。地址必须是 http(s)；生产策略下拒绝私网/本机字面地址（`400 invalid_request`），发送时再按解析结果校验（UPS-01/10）。
- 事件体：`{"id":"evt_…","type":"…","created_at":"…","data":{…}}`，用同一签名方案（§2：`PATH` / `CANONICAL_QUERY` 取自 webhook URL，签名密钥为该凭证当前密钥）以 `POST` 发送，并带 `ZS-Event-Id`；
  采购方以 `id` 去重，2xx 视为成功；首次失败后按 30 s / 2 min / 10 min / 1 h / 6 h 重试，最多 5 次，之后标记失败。出站请求仅限公网地址、不跟随重定向（UPS-01）。
- 事件类型：
  - `catalog.changed` `{"latest_seq": 1030}` —— 通知式，采购方收到后调 `/catalog/changes` 拉取；
  - `order.status_changed` / `order.delivered` / `order.refunded` —— 数据同 §7 订单对象；
  - `account.balance_low` `{"balance":"8.00","threshold":"10.00"}`（下单扣款后余额低于 webhook 注册时的 `balance_low_threshold`，24 h 内最多一次；未设置阈值则不发送）。
  - 通过 `POST /orders` 且 `callback=true` 下的订单才会推送 `order.*`；子订单的状态变化以父订单对象推送。

## 7. 报价与下单

- `POST /orders/quote` `{"items":[{"sku_id":34,"quantity":2,"manual_form_data":{…}}]}` →
  `{"quote_id":"q_…","expires_at":"…","currency":"CNY","items":[{"sku_id","quantity","unit_price","subtotal","available":true,"reason":null}],"total":"19.80","balance":"100.00","sufficient_balance":true}`
  - 报价锁定单价（含会员价/批发价/活动价，使用与商城下单相同的定价引擎）至 `expires_at`（`quote_ttl_seconds`），不锁库存；不可下单的行给出 `reason`（`out_of_stock`/`inactive`/`quantity_limit`/`form_invalid`），不计入 `total` 也不进入锁价。
- `POST /orders`（请求头 `Idempotency-Key` 必填，1–64 个可见 ASCII 字符）
  `{"quote_id":"q_…"|null,"items":[…],"downstream_order_no":"…","trace_id":"…","callback":true}`
  - 有有效报价时按报价价格成交：报价单价是**上限**（当前价更低则按当前价，更高则按报价价，差额计为优惠）；报价过期 → `409 quote_expired`，报价不存在/不属于本凭证/内容与报价不一致 → `422 quote_mismatch`；无报价时按当前价格。
  - 从采购方在供货方的钱包余额扣款，全部成功才下单（一单多行，内部为父子订单）；余额不足 `402 insufficient_balance`（下单前预检，扣款失败的订单立即取消并释放库存）；不可售行 `422 item_unavailable`。
  - 同一 `Idempotency-Key` 24 h 内重复请求返回该订单的**当前**订单对象（请求体不同则 `422 idempotency_conflict`；首个请求仍在处理中则 `409 request_in_progress`，可重试）；失败的请求不占用 Key。同一 `downstream_order_no` 同样幂等（返回既有订单）。
  - 响应为订单对象：
    `{"order_no","downstream_order_no","status","currency","total","items":[{"sku_id","product_id","quantity","unit_price","subtotal","status","delivery":…}],"created_at"}`（每行对应一个子订单，`status` 为子订单状态）
- `GET /orders/{order_no}`、`GET /orders?downstream_order_no=…`（`{items,next_cursor,has_more}`；不带参数时按订单 id 倒序列出本凭证的订单，`cursor` 翻页）、`POST /orders/{order_no}/cancel`（仅**待支付**订单可取消；API 订单下单即扣款，已付款订单返回 `409 order_not_cancelable`，退款由供货方管理员处理）。
- **交付加密**：订单对象与事件中的 `delivery` 为
  `{"encrypted": true, "alg": "A256GCM", "nonce": "<b64>", "ciphertext": "<b64>"}`，
  密钥 = `SHA256("zs-delivery:" + secret)`（发送时凭证的当前密钥），明文为 `{"type","payload","delivery_data","delivered_at"}`。采购方解密后写入本地发货记录；未交付的行 `delivery` 为 `null`。

## 8. 错误码

| HTTP | code | retryable | 说明 |
|---|---|---|---|
| 400 | `invalid_request` | false | 参数缺失/格式错误（`message` 列出字段） |
| 401 | `unauthorized` | false | 签名/时间戳/nonce/密钥无效 |
| 403 | `forbidden` | false | 凭证未审核、已禁用或用户被禁用 |
| 404 | `not_found` | false | 商品/订单不存在 |
| 409 | `quote_expired` / `order_not_cancelable` | false | |
| 409 | `request_in_progress` | true | 同一 `Idempotency-Key` 的首个请求尚未完成 |
| 410 | `cursor_expired` | false | 变更游标超出保留期 |
| 402 | `insufficient_balance` | false | 余额不足 |
| 422 | `quote_mismatch` / `idempotency_conflict` / `item_unavailable` | false | |
| 429 | `rate_limited` | true | 带 `Retry-After` |
| 500/503 | `internal_error` / `unavailable` | true | |

## 9. 采购方实现要点（本项目）

采购方按**适配器模式**实现，核心服务不含任何按协议分支的代码：

- 每个对接系统是一个 `SupplierAdapter`（`zs_domain::integration::adapter`），在 `AdapterRegistry`（`zs_infra::integration::registry`）中以协议 ID 注册：
  `dujiao-next` → `DujiaoNextAdapter`（原行为不变），`zebra-store` → `ZebraStoreAdapter`（本协议）。`site_connections.protocol` 选择适配器。
- 适配器提供元数据（名称/说明三语、配置字段表单、能力、是否支持连接码、入站路径）、出站客户端（`UpstreamClient`：握手、目录、下单、查单、取消；可选的变更流、报价、事件注册）、入站请求的预检与验签解析、连接码解析。
- 能力 ID 全系统统一：`categories`、`incremental_changes`、`push_events`、`quote`、`multi_item`、`idempotency`、`encrypted_delivery`。
  握手后连接记录保存 `features`（供货方提供的能力 ID）、`capabilities`（本连接生效的能力）、`supplier_currency`、`sync_mode`、`last_change_seq`、`webhook_status`、`handshake_error`（额外表 `integration_connection_states`，适配器专用配置存其 `extra` JSON）。
- 新建/修改连接、测试连接时执行适配器握手并保存上述状态；握手失败不阻止保存（记录错误，`webhook_status` 为 `failed`/`unsupported`）。握手成功的待定连接自动激活。
- 同步：有 `incremental_changes` 能力时由增量游标驱动（游标存连接状态），收到 `catalog.changed` 立即排队拉取；无游标或 `410 cursor_expired` 时先取 `latest_seq` 再做一次全量；否则退回原全量/增量同步。任何变更商品都重新读取单品后走原同步计划（UPS-04/08/13/14）。
- 采购：有 `quote` 能力时先报价（不可售行或余额不足按不可重试/可重试分类，UPS-11），再下单并带 `Idempotency-Key = procurement:{id}` 与报价 ID；供货方订单以 `order_no` 标识（`dujiao-next` 以数字 id）。交付解密后走原履约流程（幂等）。
- 事件接收端：`POST /api/v1/zs/events`（`dujiao-next` 仍为 `POST /api/v1/upstream/callback`），由适配器按 `ZS-Key` 预检并用该连接密钥验签解析，之后统一进入协议无关的入站核心：连接必须为启用状态且协议一致、按事件 `id` 去重（仅处理成功后记录）、状态机守卫与所属校验（连接 + 已登记的供货方订单号，沿用 UPS-02/03）。
- 出站地址策略：默认仅公网地址（解析后校验，防 DNS rebinding）且从不跟随重定向；`integration.allow_private_addresses: true`（环境变量 `ZS__INTEGRATION__ALLOW_PRIVATE_ADDRESSES`）仅用于可信局域网/测试，启动时会输出警告。
