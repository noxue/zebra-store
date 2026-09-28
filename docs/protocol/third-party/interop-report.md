# 实机互通报告：Zebra Store ↔ 异次元发卡（acg-faka）

> 日期：2026-09-25。对端是**真实的** acg-faka **3.7.9**（docker 容器 `acg-faka`，PHP + MariaDB），不是模拟。
> 本站是从当前源码新编译的独立实例（`CARGO_TARGET_DIR=target-interop`，端口 8095，独立 SQLite），并用 `scripts/seed_demo.py` 灌入演示数据。
> 两个方向都测了：acg-faka 作为**下游**（通过「店铺共享」从本站进货，覆盖异次元与萌次元(V4.0) 两种店铺类型），以及 acg-faka 作为**上游**（本站适配器 `acg-faka` 从它进货）。
> 规格文档：`acg-faka.md`、`mcy-shop.md`、`provider-compat.md`。

## 0. 环境与凭据（测试结束后保持运行）

| 项 | 值 |
|---|---|
| acg-faka 前台 / 后台 | http://127.0.0.1:18090 · http://127.0.0.1:18090/admin，管理员 `admin@acg.test` / `Acg123456789` |
| acg-faka 会员（买家兼本站的上游账号） | `zebrabuyer` / `Buyer123456`，用户 ID `1000`，app_key `0E58B8B427FE04C8` |
| 本站后端 | http://localhost:8095（容器内访问用 http://192.168.31.183:8095），管理员 `admin` / `Admin12345` |
| 本站前台 / 后台（dev，已代理到 8095） | http://localhost:5195 · http://localhost:5196 |
| 本站下游用户 | `acgdown@zs.test` / `Down12345`，用户 ID 1，兼容 app_id `1`，app_key `GDHK5C3Y0JG6MGT03BP7U3EA5SL13GJA` |
| 本站配置 | `site_config.brand.site_url = http://192.168.31.183:8095`；`ZS__INTEGRATION__ALLOW_PRIVATE_ADDRESSES=true` |

为了让测试能跑通，对 acg-faka 做了几处**环境**调整（不涉及协议行为）：在 `acg_config` 里关掉后台、登录、注册、下单的图形验证码
（`admin_login_verification` 等设为 0，并删除 `runtime/config` 缓存）；通过 `/admin/dashboard/index?agree=1` 同意用户协议。
其余操作都走 acg-faka 自己的 HTTP 接口，用管理员或会员的 Cookie 会话调用：`/admin/api/{category,commodity,card,user,store,order}/*` 和 `/user/api/{authentication,order}/*`。

## A. acg-faka 作为下游（它从本站进货）

| # | 步骤 | 结果 |
|---|---|---|
| A1 | 本站：注册用户 → `POST /api-credential/apply` → 管理员审核 → 钱包加 1000 → `POST /api-credential/compat/issue` | ✅ `app_id=1`，32 位 app_key |
| A2 | acg 后台「店铺共享」新增，类型 **异次元(V3.1.2+)**，地址 `http://192.168.31.183:8095`（`/admin/api/store/save`） | ✅ 连接成功，缓存的店名是「斑马小铺 Zebra Store」，余额 ¥1000 |
| A3 | 拉取商品（`store/items`） | ✅ 两个分类，只列出自动发货商品（PRV-02：人工发货的 Netflix 没有出现） |
| A4 | 导入 steam-100（`store/addItem`，固定加价 2） | ✅ 零售价 100、会员价 80.40、`factory_price` 78.40（本站 98 元打 8 折后的拿货价）、库存 20 |
| A5 | 上架后，acg 会员 `zebrabuyer` 在它的前台下单，用余额支付（`/user/api/order/trade`，`pay_id=1`） | ✅ acg 调用本站 `valuation`（订单 `rent` = 78.40）和 `trade`，卡密 `STEAM-DEMO-0001-ZEBRA` 同步回到 acg 订单（已发货）；本站钱包扣 78.40，生成订单 `DJ20260925222454557769`（completed），`downstream_order_refs` 记下了 request_no `416203142850916689` |
| A6 | acg「同步」按钮（`store/syncRemote`）和商品页访问触发的同步 | ✅ 同步成功，acg 日志里没有报错 |
| A7 | 多 SKU 商品（本站「原神 月卡/纪行」，两个 SKU） | ❌ → ✅ 种类名被拼成 `空月祝福 / 空月祝福 / Welkin`（**PRV-08**，已修复）；修复并重新导入后种类名是 `空月祝福` / `纪行 1_5`，按种类下单都成功 |
| A8 | 新增一个类型为 **萌次元(V4.0)** 的店铺，指向本站 `/plugin/open-api/*`（域名写 `host.docker.internal:8095`，因为 acg 不允许同一个地址存两次） | ✅ connect、items、导入（`shared_mapping` 把种类映射到 sku_id）、同步都通过；Office（单 SKU，种类名 `DEFAULT`）和原神（按种类，一次买 2 张）下单后 `contents` 同步送达 |
| A9 | 直接按 acg 的 PHP 客户端格式发请求（表单里带明文 `app_key`）：`inventory`/`stock`/`inventoryState`/`valuation`/`query`/`draftCard`，以及用同一个 request_no 重放 `trade` | ✅ 重放返回原订单，没有重复扣款；❌ → ✅ 重放响应里的 `stock` 是 `null`（**PRV-09**，已修复为字符串） |

