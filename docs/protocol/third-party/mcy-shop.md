# 萌次元商城（mcy-shop，V4/V5）对接能力规格

> 依据：`git clone` 的 `lizhipay/mcy-shop`（HEAD `a89c528`，5.0.35，2026-09-21），
> 交叉核对 `lizhipay/acg-faka` 中对接萌次元的客户端（`app/Service/Bind/Shared.php` 的 `type == 1` 分支）
> 与第三方实现 `GMWalletApp/gmshop-edge`（`src/features/suppliers/providers/acg.ts`，另一个对接同一接口的客户端）。
> 前缀：`mcy:` = mcy-shop 仓库，`acg:` = acg-faka 仓库，`gm:` = gmshop-edge 文件。
> wiki.mcy.im 处于 Cloudflare 人机验证之后，无法抓取；下文全部以源码为准。

## 0. 结论（先读这个）

1. **mcy-shop 核心代码里没有任何站点间对接协议。**
   - 核心的「货源 / 仓库（repertory）/ 供货商 / 进货」是**同一套安装内部**的多租户功能：主站、供货商、分站（商家）共用一个数据库，
     商家通过「供货码」`api_code` 查看并导入同站供货商的仓库商品（`mcy:app/Controller/User/API/Shop/Supply.php:66-99,186-196`，`mcy:app/Service/User/Bind/Supply.php`）。这不是跨站协议。
   - 所有前台/用户 API 都走 **会话 Cookie + AES-128-CBC 加密的请求/响应体**（请求头 `Secret`/`Signature`，`mcy:app/Interceptor/PostDecrypt.php:28-50`；
     响应体 `text/plain` 密文 + `Secret` 头，`mcy:app/Controller/User/Base.php:73-79`；登录态取自 Cookie，`mcy:app/Interceptor/User.php:33`），不能作为稳定的机器对接协议。
2. **跨站对接完全依赖插件**，且这些插件**不在开源仓库中**（应用商店分发，路由表与插件状态用硬件 ID 加密存储，`mcy:kernel/Plugin/Route.php:30-50`）：
   - **作为采购方**：通过「货源插件」实现 `ForeignShip` 接口（拉商品）+ `Ship` 接口（下单发货）。核心只定义接口与同步框架（§3）。
     能否对接 acg-faka / 另一个 mcy-shop，取决于是否安装了对应的商店插件；仓库里没有这些插件的实现，**无法从源码确定其线协议**。
   - **作为供货方**：需安装 `OpenApi` 插件，路由 `/plugin/open-api/*`（插件名 `OpenApi` 经 `camelToSnake` 得 `open-api`，`mcy:kernel/Util/Str.php:126-131`，`mcy:kernel/Plugin/Route.php:33-36`）。
     插件服务端源码不公开；**线协议只能从两个开源客户端反推**（acg-faka 的「萌次元(V4.0)」类型与 gmshop-edge），见 §2。
3. 用户凭证是核心字段：每个用户注册时生成 `app_key`（16 位大写，`mcy:app/Service/User/Bind/Auth.php:163`），个人中心显示「API-ID = 用户 id」「API密钥 = app_key」（`mcy:app/View/User/User/Personal.html:68-75`）。
   核心中**没有任何代码使用这两个值**——它们只被 OpenApi 插件消费。无审核流程（管理员可在后台重置，`mcy:app/Controller/Admin/API/User/User.php:151-152`）。
4. 可行性判断：
   - (a) 我们从 mcy-shop 进货：**可行但属「反推协议」**，前提是对方站点安装并启用了 OpenApi 插件。能力很弱：全量商品、SKU 级库存、按数量询价、同步下单返回 `contents`；**没有查单、没有回调、没有已确认的幂等**。
   - (b) mcy-shop 把我们当上游：mcy-shop 只能通过**货源插件**对接外站；我们无法为它提供「兼容接口」，除非我们模仿某个现成货源插件所对接的协议。
     最现实的路径：我们实现 **acg-faka `/shared/*` 兼容接口**（见 `acg-faka.md` §7.2），再由 mcy-shop 站长安装商店里「对接异次元」的货源插件（若有）。该插件的具体行为无法从源码验证。
     另一个选项：我们实现 **OpenApi 兼容接口**（`/plugin/open-api/*`），这能让 **acg-faka 的「萌次元(V4.0)」类型**和 gmshop-edge 这类客户端把我们当上游；是否能被 mcy-shop 自己的货源插件使用同样未知。

## 1. 签名算法（核心 `Str::generateSignature`，与 acg-faka 有一处不同）

`mcy:kernel/Util/Str.php:97-107`：

```php
unset($data['sign']);
ksort($data);
foreach ($data as $key => $val) {
    if ($val === '' || is_array($val)) unset($data[$key]);   // ← 与 acg-faka 不同：数组字段不参与签名
}
return md5(urldecode(http_build_query($data) . "&key=" . (string)$secret));
```

