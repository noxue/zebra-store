# 异次元发卡（acg-faka）「共享店铺」协议规格

> 目的：实现两个方向的对接
> (a) **采购方适配器**：我们的站点从 acg-faka 站点进货（`SupplierAdapter`，协议 ID 建议 `acg-faka`）；
> (b) **供货方兼容接口**：acg-faka 站点在后台「店铺共享」里把**我们的站点**当作上游添加。
>
> 依据：`git clone` 的 `lizhipay/acg-faka`（HEAD `5120942`，2026-09-21，版本 3.7.7+），
> 交叉核对 `SzeMeng76/faka-bridge`（`worker/index.js`，一个用 Cloudflare Worker 同时实现 `/shared/*` 服务端与客户端的桥）。
> 下文 `acg:` 前缀指 acg-faka 仓库路径，`fb:` 指 faka-bridge 仓库路径。本地副本：
> `scratchpad/thirdparty/{acg-faka,faka-bridge}`（会话 scratchpad，非仓库内容）。

## 0. 结论速览

| 项 | 结论 |
|---|---|
| 传输 | `POST`，`application/x-www-form-urlencoded`（PHP `$_POST`），**不接受 JSON 体** |
| 路径 | `/shared/authentication/connect`、`/shared/commodity/{items,item,inventory,inventoryState,stock,valuation,trade,query,draftCard,draft}` |
| 鉴权 | 表单字段 `app_id`（= 供货站**用户 ID**）+ `sign`；`sign = md5(ksort 后去空值的 k=v&…&key=app_key)`，无时间戳/nonce，可无限重放 |
| 信封 | 成功 `{"code":200,"msg"?:…,"data":…}`；业务失败 **HTTP 200** + `{"code":0,"msg":"…"}`（无 `data`） |
| 目录 | 一次性返回整棵「分类 → 商品」树，无分页、无增量、无删除通知 |
| SKU | 商品内「种类」（`race`，INI `[category]`，各有单价）+ 附加规格（`sku`，INI `[sku]`，加价额），不是独立 SKU 对象 |
| 价格 | `items`/`item` 的 `price`/`user_price` 是**零售/会员价**；**按调用方身份计算的拿货价**在 `factory_price` / `config[category_factory]`（3.6.5+ 的 `item` 才有，老版本只有 `inventory` 给） |
| 下单 | `trade` 同步：强制余额支付，响应 `data.secret` 即卡密（多张以 `\n` 分隔）；人工发货商品返回提示文案 |
| 查单 | `query` 按 `tradeNo` 返回 `{secret, widget, status}`，`status` 只是**支付状态**，无发货状态 |
| 幂等 | `request_no` 唯一：重复时**报错**（`The request ID already exists`），不返回原订单 |
| 回调 | **无**任何对下游的推送 |
| 凭证获取 | 在供货站注册会员即自动生成 `app_key`（16 位大写字母数字），个人中心可见、可重置；**无审核** |

## 1. 路由与传输

- 框架路由：`acg:kernel/Kernel.php:65-92`。路径 `/a/b/c` → 控制器 `App\Controller\A\B`，方法 `c`（最后一段按 `.` 切分，首段为方法名）。
  `/shared/commodity/items` → `App\Controller\Shared\Commodity::items()`。
  没有 URL 重写的站点也可用 `/?s=/shared/commodity/items`（`$_GET['s']`，`Kernel.php:53`）；acg-faka 自己的客户端总是用路径形式（`acg:app/Service/Bind/Shared.php:159,173,261,352…`）。
- 所有共享控制器都挂了拦截器 `[Waf, SharedValidation]`（`acg:app/Controller/Shared/Commodity.php:26`、`Authentication.php:14`）。
- 请求读取 `$_POST`（`acg:kernel/Context/Request.php:12`），因此只认 `application/x-www-form-urlencoded`（或 multipart）。嵌套参数使用 PHP 方括号语法：`sku[区服]=亚服`。
  注意 PHP 会把顶层键名中的 `.` 和空格改成 `_`。
- 方法：框架不限制 HTTP 方法，但鉴权读的是 POST 体，实际只能 `POST`。
- 响应：控制器返回数组时 `Content-Type: application/json;charset=utf-8`，`json_encode(JSON_UNESCAPED_UNICODE|JSON_UNESCAPED_SLASHES)`（`Kernel.php:167-170`）。
- 异常：`JSONException` / `ParameterMissException` → HTTP 200 + `{"code": <异常 code，默认 0>, "msg": "…"}`（`Kernel.php:190-196`）。控制器/方法不存在 → 404 HTML 页（`Kernel.php:137-145,184`）；其它未捕获异常 → HTML 500 页。
- 成功信封：`acg:app/Controller/Base/API/Shared.php:22-28`：`code` 恒为调用值（200），**`msg` 只有在传了消息时才出现**（`items`/`item`/`stock`/`valuation`/`draftCard`/`draft` 没有 `msg` 键），`data` 总是存在。

## 2. 鉴权与签名

### 2.1 服务端校验（`acg:app/Interceptor/SharedValidation.php:29-49`）