最终核对：本站钱包 1000 → 451.61，逐笔都对得上，包括 B 场景里本站用户自己的采购订单；acg 余额和订单数都与操作一致。

## B. acg-faka 作为上游（本站从它进货）

acg 上的准备工作都走它的后台接口：新建分类「游戏点卡ACG」；商品 `ACG 测试点卡 15元`（自动发货，无种类）、`ACG 月卡季卡`（`[category]`，月卡/季卡）、
`ACG 代充（带规格与控件）`（`[category]` × `[sku] 区服`，外加控件 `account`，正则 `^[0-9]{6,12}$`）；导入卡密；注册会员 `zebrabuyer`，由后台充值余额。

| # | 步骤 | 结果 |
|---|---|---|
| B1 | 本站后台：握手预检 `POST /admin/site-connections/handshake`（协议 `acg-faka`，app_id 1000） → 创建连接 | ✅ 握手返回余额、币种 CNY，能力只有 `categories`；创建后 ping 成功 |
| B2 | 浏览上游商品 `GET /admin/upstream-products` | ✅ 拿货价与 acg 自己的 `valuation` 一致（12.00；季卡 28.00；B服 = 20 + 2 = 22.00）；种类/规格展开成 SKU；库存按 `stock(code, race, sku[…])` 取 |
| B3 | 批量导入、在后台改价（14.00）并上架 | ✅ 导入的商品默认未上架（与原项目一致） |
| B4 | 本站前台用钱包下单（`/orders/create-and-pay`，`use_balance`）→ 采购 → acg `trade` | ✅ 季卡：`ACG-QUARTER-0001`；点卡 ×2：`ACG-CARD-0001\nACG-CARD-0002`；本站订单已交付，acg 侧订单的 `request_no` 为 `Z…`（19 位） |
| B5 | acg 调价后，在本站执行映射同步 | ✅ 售价 14.30（成本 13 × 1.1），库存跟着变 |
| B6 | acg 余额不足 | ✅ 采购单 `rejected`，错误信息「余额不足」（原文保留，按核心规则不自动重试）；给 acg 充值后由管理员手动重试 → fulfilled，没有被 request_no 去重误拦 |
| B7 | acg 的人工发货商品（DEMO） | ✅ acg 返回的是「正在发货中…」这类提示文案，没有被当成卡密（ACG-04）；采购单保持 `accepted` |
| B8 | 带控件和规格的商品：`account=12345678`，规格 小月卡/B服 | ✅ acg 订单里 `widget` 与 `sku` 正确，卡密 `DC-S-B-001` 送达 |
| B9 | 同一商品填 `account=abc` | ❌ → ✅ 修复前本站照常扣款，采购阶段才被 acg 以「账号格式不正确」拒绝（**ACG-06**）；修复后在本站结算时就被表单校验拦下，不会扣款 |

## 截图（`docs/protocol/screenshots/third-party/`）

| 文件 | 内容 |
|---|---|
| `acg-faka-shared-store-connected.png` | acg 后台「店铺共享」：本站以异次元、萌次元(V4.0) 两种类型各连一次，状态正常，显示缓存余额 |
| `acg-faka-imported-commodities.png` | acg 从本站导入的商品 |
| `acg-faka-orders.png` | acg 后台的商品订单 |
| `acg-faka-order-delivered-card.png` | acg 会员购买记录：已支付、已发货，「查看卡密」里是 `STEAM-DEMO-0001-ZEBRA` |
| `zs-admin-connection-acg-faka.png` | 本站后台「连接管理」：异次元测试站（acg-faka，全量同步，已激活） |
| `zs-admin-procurement-orders.png` | 本站采购单（fulfilled / rejected / accepted） |
| `zs-order-delivered-from-acg-faka.png` | 本站前台订单详情：已交付 `ACG-CARD-0001/0002` |
| `zs-compat-key-personal-center.png` | 本站个人中心「API 对接」：兼容 app_id / app_key |