- 与 acg-faka（`acg:app/Util/Str.php:103-113`）唯一差异：**顶层数组值被排除**。其余（ksort、去顶层空串、`urldecode`、`&key=`、小写 md5）相同。
- 核心里它用于 `PostDecrypt`（前台加密 API 的签名，密钥是每次请求随机的 `Secret` 头，`PostDecrypt.php:36-45`）。OpenApi 插件大概率复用该函数（acg-faka 客户端就是用同一公式生成 `Api-Signature` 的），但**插件端的具体验签逻辑无法从源码确认**。

## 2. OpenApi 插件线协议（由客户端反推，未经服务端源码验证）

来源：`acg:app/Service/Bind/Shared.php:25-53`（`mcyRequest`）、`:165-234`、`:240-256`、`:312-326`、`:380-387`、`:403-418`、`:538-576`、`:666-671`；
`gm:acg.ts:36-162`（schema 由 zod 定义，比 acg-faka 客户端更严格，可作为形状的第二来源）。

### 2.1 传输与鉴权

- `POST <domain>/plugin/open-api/<endpoint>`，`Content-Type: application/x-www-form-urlencoded`，体为**仅业务字段**（不含 `app_id`/`sign`）。
- 请求头：
  - `Api-Id: <用户 id>`
  - `Api-Signature: md5(ksort(业务字段) 去空串 → k=v&… + "&key=" + app_key)`（客户端用 acg-faka 的 `Str::generateSignature`；业务字段均为扁平标量，所以两种实现结果一致。无字段时签名原串为 `&key=<app_key>`）。
- 超时 30 s，不跟随重定向（`Bind/Shared.php:28-37`）。
- 响应：`{"code":200,"msg":"…","data":…}`；`code != 200` 为失败（acg-faka 取 `msg` 显示；gmshop-edge 要求 HTTP 200 且 `code == 200`）。失败时的 `code` 取值未知。
- 无时间戳、无 nonce。

### 2.2 端点

| 端点 | 请求字段 | 响应 `data`（已知字段） |
|---|---|---|
| `connect` | — | `{"username": "<调用者用户名>", "balance": <数字或字符串>}` |
| `items` | — | 商品数组（见下），**全量、无分页** |
| `item` | `id=<商品 id>` | 单个商品对象（同下） |
| `sku/stock` | `sku_id` | `{"stock": <整数 \| 数字字符串 \| null \| 非数字字符串>}`；acg-faka 把非数字当作 999，gmshop 把 `null`/`-1` 当无限 |
| `sku/state` | `sku_id`, `quantity` | `{"state": <bool>}` 是否可购 |
| `amount` | `sku_id`, `quantity` | `{"amount": <总价>}` 按调用者身份的价格 |
| `trade` | `sku_id`, `quantity`, `trade_no`, `<控件 name>=<值>`… | `{"contents": "<交付内容>"}`；可能缺失（acg-faka 回退为 `此商品没有发货信息或正在发货中`，gmshop 视为 processing） |

商品对象（`gm:acg.ts:9-23`，`acg:Bind/Shared.php:176-234`）：

```json
{"id": 12, "name": "…", "introduce": "<HTML>", "picture_url": "https://…",
 "category": {"name": "分类名"},
 "widget": "[{\"title\":\"账号\",\"name\":\"account\",\"placeholder\":\"\",\"type\":\"text\",\"regex\":\"\",\"error\":\"\",\"data\":\"选项1\\n选项2\"}]",
 "sku": [{"id": 34, "name": "月卡", "stock_price": "8.50", "stock": 12}]}
```

- `sku[].stock_price`：调用者的拿货单价（推断：OpenApi 按调用用户计算，与核心 `RepertoryOrder::getAmount` 同口径；未验证）。
- `sku[].stock`：整数、数字字符串或 `null`；acg-faka 在所有 SKU 库存和为 0 时把商品库存设为 10000000（`Bind/Shared.php:223-230`），说明 `0`/`null` 在实践中常表示「不限/未知」。
- `widget`：JSON **字符串**，字段 `title,name,placeholder,type,regex,error,data`（`data` 为换行分隔的选项）。
- 没有商品状态、没有删除标记、没有分类 id（只有名称）。

### 2.3 下单与幂等