1. `app_id = $_POST['app_id']`，必须是标量；`User::find(app_id)`（**用户表主键**），不存在 → `{"code":0,"msg":"商户ID不存在"}`。
2. `expected = Str::generateSignature($_POST, $user->app_key)`（对**收到的全部 POST 字段**签名）。
3. `sign` 必须是字符串且 `hash_equals(expected, sign)`，否则 `{"code":0,"msg":"密钥错误"}`。
4. 通过后把用户放进上下文（`Context::set(Shared::SESSION, $user)`），之后的会员等级、拿货价都按该用户算。
- 不校验用户状态（封禁用户仍能过鉴权；只有 `trade` 的余额扣款路径会检查 `status`，`Order.php:903-905`）。
- 没有时间戳、nonce、IP 白名单：**签名可被无限重放**。
- 旧版（≤1.3.9）是 `$_POST['sign'] != $signature` 松散比较（`git show 09911a2:app/Interceptor/SharedValidation.php`），算法相同。

### 2.2 签名算法（`acg:app/Util/Str.php:103-113`，自 2022 年起未变）

```php
unset($data['sign']);
ksort($data);                                   // 顶层键排序（非数字键即按字节序）
foreach ($data as $k => $v) if ($v === '') unset($data[$k]);   // 仅删除「顶层」空字符串
return md5(urldecode(http_build_query($data) . "&key=" . $appKey));
```

精确语义（实现时照此）：

- 参与签名的是**除 `sign` 外的全部字段**，包括客户端顺带发送的 `app_key`（acg-faka 自己的客户端会发，见 §2.3）。
- 只删除**顶层**值恰为 `''` 的字段；`"0"`、嵌套数组里的空串都保留。
- `http_build_query` 对嵌套数组展开为 `sku[k]=v`，**内层保持原插入顺序（不排序）**；`null` 跳过；空数组不产生任何片段；`true/false` → `1/0`。
- 整串 `urldecode`，所以签名串是**未编码的原文** `k=v`（`+`、`%` 等原样；值中含 `&`/`=` 时不做转义）。
- 摘要：`md5`，**小写 hex**，UTF-8 字节。
- 等价的服务端实现（我们做兼容接口时用）：把原始表单体按出现顺序解析为 `(name, value)` 对 → 取每对的顶层键（`name` 中第一个 `[` 之前的部分，并按 PHP 规则把 `.`/空格换成 `_`）→ 丢弃「无方括号且值为空串」的对 → 按顶层键**稳定排序**（字节序）→ 以 `name=value` 用 `&` 连接 → 追加 `&key=<app_key>` → md5。
  同名扁平键重复时 PHP 取最后一个。数字形式的顶层键（如 `123`）在 PHP 中是整数键，与字符串键混排时 PHP 8 会把整数转字符串比较——结果与字节序一致，可忽略。

### 2.3 客户端签名（acg-faka 作为采购方，`acg:app/Service/Bind/Shared.php:72-106`）

```php
$data = array_merge($data, ["app_id" => $appId, "app_key" => $appKey]);   // 注意：明文发送 app_key！
$data['sign'] = Str::generateSignature($data, $appKey);
Http::make()->post($url, ['form_params' => $data, 'timeout' => 30, 'allow_redirects' => false]);
```

- acg-faka 客户端**把 `app_key` 明文放进表单**（且参与签名）。faka-bridge 只发 `app_id`+`sign`（`fb:worker/index.js:211`），两者都能通过服务端校验，因为服务端对收到的字段现算。
  我们的客户端**不要**发送 `app_key`（泄露密钥）。
- 结果判定：`code != 200` 即失败，错误文本取 `msg`（会被脱敏，`SharedPayload::upstreamError`）。HTTP 404/405 仅在「可选端点」上被当作「老版本上游」（§6）。

### 2.4 测试向量（Python 复刻 PHP，脚本 `scratchpad/thirdparty/vec/sign.py`；并用 faka-bridge 的 `acgSign` 独立复算一致）

`app_key = 8F3A2C9D1E7B6A54`，`app_id = 1024`：

| # | 表单字段（发送顺序） | 签名原串 | `sign` |
|---|---|---|---|
| V1 | `app_id=1024, app_key=8F3A…54`（acg-faka 客户端的 connect） | `app_id=1024&app_key=8F3A2C9D1E7B6A54&key=8F3A2C9D1E7B6A54` | `07bf581b48cc04485a688ec49f481368` |
| V2 | `app_id=1024`（只发 app_id 的客户端） | `app_id=1024&key=8F3A2C9D1E7B6A54` | `1095131eb01a23659cff325ba248290b` |
| V3 | trade：`shared_code=ABCDEF1234567890, contact=buyer@example.com, num=2, card_id=0, device=0, password=, race=月卡, request_no=123456789012345678, sku[区服]=亚服, sku[版本]=标准, app_id=1024, app_key=8F3A…54` | `app_id=1024&app_key=8F3A2C9D1E7B6A54&card_id=0&contact=buyer@example.com&device=0&num=2&race=月卡&request_no=123456789012345678&shared_code=ABCDEF1234567890&sku[区服]=亚服&sku[版本]=标准&key=8F3A2C9D1E7B6A54` | `f7f8a9ce8e0ed44908af0afacb33c593` |

V3 的实际请求体（`password=` 空串被发送但不参与签名）：

