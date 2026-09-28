# 供货方兼容协议（异次元 acg-faka / 萌次元 mcy OpenApi）

> 让运行**异次元发卡（acg-faka）**的站点，以及说**萌次元 mcy-shop OpenApi 插件协议**的客户端
> （acg-faka 的「萌次元(V4.0)」店铺类型、gmshop-edge 等），把**本站**当作上游货源接入。
> 线协议依据：`acg-faka.md`（§3、§7.2）、`mcy-shop.md`（§2、§4.2）。
> 反方向（本站从这些系统进货）是采购方适配器，见 `docs/BACKEND_GUIDE.md` §9，不在本文范围。

## 1. 架构

```
POST /shared/*            ─► routes/integration/provide.rs ─► provide::acg_faka::AcgFaka   ┐  验签 + 请求/响应映射
POST /plugin/open-api/*   ─►            (薄 handler)        ─► provide::mcy::McyOpenApi    ┘
                                                                    │
                                              provide::access::CompatAccess   app_id → Caller（凭证/用户/开关/白名单/验签回调）
                                              provide::desk::SupplyDesk       目录、按调用者定价、库存、钱包下单（幂等）、查单、余额
                                                                    │
                     SupplierCatalog（目录读） · UpstreamOrdering（订单组：price_lines / place / get / deliver_now） · OrderRefRepo（幂等）
```

- **核心 `SupplyDesk`**（`crates/app/src/integration/provide/desk.rs`）与协议无关，只说领域类型：
  `site/currency/base_url/balance`、`project`（会员价投影）、`offers`（按分类的可售商品树）、`offer/offer_of_sku`、
  `check`（数量限制 + 库存）、`quote`（商城同一定价引擎：会员/批发/促销）、`place`（钱包扣款 + 按下游单号幂等 + 自动发货同步交付）、
  `resume`/`order_by_request`/`order`（查单）。
  dujiao-next（`SupplierService`）与 zebra-store（`ZsSupplier`）的商品投影、站点信息也已改为委托给同一个 desk（行为不变、原测试未改）；
  二者的下单/查单仍保留各自实现（协议特有的状态、回调与多行订单），未强行迁移。
- **门面**（`acg_faka.rs`、`mcy.rs`）只做：解析 PHP 表单（`form.rs`）→ 验签 → 调 desk → 拼装各自的 JSON 信封。门面之间没有互相依赖。
- **路由**挂在站点根路径（`RouteSet.root`），handler 只读取原始请求体/头部交给门面，永远返回 HTTP 200 JSON（协议开关关闭时 404）。
- **组装**：`crates/infra/src/wire/provide.rs` → `Services.provide`（与 `wire/integration.rs` 分开，采购方适配器与供货方门面可独立演进）。

## 2. 提供的端点