- `trade_no`：acg-faka 传 `substr(md5(本地订单号), 0, 24)`（`Bind/Shared.php:409`）——暗示服务端对 `trade_no` 有长度/字符限制（≤24？）并以此去重；gmshop-edge 在「对账」时**用同一 `trade_no` 重发 `trade`**（`gm:acg.ts:111-121`），暗示重复 `trade_no` 返回原订单内容。两者都是推断，**需要在真实站点上验证后才能声明 `idempotency`**。
- 交付：`contents` 字符串，多张卡密按行分隔（gmshop 按 `\r?\n` 切分）。核心的仓库订单是**同步发货**：`RepertoryOrder` 在同一事务内调用供货商发货插件的 `delivery()` 并把返回值存入 `contents`；库存不足/插件异常时 `contents` 为「库存不足，请申请售后」「发货失败，请直接申请售后…」之类文案（`mcy:app/Service/Common/Bind/RepertoryOrder.php:209-235`）。OpenApi 插件大概率把这个 `contents` 返回。
- **没有已知的查单端点**；无回调。

### 2.4 签名测试向量（Python 复刻，脚本 `scratchpad/thirdparty/vec/sign.py`）

`app_key = 8F3A2C9D1E7B6A54`，下游单号 `123456789012345678` → `trade_no = md5("123456789012345678")[0:24] = 9efebb3d7d059bff092842bf`：

| 业务字段（发送顺序） | 签名原串 | `Api-Signature` |
|---|---|---|
| `sku_id=17, quantity=1, trade_no=9efebb3d7d059bff092842bf, account=a b+c@x` | `account=a b+c@x&quantity=1&sku_id=17&trade_no=9efebb3d7d059bff092842bf&key=8F3A2C9D1E7B6A54` | `2f3078747abcffb750084230a4d60019` |

请求体：`sku_id=17&quantity=1&trade_no=9efebb3d7d059bff092842bf&account=a+b%2Bc%40x`，头 `Api-Id: 1024`。
（`urldecode` 使签名串中的空格与 `+` 均为原文；值中的空格在请求体里编码为 `+`，`+` 编码为 `%2B`。）

## 3. mcy-shop 作为采购方（核心框架，插件提供实现）

- 插件接口：
  - `ForeignShip`（`mcy:kernel/Plugin/Handle/ForeignShip.php`）：`getItems(): Item[]`、`getItem(string $uniqueId, array $options): ?Item`；
    构造参数为「货源配置」`PluginConfig.config`（站长在插件配置里填写的对方域名/密钥等，字段由插件定义，`mcy:kernel/Plugin/Abstract/ForeignShip.php`）。
  - `Ship`（`mcy:kernel/Plugin/Handle/Ship.php`）：`delivery(): string`（同步返回交付内容）、`stock()`、`hasEnoughStock(int)`、`inspection(array)`、自定义渲染。
- 中性商品结构：`Item{uniqueId=md5(对方 id), category(名称), name, introduce, pictureUrl, widgets[], attr[], skus[], versions{name,introduce,picture_url 的 md5}, options}`、
  `Sku{uniqueId=md5(对方 id), name, pictureUrl, price, cost?, options, message?, marketControl*…, versions{name,price,picture_url 的 md5}}`（`mcy:kernel/Plugin/Entity/Item.php`、`Sku.php`）。
- 导入：后台「货源 → 拉取」调用 `getItems()`，按分类名分组展示（`mcy:app/Controller/Admin/API/Plugin/Ship.php:36-73`），勾选后 `import` 建仓库商品（`RepertoryItem::import`，`mcy:app/Service/Common/Bind/RepertoryItem.php:58`），套用「同步模板」（加价百分比、汇率、保留小数、是否同步名称/图片/介绍/价格）。
- 同步：`getSyncRemoteItems(second=120)` 选出 `status=2`、`update_time` 超过 120 秒的远程商品（`RepertoryItem.php:667-685`），逐个 `syncRemoteItem`：`getItem()` → 比较 `versions` 摘要决定是否更新名称/图片/介绍 → SKU 按 `uniqueId` 增删改，成本 `cost ?? price` 每次都同步、价格仅在 `versions.price` 变化时按模板重算（`RepertoryItem.php:390-630`）。
  `getItem` 返回 null 计异常，连续 >6 次把商品转为审核中（`RepertoryItem.php:415-427`）。**调度由插件的进程触发**，核心只提供列表与单品同步函数（管理端也可手动触发，`Admin/API/Plugin/Ship.php:109-129`）。
- 下单：买家下单后 `RepertoryOrder` 在 serializable 事务里调供货商插件 `hasEnoughStock()` → `delivery()`，返回值即交付内容（同步）；失败写入售后提示文案，不抛出（非直购）。

## 4. 映射到我们的模型

### 4.1 采购方适配器（我们 → mcy-shop OpenApi，协议 ID 建议 `mcy-open-api`）

配置字段：`base_url`（url）、`api_id`（text，对方用户 id）、`app_key`（secret）。