```
shared_code=ABCDEF1234567890&contact=buyer%40example.com&num=2&card_id=0&device=0&password=&race=%E6%9C%88%E5%8D%A1&request_no=123456789012345678&sku%5B%E5%8C%BA%E6%9C%8D%5D=%E4%BA%9A%E6%9C%8D&sku%5B%E7%89%88%E6%9C%AC%5D=%E6%A0%87%E5%87%86&app_id=1024&app_key=8F3A2C9D1E7B6A54&sign=f7f8a9ce8e0ed44908af0afacb33c593
```

## 3. 接口清单（供货方，当前版本）

以下所有请求都另带 `app_id`、`sign`（以及可选 `app_key`）。「失败」一律指 HTTP 200 + `{"code":0,"msg":…}`。

### 3.1 `POST /shared/authentication/connect` — 握手/余额

`acg:app/Controller/Shared/Authentication.php:20-24`

- 请求：无业务字段。
- 响应：`{"code":200,"msg":"success","data":{"shopName":"<站点名 Config shop_name>","balance":<调用者余额，数字>}}`
- 采购方校验（`acg:app/Controller/Admin/Api/Store.php:173-189`）：`shopName` 去标签后 1–128 字、无控制字符；`balance` 数字、0 ≤ b ≤ 999999999999.99。

### 3.2 `POST /shared/commodity/items` — 全量目录树

`acg:app/Controller/Shared/Commodity.php:45-94,168-171`

- 请求：无业务字段。
- 选取：`category.status=1` 的分类，其 `commodity` 中 `api_status=1`（开放对接）且 `status=1`；`hide=1` 的商品仅当调用者会员等级的 `level_price` 配置 `show=1` 时出现；空分类被跳过。
- 响应 `data`：

```json
[{"id":3,"name":"游戏","sort":0,"icon":"…","status":1,"pid":0,
  "children":[{ 商品行 }, …]}]
```

  分类字段白名单 `id,name,sort,icon,status,pid`（`acg:app/Util/SharedPayload.php:60`）。
  商品行字段白名单（`SharedPayload.php:49-57`）：
  `id, category_id, name, description(HTML), cover, price, user_price, status, code, sort, delivery_way, contact_type, password_status, coupon, seckill_status, seckill_start_time, seckill_end_time, draft_status, draft_premium, inventory_hidden, only_user, purchase_count, widget(JSON 字符串), minimum, maximum, config(INI 字符串), stock, tags`。
  - `price`/`user_price`/`draft_premium` 是 JSON **数字**（Eloquent `float` cast，`acg:app/Model/Commodity.php:78-111`），不是两位小数字符串。
  - **`items` 不含 `factory_price`**（当前版本已把它列入禁止出站，`SharedPayload.php:66-73`）；≤3.1.1 的老版本 `items` 直接整行 `toArray()`，并按调用者现算 `factory_price`/`category_factory`（`git show 09911a2:app/Controller/Shared/Commodity.php`）。
  - `stock`：自动发货（`delivery_way=0`）且非转售商品 = 未售卡密数；转售商品 = 缓存快照（可能陈旧，取较大值，`Commodity.php:97-116`）；人工发货 = 商品 `stock` 列。**不按种类细分**。
  - `config` 里的成本段 `category_cost/sku_cost/shared_mapping/*_factory` 被裁掉（`SharedPayload.php:95-98`）。
  - 描述/封面里指向「本站上游」的地址被替换为 `/favicon.ico` 或 `***`，占位图再被改成空封面（`SharedPayload.php:114-173`）。
- 采购方导入列表时的校验（`Store.php:252-313`）：最多 200 个分类；商品 `id` 为 1..4294967295 的正整数；`code` 1–64 个非空白字符；`name` 去标签后 ≤255 字；id/code 不可重复。

### 3.3 `POST /shared/commodity/item` — 单商品详情

`acg:app/Controller/Shared/Commodity.php:177-230`，底层 `acg:app/Service/Bind/Shop.php:124-236`

- 请求：`code=<商品对接码>`（3.1.1 及以前字段名是 `sharedCode`，acg-faka 客户端两个都发，`Bind/Shared.php:352-356`）。
- 校验：商品存在且 `api_status=1`（否则 `该商品未开放对接`），`status=1`（否则 `该商品暂未上架`）。
- 若本站商品本身也是转售且开启了 `shared_sync`，会**同步向它的上游发请求**再返回（套娃）。
- 响应 `data`：`Shop::getItem` 的详情（剔除 `shared_*`、`factory_price`、`level_price`，`SharedPayload.php:82-87`）：
  `id, name, description, only_user, purchase_count, category_id, cover, price, user_price(会员价，为 0 时已回退为零售价), status, owner{id,username,avatar}|null, delivery_way, contact_type, password_status, coupon, seckill_*, draft_status, draft_premium, inventory_hidden, widget(已解码的数组), minimum, maximum, config(解析后的对象，非 INI 串), stock, code, tags(数组), order_sold, service_url, service_qq, share_url, login(false), trade_captcha`，
  **外加**（3.6.5+，#842）`factory_price` = 按调用者身份算的 1 件拿货价（非种类商品）；种类商品为 `0`，拿货价在 `config.category_factory{种类:单价}`。
- 注意：`config` 是按 `group=null` 解析的，即**零售口径**的种类价/批发价；调用者的会员价只体现在 `factory_price`/`category_factory`。
- 老版本（≤3.1.1）的 `item` 返回的是**只含该商品的分类树**（与 `items` 同形），客户端需按形状识别（`Bind/Shared.php:278-310`）。