| 协议 | 路径（POST，`application/x-www-form-urlencoded`） | 说明 |
|---|---|---|
| acg-faka | `/shared/authentication/connect` | `{shopName, balance}` |
| | `/shared/commodity/items` | 全量「分类 → 商品」树（≤200 分类），无 `msg` 键 |
| | `/shared/commodity/item` | `code`：3.1.2+ 单商品详情（含 `factory_price`/`config.category_factory`）；只传 `sharedCode`（≤3.1.1 客户端）时返回只含该商品的树 |
| | `/shared/commodity/inventory` | `sharedCode, race`：`count`、零售/调用者价、INI `config`（含 `[category_factory]`）、`factory_price`、`is_category` |
| | `/shared/commodity/inventoryState` | `shared_code, card_id, num, race`：`data: []` 或 `库存不足` 等 |
| | `/shared/commodity/stock` | `code, race, sku[…]`：`{"stock":"12"}`（字符串） |
| | `/shared/commodity/valuation` | `code, num, race, sku[…], card_id`：`{"price":"17.00","currency_code":"CNY"}`（总价） |
| | `/shared/commodity/trade` | `shared_code, num, race, request_no, …`：`{url, amount, tradeNo, secret, leave_message, stock}`，卡密同步返回 |
| | `/shared/commodity/query` | `tradeNo`：`{secret, widget:null, status}`，仅本人订单 |
| | `/shared/commodity/draftCard`、`/draft` | 恒为 `{"list":[],"total":0}` / `{"draft_premium":0}`（不支持预选卡） |
| mcy OpenApi | `/plugin/open-api/connect` | `{username, balance}`（`username` 给站点名，不暴露用户邮箱） |
| | `/plugin/open-api/items`、`/item`（`id`） | `{id,name,introduce,picture_url,category:{name},widget:"[]",sku:[{id,name,stock_price,stock}]}` |
| | `/plugin/open-api/sku/stock`（`sku_id`） | `{stock:<int>}` |
| | `/plugin/open-api/sku/state`（`sku_id, quantity`） | `{state:bool}`，业务拒绝一律 `false` |
| | `/plugin/open-api/amount`（`sku_id, quantity`） | `{amount:"17.00"}`（总价） |
| | `/plugin/open-api/trade`（`sku_id, quantity, trade_no, <控件>…`） | `{contents, trade_no, order_no, amount}`；未交付时不带 `contents` |

信封：成功 `{"code":200,"msg":"success","data":…}`（acg 的 `items/item/stock/valuation/draftCard/draft` 与原版一样没有 `msg`），
失败 HTTP 200 + `{"code":0,"msg":"<中文文案>"}`（`商户ID不存在`、`密钥错误`、`库存不足`、`余额不足`、`该商品未开放对接`、`商品不存在`、`订单不存在` …）。

## 3. 字段映射

| 我们 | acg-faka | mcy |
|---|---|---|
| 商品 id | `id`，`code` = id 字符串（规范正整数，`01`/`0`/非数字 → `商品不存在`） | `id` |
| 分类 | 树的顶层 `{id,name(zh-CN),sort,icon,status:1,pid:0}`，空分类不输出 | `category:{name}` |
| SKU | 多 SKU（或唯一 SKU 带规格值）→ `[category]` 种类；种类名 = 规格值 `a / b`（本地化值 `{"zh-CN",…}` 取 zh-CN，PRV-08；否则 sku_code），`= . [ ] ; "` 替换为 `_`，重名追加 `#<sku id>`；唯一且无规格的 SKU 不输出种类，`race` 忽略 | `sku[]`，`name` 同左 |
| 零售价 | `price`、`[category]`（JSON 数字 / INI 两位小数） | — |
| 调用者价（商城定价引擎，最小购买量时的单价） | `user_price`、`factory_price`、`[category_factory]` | `stock_price`（字符串） |
| 库存 | 未售卡密数；无限库存 → `999999` | 同左 |
| 购买限制 | `minimum/maximum`（0 = 不限） | 下单时校验 |
| 控件 | `widget` 恒 `[]`（只提供自动发货商品，不需要下单表单） | 同左；多余的控件字段被忽略 |
| 描述 / 封面 | `description` = content（HTML）或 description；`cover` = 首图绝对 URL（品牌 URL，否则请求 origin） | `introduce` / `picture_url` |

## 4. 凭证模型

- `app_id` / `Api-Id` = **本站用户 ID**（两个系统的惯例：acg-faka `User::find(app_id)`、萌次元个人中心「API-ID = 用户 id」）。只接受规范写法的正整数。
- 用户必须有**已审核 + 启用**的 API 凭证（`api_credentials`，UPS-18）且账户为 active。
- `app_key` 是**独立**的随机密钥（32 位大写字母数字，AES-GCM 加密存于新表 `api_compat_keys`，每个凭证一行），
  **不是** dujiao-next / zebra-store 协议的 HMAC Secret：兼容协议只用 md5，acg-faka 客户端还会把密钥明文放进表单，泄露面不能波及更强的协议。