| 能力 ID | 支持 | 说明 |
|---|---|---|
| `categories` | ⚠️ | 只有分类**名称**，无 id/层级；按名称生成稳定 id（如 名称的哈希） |
| `incremental_changes` | ❌ | 只能全量 `items` 轮询比对 |
| `push_events` | ❌ | |
| `quote` | ⚠️ 伪装 | `amount(sku_id, quantity)` 可询价，不锁价 |
| `multi_item` | ❌ | 一单一 SKU |
| `idempotency` | ❓ | `trade_no` 可能去重（§2.3 推断），未验证前不声明 |
| `encrypted_delivery` | ❌ | |

操作映射：`handshake/ping` → `connect`（`username` 作站点名的替代，货币需人工填写）；`list_products` → `items` 本地分页；`get_product(id)` → `item(id)`（数字 id 直接可用）；
SKU → 我们的 SKU（`id`、`name`、价格 `stock_price`、库存 `sku/stock`，`null`/非数字按「无限」或保守值处理）；`widget` → `manual_form_schema`（`title→label`，`data` 换行选项）；
`place_order` → 先 `sku/state` + `amount` 预检，再 `trade(sku_id, quantity, trade_no=<由我们的采购 id 派生的 ≤24 位 [0-9a-f]>，控件字段)`；`contents` 非空且非售后提示文案 → 已交付，否则只能人工处理（实现：`contents` 作为 `CreateOrderResponse.fulfillment` 交给核心，与 trade_no 同一事务落库 `procurement_deliveries` 后交付；售后文案 → `review` → 采购单 `manual_review`。**无查单接口**，`get_order` 返回 Unsupported 或以重发同一 `trade_no` 作为对账手段——必须先验证其幂等性）。
`cancel_order` 不支持。错误只有 `msg` 文本，按关键字分类（库存/余额），其余一律不可重试；网络超时 → 结果未知，不得自动重下。

**无法实现**：增量同步、推送、锁价、多商品、可靠幂等与查单、异步交付回传、加密交付、商品删除识别（只能「从全量中消失」）、分类层级。

### 4.2 供货方兼容（mcy-shop 或其生态 → 我们）

- **mcy-shop 核心无法把我们当上游**——它只能借货源插件。我们没有能力为「未知的闭源插件」提供兼容。
- 可做的兼容层（按价值排序）：
  1. **acg-faka `/shared/*` 兼容**（详见 `acg-faka.md` §7.2）——覆盖所有 acg-faka 站点，也覆盖任何实现了「对接异次元」的 mcy-shop 货源插件（若存在）。
  2. **OpenApi 兼容 `/plugin/open-api/*`**：按 §2 的形状实现 `connect/items/item/sku/stock/sku/state/amount/trade`，鉴权读 `Api-Id`（我们的凭证 id/key）与 `Api-Signature`（按 §1 公式、对表单体扁平字段现算，**排除数组字段**以兼容 mcy 的实现）。
     受益方：acg-faka 的「萌次元(V4.0)」店铺类型、gmshop-edge 等第三方。实现要点：
     - `items` 全量返回；`sku[].stock_price` 为调用者拿货价（两位小数字符串即可，客户端接受字符串或数字）；`stock` 返回整数，无限库存返回一个大数而非 `-1`（acg-faka 把 `-1` 当数字累加）。
     - `trade` 必须**同步**返回 `contents`（acg-faka 客户端把它直接当最终交付，永不查询，`Bind/Shared.php:416-417`）；以 `trade_no` 做幂等并对重复请求返回原 `contents`（兼容 gmshop 的对账重发）。
     - 控件字段以顶层表单字段到达，需按我们商品的 `manual_form_schema` 取值。
     - `category` 只输出 `{name}`。
- **棘手点**：OpenApi 的完整形状来自客户端反推，mcy-shop 官方插件的真实响应可能有更多字段或不同错误码；无时间戳/nonce，只能靠 HTTPS + 限流；`/plugin/…` 路径需在反向代理上转发到后端；同步交付约束同 acg-faka。

## 5. 历史与已知问题

- mcy-shop 仓库提交信息只有版本号（`git log`：5.0.14 → 5.0.35），与对接相关的核心文件（`RepertoryItem.php`、`Supply.php`、`Str.php`）历史中没有可识别的「对接协议」修复；`Supply.php` 注释记录了一处修复：禁止凭 id 直接访问隐藏（`privacy=1`）货源，必须凭 `api_code`（`mcy:app/Service/User/Bind/Supply.php:52`）——属同站内部功能。
- acg-faka 的萌次元客户端缺陷（我们实现客户端时要避免）：`mcyRequest` 把**所有**异常吞成 `连接失败#0`（`Bind/Shared.php:50-52`），丢失上游 `msg`；`trade` 响应缺 `contents` 时写入固定文案而非标记待处理；按 SKU 名映射 `shared_mapping`，SKU 改名即断链。