### 3.4 `POST /shared/commodity/inventory` — 库存 + 拿货价（所有版本都有）

`acg:app/Controller/Shared/Commodity.php:294-388`

- 请求：`sharedCode=<code>`（注意驼峰，**与 item 的 `code` 不同**），`race=<种类名>`（可空）。
- 响应：

```json
{"code":200,"msg":"success","data":{
  "count": 12,              // 自动发货：该种类（未传则第一个种类）的未售卡密数；人工发货恒为 0
  "delivery_way": 0, "draft_status": 0,
  "price": 10, "user_price": 9,
  "config": "[category]\n月卡=10\n…\n[category_factory]\n月卡=8.5",   // INI 字符串，category_factory 为调用者拿货价
  "factory_price": 8.5,     // 非种类商品的调用者拿货价，种类商品为 0
  "is_category": false}}
```

### 3.5 `POST /shared/commodity/stock` — 实时库存（3.1.2+）

`Commodity.php:488-495`，`Shop.php:263-288`

- 请求：`code`，`race`（可选），`sku[名]=值`（可选）。
- 响应：`{"code":200,"data":{"stock":"12"}}`（**字符串**）。自动发货 = 满足 race/sku 的未售卡密数；人工发货 = 商品 `stock` 列；转售商品 = 缓存的上游读数。没有「无限」语义（faka-bridge 把 `-1` 当无限，`fb:worker/index.js:368`，但 acg-faka 本身不会产生 `-1`）。

### 3.6 `POST /shared/commodity/valuation` — 询价（3.1.2+）

`Commodity.php:501-516`，`acg:app/Service/Bind/Order.php:146-326`

- 请求：`code, num, race, sku[…], card_id`。
- 响应：`{"code":200,"data":{"price":"17.00","currency_code":"CNY"}}` — `price` 为**总价**（已含数量、批发阶梯、种类价、SKU 加价、预选加价、会员等级折扣；再经分站加价 `getSubstationPrice`）。`currency_code` 自 3.7.x 起有。
- 不锁价、不保留库存。

### 3.7 `POST /shared/commodity/inventoryState` — 可售预检

`Commodity.php:236-288`

- 请求：`shared_code, card_id, num, race`。
- 响应：`{"code":200,"msg":"success","data":[]}` 或失败（`库存不足`/`当前商品已停售`/`该卡已被他人抢走啦`）。仅做卡密数量检查，人工发货商品永远通过。

### 3.8 `POST /shared/commodity/trade` — 下单（同步交付）

`Commodity.php:395-403`，`acg:app/Service/Bind/Order.php:554-972`，`orderSuccess` `Order.php:1058-1125`

- 请求字段：
  | 字段 | 说明 |
  |---|---|
  | `shared_code` | 商品对接码（必填） |
  | `num` | 数量 ≥1，受 `minimum`/`maximum` 约束 |
  | `race` | 种类名；商品配置了 `[category]` 时必填且必须存在 |
  | `sku[名]=值` | 商品配置了 `[sku]` 时每个规格必选 |
  | `card_id` | 预选卡 ID，`0` 表示不预选；预选时数量强制为 1 |
  | `contact` | 联系方式；**调用者已登录（总是）时会被替换为随机串**（`Order.php:813-815`），可随便填 |
  | `password` | 查单密码，可空 |
  | `device` | 设备类型 0–3，填 0 |
  | `request_no` | 调用方单号（acg-faka 客户端传自己订单的 18 位 `trade_no`）。已存在则报错 `The request ID already exists`（`Order.php:817-819`），**不是**返回原订单 |
  | `coupon` | 优惠券码（可选） |
  | `<widget.name>` | 商品自定义输入控件的值，按控件 `name` 直接作为顶层字段（有 `regex` 时校验，失败返回控件的 `error` 文案，`Order.php:606-624`） |
- 行为：强制 `pay_id=1`（安装时的「余额」支付，`#system`，`kernel/Install/Install.sql:455`；站长若对商品关闭该支付方式则失败 `当前支付方式已停用…`）。
  下单前实时查库存/询价；在 serializable 事务中扣调用者余额（`Bill::create(... TYPE_SUB)`，`Order.php:896-913`），然后 `orderSuccess` 立即拉卡。余额不足由 `Bill::create` 抛 `余额不足`（`acg:app/Model/Bill.php:75-77`）。
- 成功响应：

```json
{"code":200,"msg":"success","data":{
  "url":"https://supplier/user/personal/purchaseRecord?tradeNo=…",
  "amount":"17.00",                       // 实扣金额（字符串）
  "tradeNo":"583920174628301745",         // 供货方订单号（18 位数字）
  "secret":"卡密1\n卡密2",                 // 交付内容，多张以 PHP_EOL 分隔
  "leave_message":"…"|null,
  "stock":"10"}}                           // 下单后剩余库存（字符串）
```

  - 自动发货商品：`secret` 是卡密；若拉卡时不足（并发被抢）则为文案 `很抱歉，有人在你付款之前抢走了商品，请联系客服。`（钱已扣，订单仍为已支付，`Order.php:1129,1164-1176`）。
  - 人工发货商品：`secret` 为商品 `delivery_message` 或 `正在发货中，请耐心等待，如有疑问，请联系客服。`；站长之后人工发货会改写 `order.secret`（`acg:app/Controller/Admin/Api/Order.php:409-410`）。
  - 风控插件挂起发货时：`secret` = `订单正在人工审核中，通过后会立即发货，请耐心等待。`（`Order.php:1069-1078`）。
  - **无法从响应区分「真卡密」与「提示文案」**，只能靠文本启发式（faka-bridge 用正则 `待处理|等待发货|稍后查询|pending|processing` 判断，`fb:worker/index.js:398`）。