- 用户在「个人中心 → API 对接 → 异次元 / 萌次元 对接」生成/重置 `app_key`（重置后旧密钥立即失效）、开关兼容访问、设置 IP 白名单。
  接口：`GET|PUT /api/v1/api-credential/compat`（`{is_active?, ip_allowlist?}`）、`POST /api/v1/api-credential/compat/issue`。
- 管理员重新审核（`approve` 会换新 `api_key`）后，兼容密钥自动作废（行里记录了签发时的 `bound_api_key`），用户需重新生成。
- 管理员停用凭证、用户被禁用 → 立即拒绝（`商户ID不存在`，不区分原因，避免枚举）。

## 5. 签名

- acg-faka：对**收到的全部表单字段**（除 `sign`，含客户端顺带发送的 `app_key`、嵌套 `sku[…]`）按 PHP 语义现算：
  顶层键 `.`/空格 → `_`，`a[]` 自动编号，同名取最后一个；按顶层键字节序排序，丢弃顶层空串，值不转义，`&key=<app_key>`，小写 md5。
  `app_id` 必须是标量（`app_id[]=` → `商户ID不存在`）；比较为常量时间的**逐字节**比较（大小写敏感，`0e…` 不会被当作数字）。
- mcy：`Api-Signature` 对表单**顶层标量**字段计算（数组字段不参与，mcy `Str::generateSignature`）；无字段时签名串为 `&key=<app_key>`。
- 两份文档中的测试向量（acg V1–V3、mcy §2.4）都有单元测试（`form.rs`）与集成测试（按 PHP 客户端方式构造请求）。

## 6. 决策

1. **只提供自动发货（卡密）商品**（`DeliveryPolicy::Synchronous`）。acg-faka 把 `trade` 响应的 `secret`（萌次元类型是 `contents`）
   直接作为买家的最终交付并置已发货，**之后从不查询**。人工发货/上游转售商品的真实交付无法送达对方买家，所以：
   不出现在 `items`，按 id/code/sku 访问返回 `该商品未开放对接`，`trade` 同样拒绝（不扣款）。
   自动发货在请求内完成：`place` 后调用订单组新增的 `UpstreamOrdering::deliver_now`（对拆分后的子订单执行 `auto_fulfill`，
   与队列里的 `order:auto_fulfill` 幂等互斥），再读回卡密；极端情况下未能交付时 acg 返回原版文案「正在发货中…」，mcy 不带 `contents`。
2. **幂等**：`request_no`（acg）/ `trade_no`（mcy）写入 `downstream_order_refs`（`(credential, downstream_order_no)` 唯一），
   重复请求**返回原订单同形响应**（不再扣款，未交付的会再尝试交付），而不是 acg 原版的 `The request ID already exists` 报错——
   acg-faka 客户端超时后不会重试，gmshop-edge 依赖重发对账，返回原单对两者都安全。原单支付失败/已取消 → `该订单未支付成功`。
3. **余额预检**：下单前用定价引擎算出总价并比较余额，不足直接 `余额不足`（不创建订单）；实际扣款仍在订单组事务内条件更新。
4. **0 元拦截**：调用者价或零售价 ≤ 0 的商品询价/下单一律 `商品价格异常，暂停对接`（acg-faka `Order.php:875-889` 同款防护）。
5. **配置开关**：`integration.acg_faka_compat`、`integration.mcy_compat`，**默认 `true`**（环境变量 `ZS__INTEGRATION__ACG_FAKA_COMPAT` / `…MCY_COMPAT`）。
   理由：端点在用户显式生成兼容密钥之前完全无效（需管理员审核过的凭证 + 用户主动开通），默认开启不扩大攻击面；关闭时路径返回 404，
   个人中心显示「本站未开启」。
6. **币种**：`valuation` 带 `currency_code`（站点结算货币），connect/余额不换算。