## 发现并修复的问题

| 编号 | 位置 | 问题 | 修复 | 回归测试 |
|---|---|---|---|---|
| PRV-08 | `crates/app/src/integration/provide/desk.rs` `sku_name` | 后台和种子数据把 SKU 规格值存成一个本地化对象 `{"zh-CN","zh-TW","en-US"}`，却被当成「规格名 → 值」逐项拼接，acg 的种类名和萌次元的 SKU 名变成三种语言并列 | 与前台 `isLocalizedObject` 规则一致：键全是语言代码的对象视为一个值，取 zh-CN | `desk.rs::prv08_localized_spec_value_is_one_name`，`integration_provider_acg_faka::prv08_localized_spec_value_is_one_race` |
| PRV-09 | `crates/app/src/integration/provide/acg_faka.rs` `trade` | 重放响应的 `stock` 为 `null`，原版总是字符串 | 重放时读取当前库存；读不到时返回 `"0"` | `acg_faka_downstream_end_to_end`（重放断言 `stock == "1"`） |
| ACG-06 | `crates/infra/src/integration/acg_faka.rs` `form_schema` | 控件正则没有带入表单 schema；acg 在 `trade` 时才校验，而那时买家已经付过款 | 本站校验器能以相同语义编译的正则就写进 `regex`（萌次元适配器共用这段映射） | `acg_faka.rs::acg06_widget_regex_is_copied_when_portable`，以及 ACG-05 用例的更新 |

## 发现的对方问题（不在本站修复范围）

- **acg-faka 3.7.9 后台导入卡密时带 `sku[…]` 会把 JSON 键双重转义**（入库成 `{"\\u533a\\u670d":…}`），导致它自己的 `stock`/`draftCard`/下单拉卡按 `sku->区服` 永远查不到，有附加规格的商品库存显示为 0。
  测试时直接在库里把卡密行改回 `JSON_OBJECT('区服', …)` 才能继续。本站适配器的行为是对的：上游说 0，就记为 0。acg 订单的 `sku` 列也有同样的双重转义。
- acg 导入商品时，百分比加价 `premium_type=1` 的 `premium` 按倍数计算（填 10 是加价 1000%），只影响 acg 侧的售价，与协议无关。
- acg 不允许同一个店铺地址保存两次，所以同一站点想同时用两种协议，只能换一种主机名写法。

## 仍存在的差距（设计使然或需要其他模块配合）

1. **上游是人工发货商品时，永远不会自动完成**（ACG-04 的既定设计）。实测 acg 站长手动发货后，`query` 返回的 `secret` 已经是真实内容，但本站采购单仍停在 `accepted`，要管理员人工处理。
   原因是 acg 的提示文案（`delivery_message`）由站长自定义，无法可靠地分辨「提示」和「卡密」。可以考虑的改进：在 procurement 里保存 `trade` 当时的 `secret`，之后 `query` 到的内容与之不同，就视为已人工发货。
   这需要改 `procurement.rs` 和领域里的 procurement 类型（归另一个 agent），本次没有改。
2. 上游 `余额不足` 按核心规则（`is_retryable_error_code`，与原项目一致）不会自动重试，只能由管理员手动重试；实测手动重试有效。
3. 对方控件里只有 PCRE 能表达的正则（环视、反向引用）仍然只由对方校验，这种情况下 ACG-06 的风险依旧存在。
4. 下游方向只提供自动发货商品（`DeliveryPolicy::Synchronous`），人工发货商品不会出现在 acg 的商品列表里。这是设计决定，见 `provider-compat.md` §6。
5. 萌次元(V4.0) 协议里，单 SKU 商品的种类名显示为 SKU 编码（例如 `DEFAULT`）。功能正常，只是不好看。
6. 本站订单详情把「对接商品」的交付方式显示为「人工交付」，这是订单模块的展示问题，与协议无关。

## 验证

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 均通过；`cargo test --workspace` 全部通过（中途 `zs-infra` 的 `schema.rs` 因另一个 agent 正在新增表而短暂 74≠73，其完成后重跑 zs-infra/migration/server/shared 全绿）。
- 实机复测：修复之后，A7、A9、B9 都在真实 acg-faka 上重新跑过并通过（见上表）。