- 转售（套娃）商品：`orderSuccess` 会同步调用它自己的上游 `trade`，把上游返回的 `secret` 原样写入。

### 3.9 `POST /shared/commodity/query` — 查单

`Commodity.php:465-482`

- 请求：`tradeNo=<供货方订单号>`（框架按参数名从 `$_REQUEST` 注入，`kernel/Annotation/Collector.php:93-105`；放在 POST 体里即可，签名会覆盖它）。
- 只能查**调用者自己**（`owner=app_id`）的订单，否则 `订单不存在`。
- 响应：`{"code":200,"msg":"success","data":{"secret":"…","widget":{"<name>":{"value":"…","cn":"标题"}}|null,"status":1}}`
  `status` 是 `order.status`：`0` 未支付、`1` 已支付。**没有 `delivery_status`**，人工发货前后 `status` 都是 1，只能比较 `secret` 文本是否变化。
- 不能按 `request_no` 查。

### 3.10 预选卡：`draftCard` / `draft`

- `POST /shared/commodity/draftCard`：`code, page, limit, race, sku[…], search-draft=…`（老版本字段 `sharedCode`）。返回 `{"list":[{"id","draft","draft_premium"}],"total"}`（≤3.1.1 返回 Laravel 分页器 `{current_page,data,total,…}`）。`Commodity.php:410-457`
- `POST /shared/commodity/draft`：`code, card_id` → `{"draft_premium":…}` 等。`Commodity.php:522-533`
- 仅 `draft_status=1` 的商品有意义；我们作为供货方可恒定 `draft_status=0`，这两个接口返回空即可（faka-bridge 即如此，`fb:worker/index.js:474-475`）。

## 4. 商品数据结构（INI `config` 与控件）

- `config` 为 acg-faka 自定义 INI（`acg:app/Util/Ini.php:58-121`）：`[节]` 下每行 `a.b.c=值`，无引号、`=` 两侧不 trim、一行只能有一个 `=`。
  ```ini
  [category]            ; 种类 → 单价（绝对价）。存在时下单必须选 race
  月卡=10.00
  季卡=28.00
  [category_wholesale]  ; 种类批发：种类.起购数量=单价
  月卡.10=9.00
  [wholesale]           ; 无种类商品的批发：起购数量=单价
  10=9.00
  [sku]                 ; 附加规格：规格名.选项=加价额（0 表示不加价）
  区服.亚服=0
  区服.美服=1.50
  [category_factory]    ; 仅出现在 inventory/item 响应中：调用者对各种类的拿货单价
  月卡=8.50
  ```
- 计价顺序（`Order.php:146-326` `valuation`）：基础价（零售或会员价）→ 会员等级自定义价 → 种类价覆盖 → 批发阶梯（`krsort` 后第一个 `num >= 阈值`）→ SKU 加价累加 → 预选加价 → 商品组折扣 → 优惠券 → ×数量。
- `widget`：JSON 数组 `[{"cn":"标题","name":"account","placeholder":"","type":"text|password|number|select|checkbox|radio|textarea","regex":"","error":"","dict":"显示=值,显示2=值2"}]`；
  采购方导入校验：`name` 匹配 `^[A-Za-z][A-Za-z0-9_]{0,31}$`、最多 32 个、`dict` 最多 100 项（`Store.php:523-603`）。
- `contact_type`：0 任意、1 手机、2 邮箱、3 QQ；`password_status`：是否要求查单密码；`delivery_way`：0 自动（卡密）、1 人工。

## 5. 采购方行为（acg-faka 作为下游如何对接上游）

- 后台「店铺共享」（`acg:app/Controller/Admin/Api/Store.php`）录入：`type`（0 = 异次元 3.1.2+，2 = 异次元 ≤3.1.1 / SharedStock 插件，1 = 萌次元 V4 open-api，见 `assets/admin/js/_admin.js` 的 `_shared_type`）、
  `domain`（http/https 完整地址，可带路径前缀，≤128 字符，`Store.php:75-131`）、`app_id`（`^[A-Za-z0-9._:@-]{1,32}$`，`Store.php:137-148`）、`app_key`（1–64 个非空白字符，`Store.php:154-165`）、`currency`、`currency_rate`。
  保存后「连接」调 `connect` 缓存站名与余额（`Store.php:989-1011`）。