## 7. 重放风险与缓解（残余风险）

两个协议都**没有时间戳/nonce**，签名只覆盖表单字段，任何截获的请求都可以原样重放；acg-faka 客户端甚至明文发送 `app_key`。协议范围内的缓解：

- 独立 `app_key`，可随时重置，泄露不影响 HMAC 协议；兼容访问可单独关闭；可选 **IP 白名单**（IP/CIDR，最多 20 条，按 `server.trusted_proxies` 解析的客户端 IP 判断；配置白名单后未知来源一律拒绝）。
- 限流：每个 `协议|IP|app_id` 每分钟 300 次（超限 30 s，返回 `请求过于频繁`）。
- 写操作幂等：重放 `trade` 只会得到原订单（不重复扣款）；重放不能改变数量/商品（签名覆盖全部字段）。
- 只能读到调用者自己的订单与余额。

**残余风险**（必须告知站长/用户，个人中心已提示）：截获请求者可以重放读取接口（目录、价格、余额），以及重放 `trade`/`query`
**再次获得该订单的卡密**。因此只应在 HTTPS 下使用，并尽量配置 IP 白名单；怀疑泄露时立即重置密钥。

## 8. 必测用例（PRV，均有自动化测试）

| 编号 | 场景 | 测试 |
|---|---|---|
| PRV-01 | `app_id` 必须是标量（数组不 500）；签名逐字节常量时间比较（magic hash、大小写、改字段后重放均拒绝）；非规范 id 不别名 | `form.rs::prv01_*`、`integration_provider_acg_faka::prv01_*`、`integration_provider_mcy` |
| PRV-02 | 只有开放对接（上架、分类启用、自动发货）的商品可按 code/id/sku 访问与下单（acg-faka 3.7.x `api_status` 闸门） | 两个 e2e 测试 |
| PRV-03 | 重复 `request_no`/`trade_no` 返回原单、不重复扣款 | 两个 e2e 测试 |
| PRV-04 | `draftCard` 不执行任何过滤（acg-faka `search-secret` 卡密盲注修复） | acg e2e |
| PRV-05 | 0 元商品拒绝询价/下单 | `prv05_zero_price_refused` |
| PRV-06 | 限流返回 JSON 失败 | `prv06_rate_limited` |
| PRV-07 | 用户开关、IP 白名单、管理员停用、用户禁用、重新审核作废密钥、未审核不能生成 | `prv07_credential_and_key_state`、`access.rs` 单测 |
| PRV-08 | 本地化规格值（`{"zh-CN","zh-TW","en-US"}`）是一个种类名（zh-CN），不是三段拼接（实机互通发现） | `desk.rs::prv08_*`、`prv08_localized_spec_value_is_one_race` |
| PRV-09 | 重放 `trade` 的 `stock` 仍为字符串（实机互通发现） | `acg_faka_downstream_end_to_end` |

## 9. 限制

- 不支持：预选卡（`card_id`≠0 → 拒绝）、附加规格 `[sku]` 加价（我们的规格已展开为 SKU；`sku[…]` 参数被忽略）、商品控件/人工发货、
  秒杀、优惠券、分站价、增量同步/推送（两种协议本身都没有）。
- acg-faka ≤3.1.1 的 `item` 树形响应只做了最小兼容；`GET /?s=/shared/...` 无 URL 重写形式不支持（acg-faka 自己的客户端总用路径形式）。
- mcy OpenApi 的服务端形状由客户端反推（官方插件闭源），错误 `code` 取 0（与 mcy 内核 `JSONException` 默认一致）。
- 部署：反向代理需把 `/shared/` 与 `/plugin/open-api/` 转发到后端（与 `/api/` 相同）；storefront 开发代理已配置。
- 实机互通记录（真实 acg-faka 3.7.9 两个方向、萌次元(V4.0) 类型）：`interop-report.md`。