- 导入：拉 `items` 树让站长勾选（`Store.php:1018-1041`），对每个选中 code 调 `item`，建本地商品：`delivery_way=1`、`api_status=0`、新随机 `code`、`shared_id/shared_code`、按固定/百分比/模板加价（`Store.php:1081-1330`）。**没有自动定时同步**（官方 AutoDock 是应用商店付费插件）。
- 同步：①商品详情页每次访问且 `shared_sync=1` 时同步调 `item`（`Shop.php:156-171`）；②后台「同步」按钮逐个 `syncRemoteItem`（`Store.php:1336-1366`）。
- 买家下单：`getItemStock`（`/stock`，缓存于 `shared_stock`，成交后失效）→ `getValuation`（`/valuation`，失败回退为 0/本地成本）→ 付款成功后 `orderSuccess` 调上游 `/trade`，把返回的 `secret` **直接作为本地订单的交付内容并置 `delivery_status=1`**（`Order.php:1082-1086`，`Bind/Shared.php:399-442`）。
  **它从不保存上游 `tradeNo`，也从不调用 `/query`**：上游若返回的是「待发货」文案，下游买家永远只看到这段文案。
- 协议代次探测：对 `stock/draft/valuation` 用 `postOptional`，HTTP 404/405 视为 ≤3.1.1，并记到 `shared.protocol` 列（`Bind/Shared.php:60-151`）。
- 跨币种：`SharedCurrency`（`acg:app/Util/SharedCurrency.php`）按 `currency_rate` 或站点汇率把上游金额换算为本站货币；协议本身不换算。

## 6. 版本差异与历史修复（兼容时要当心）

提交信息只有版本号，下列结论来自代码注释与 `git show <ver>:<file>` 对比：

| 版本 | 变化 | 影响 |
|---|---|---|
| ≤3.1.1（如 `09911a2` 1.3.9） | `item` 收 `sharedCode` 返回**分类树**；无 `stock/valuation/draft`；`items` 整行出站（含 `factory_price` 按调用者现算、`shared_code`、`level_price`…）；`draftCard` 用强类型参数 `sharedCode,page,limit,race` | 客户端需兼容两种 `item` 形状；拿货价走 `inventory` |
| 3.1.2 | `item` 改收 `code` 返回单商品；新增 `stock/valuation/draft` | |
| 3.6.5（#842） | `item` 补回按调用者现算的 `factory_price` | 更早版本的 `item` 无此字段 → 用 `inventory` |
| 3.7.x | `api_status` 开放对接闸门补到所有按 code 寻址的接口（之前任何人可对未开放商品下单）；`items` 改白名单出站；`draftCard` 只允许按 `draft` 过滤（修复 `search-secret` 盲注卡密）；`SharedValidation` 改 `hash_equals` + 标量检查（修复 `0e…` magic hash 与数组 `app_id` 500）；上游图片/域名脱敏、占位图改空封面（3.7.2 回归修复）；`valuation` 增加 `currency_code` | 我们的客户端不能依赖 `items` 里的 `factory_price`；图片可能为空 |
| 3.5.9 | 控件 JSON 以 URL 编码形态入库的回归（`%5B%7B…`），3.6.0 修复并在读取时透明还原（`Commodity.php` `getWidgetAttribute`） | 解析 `widget` 时对 `%5B` 开头的串做 `urldecode` |
| 3.x | 会员价 `user_price=0` 表示「按零售价」，旧版加价会把 0 变成加价额导致亏本（`Bind/Shared.php:753-762`） | 我们映射价格时把 `user_price<=0` 视为未设置 |
| 3.x | 0 元单拦截：有价值商品算出 ≤0 元一律拒单（`Order.php:875-889`） | |

## 7. 映射到我们的模型

### 7.1 采购方适配器（我们 → acg-faka 上游）

**配置字段**（`ConfigField`）：`base_url`（url，必填）、`app_id`（text，必填，供货站用户 ID）、`app_key`（secret，必填）。可选 `extra.protocol_generation`（自动探测，同 acg-faka 的 `protocol` 列）。

**能力**：

| 能力 ID | 支持 | 说明 |
|---|---|---|
| `categories` | ✅ | 由 `items` 树的顶层分类得出（`id,name,pid,sort,icon`） |
| `incremental_changes` | ❌ | 无变更流；只能全量拉 `items` 做差异 |
| `push_events` | ❌ | 无任何回调 |
| `quote` | ⚠️ 可伪装 | `valuation` 能给出按调用者身份的总价，但**不锁价**；若实现 `quote()`，报价 ID 只能本地保存「预期总价」，下单后用响应 `amount` 核对，超出即告警（faka-bridge 的「价保」做法） |
| `multi_item` | ❌ | 一单一个商品/种类 |
| `idempotency` | ⚠️ 部分 | `request_no` 唯一可防**重复扣款**，但重复请求返回错误而非原订单，且不能按 `request_no` 查单 → 请求超时后无法确认是否成交（需「待人工核对」状态） |
| `encrypted_delivery` | ❌ | 卡密明文在响应体中；且客户端惯例明文传 `app_key`（我们不传） |

**UpstreamClient 各操作**：

- `handshake`/`ping` → `connect`：站点名 `shopName`、余额 `balance`（数字 → `Amount`）。货币：协议不给，3.7.x 可从一次 `valuation` 的 `currency_code` 得知；否则由管理员填写（默认 CNY）。
- `list_categories` → `items` 顶层。
- `list_products` → 一次 `items` 拿全量后**在本地分页**（`total` = 商品数）；`get_product(id)`：我们的核心按数字 id 取单品，而 acg 按 `code` 寻址——适配器须维护 `id → code` 映射（从 `items` 获得，缓存于内存或 `extra`），再调 `item(code)`；拿货价用 `item.factory_price` / `config.category_factory`，缺失时（<3.6.5 / ≤3.1.1）调 `inventory(sharedCode, race)`。
- **商品/SKU 映射**：acg 商品 → 我们的商品；
  - 有 `[category]`：每个种类 → 一个 SKU（`sku_code` = 种类名，`spec_values = {"种类": 名}`）；单价 = `category_factory[名]`（拿货价），库存 = `stock(code, race=名)`；
  - 无种类：一个默认 SKU，价格 = `factory_price`；
  - `[sku]` 附加规格（加价额）：每种组合都是不同价格且下单要传 `sku[名]=值`。可展开为笛卡尔积 SKU（数量爆炸时截断/放弃），或**不支持**并在导入时标记「需人工」；推荐：组合数 ≤ 50 时展开，价格 = 种类拿货价 + 各规格加价（加价在拿货口径上是否打会员折扣未知，最终以 `valuation` 为准）。
  - 批发阶梯（`wholesale` / `category_wholesale`）是零售口径，不能直接当拿货价；下单前用 `valuation(num)` 取真实总价。
  - `widget` → 我们的 `manual_form_schema`（`name/cn/type/regex/dict`），下单时把 `manual_form_data` 以顶层字段发送。
    对方在 `trade` 里才用 `preg_match("/{regex}/")` 校验（此时本站买家已付款），所以本站校验器能同义编译的 `regex` 必须带入 schema、在结算时校验（ACG-06，实机互通发现）。
  - `minimum/maximum` → 购买数量限制；`contact_type/password_status` 可忽略（登录态下单不校验 contact）。
  - `delivery_way=0` → `fulfillment_type=auto`；`1` → `manual`（交付不可靠，见下）。
  - `stock`：`stock` 端点字符串转整数；人工发货商品的库存是站长填写的数字。
  - 删除/下架：`items` 中消失或 `item` 报 `商品不存在`/`该商品未开放对接`/`该商品暂未上架` → 视为下架（连续多次再下架，避免瞬时错误）。
- `place_order` → 可选先 `valuation` 预检总价 → `trade(shared_code, num, race, sku, card_id=0, device=0, contact=<随机>, password='', request_no=<我们的单号，建议 ≤ 64 字符的纯字母数字>, <widget 字段>)`。
  成功：供货方单号 `tradeNo`，实扣 `amount`，交付 `secret` 按 `\n` 切分。
  业务错误码映射（按 `msg` 文本，均 `code=0`）：`库存不足`→out_of_stock（可重试）、`当前商品已停售`/`该商品未开放对接`/`商品不存在`→item_unavailable（不可重试）、`余额不足`→insufficient_balance（可重试）、`The request ID already exists`→重复提交（需核对）、其它 → 不可重试。网络/HTTP 5xx/超时 → **结果未知**，不得自动重下（同 faka-bridge 的「待人工核对」）。
- `get_order(OrderRef::No(tradeNo))` → `query(tradeNo)`：`status=1` 且 `secret` 非提示文案 → 已交付；否则处理中。`addressable` 只接受单号（不接受数字 id）。
- `cancel_order` → 不支持（下单即扣款，无取消接口）。
- `download` → 普通 GET（图片），注意 3.7.2+ 可能为空封面。
- 入站：无（`inbound_path` 为空）。

**我们功能中无法通过该协议实现的**：增量/推送同步、锁价报价、多商品订单、真正幂等（可重放查询）、加密交付、订单取消、按下游单号查单、区分「已交付」与「待人工发货」、跨币种声明（老版本）、SKU 级库存以外的库存状态（`low_stock` 等需本地推断）。

### 7.2 供货方兼容接口（acg-faka 站点 → 我们）

在我们的服务端实现 `POST /shared/authentication/connect` 与 `POST /shared/commodity/*`（路由挂在站点根路径，与 SPA 路由不冲突；需要 `/shared/*` 反代到后端）。

- **凭证**：acg-faka 管理员填写的 `app_id` 可以是 `^[A-Za-z0-9._:@-]{1,32}$` 任意串 → 直接使用我们 `api_credentials` 的 API Key（若长度/字符集合规）或凭证数字 id；`app_key` = 该凭证的 secret（≤64 个非空白字符）。
  沿用我们现有规则：凭证已审核 + 启用 + 用户正常；密钥轮换期内新旧 secret 都可验签。
  签名按 §2.2「稳定排序」法对原始表单体现算，**常量时间比较**；`app_key` 字段若出现只参与签名、不作鉴权依据。
  因协议无时间戳/nonce，只能靠限流（每 key 每分钟 N 次）+ HTTPS 降低重放风险；建议兼容接口默认关闭、由站长在设置中开启。
- **信封**：一律 HTTP 200，成功 `{"code":200,"msg":"success","data":…}`，失败 `{"code":0,"msg":"<中文文案>"}`（acg-faka 客户端只看 `code != 200`，并对 `msg` `strip_tags`）。
  必须返回 JSON 而不是 HTML/302：acg-faka 客户端 `allow_redirects=false`。
- **文案**：acg-faka 客户端不解析错误文案，但站长会看到，用中文；库存不足请用 `库存不足`（acg-faka 内部同样文案）。
- **目录映射**（我们 → acg）：
  - 分类：`{id, name(zh-CN), sort, icon, status:1, pid:0, children:[…]}`，只输出有可对接商品的分类，≤200 个。
  - 每个我们的**商品** → 一个 acg 商品：`id` = 商品 id（正整数），`code` = 商品 id 的字符串（或 slug，≤64 非空白字符，**不可含空格**）；
    多 SKU → `[category]` 种类（种类名 = SKU 显示名，需保证在商品内唯一且不含 `=`、换行、`.`（`.` 会被 INI 当层级）、`[`/`]`）；单 SKU 且无规格名 → 不输出 `[category]`。
  - `price`/`user_price` 输出**数字**（JSON number），给零售价与调用者的会员价；`factory_price`（`item` 中）与 `config` 的 `[category_factory]` 给**调用者实际拿货价**（我们的定价引擎按该 API 用户计算）。老版 acg-faka 从 `inventory` 取拿货价，所以 `inventory` 也必须给。
  - `stock`：整数；我们的 `-1`（无限）需映射为一个大数（如 `999999`，faka-bridge 同此做法），因为 acg-faka 用 `num > stock` 判断。
  - `delivery_way`：自动发货 0，人工 1；`draft_status` 恒 0；`seckill_status` 0；`minimum/maximum` 映射购买限制；`only_user` 0；`inventory_hidden` 0。
  - `widget`：我们的 `manual_form_schema` → acg 控件数组（`name` 必须 `^[A-Za-z][A-Za-z0-9_]{0,31}$`，不合规的字段无法对接 → 该商品不输出或拒绝）；`items` 中为 JSON 字符串，`item` 中为数组。
  - `description`：HTML；`cover`：绝对 URL（acg-faka 会可选本地化下载，地址必须可公网访问；为空时它用自己的 favicon）。
  - `config` 在 `items` 中是 INI 字符串，在 `item` 中是对象（`{"category":{"月卡":"10.00"},"category_factory":{…}}`）；acg-faka 对两种都能处理（`Store.php:763-771`、`Bind/Shared.php:371-373`）。
- **`stock`/`inventory`/`inventoryState`/`valuation`**：按 `code` + `race`（=SKU 名）定位 SKU；`valuation` 用与商城下单相同的定价引擎按调用用户计算 `num` 件总价，返回 `{"price":"17.00","currency_code":"<站点结算货币>"}`。
- **`trade`**：
  1. 以 `request_no` 为幂等键（acg-faka 传其本地 18 位订单号）：同一凭证下已存在 → acg 原语义是报错；**更安全的做法**是返回原订单同形响应（faka-bridge 即如此，`fb:worker/index.js:425`），因为 acg-faka 客户端超时后不会重试，返回原订单不会造成重复发货。
  2. 走我们的 API 下单流程（钱包扣款、`downstream_order_no = request_no`、`manual_form_data` = 未知顶层字段中与控件 `name` 匹配的那些）。
  3. **必须同步交付**：acg-faka 把响应 `secret` 直接当作买家的最终交付并置已发货，**之后永不查询**。所以：自动发货商品需在请求内完成发货再返回卡密（`\n` 连接）；人工发货/异步履约的商品只能返回提示文案，之后的真实交付**无法送达** acg-faka 的买家 → 建议兼容接口**只输出自动发货商品**，或在站点设置中明确告知。
  4. 响应：`{"code":200,"msg":"success","data":{"url":"<我们订单详情链接>","amount":"17.00","tradeNo":"<我们的订单号>","secret":"…","leave_message":null,"stock":"<剩余库存字符串>"}}`。
- **`query`**：`tradeNo` → 我们的订单号，仅限该凭证用户的订单；`status`：已支付 1，其它 0；`secret`：已交付内容，未交付时给提示文案；`widget`：`manual_form_data` 映射 `{name:{value,cn}}` 或 `null`。
- **`draftCard`/`draft`**：返回 `{"list":[],"total":0}` / `{"draft_premium":0}`。
- **`connect`**：`{"shopName":"<站点名>","balance":<该用户钱包余额，数字>}`。

**棘手点**：

1. 签名覆盖原始表单、嵌套 `sku[…]`、PHP 键名改写（`.`→`_`）——必须按原始字节解析，不能先解析成 map 再重建。
2. 无防重放；密钥明文可能随 acg-faka 客户端请求体传输（其客户端行为，无法改变），需强制 HTTPS 并提示站长。
3. 同步交付约束（上一节第 3 点）与我们的异步履约模型冲突。
4. 老版 acg-faka（≤3.1.1）会调用 `item` 时传 `sharedCode` 并期望树形响应——我们按新版返回单商品，老版客户端会在导入时报错（`远端商品字段 name 内容不正确` 的反面）；可按「只收到 `sharedCode` 没收到 `code`」识别老客户端并返回树形。
5. acg-faka 用浮点数表示金额；我们输出数字时保持两位小数（`17.00` 作为 JSON 数字会变成 `17`，无妨）。
6. 金额币种：acg-faka 需要站长在店铺档案里填对方货币；我们在 `valuation` 带 `currency_code`，3.7.x 客户端会在不一致时写日志告警。
7. 兼容路径位于站点根的 `/shared/`，部署时需让反向代理把它转给后端（和 `/api/` 一样）。
