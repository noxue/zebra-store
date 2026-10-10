# 卡密转换器 HTTP 协议

本文档定义 Zebra Store 与外部卡密转换程序的最小 HTTP 合约。转换器由第三方独立部署；它收到上游卡密并验证/导入，返回自身系统生成、可供最终用户使用的卡密。Zebra Store 只负责按商品映射调用，不内置转换器专属字段名或业务类型。当前只有这一套协议；版本仅体现在接口 URL 的 `/v1/` 路径中，请求和响应 JSON 不携带版本字段。

## 安全认证

每个转换器必须为 Zebra Store 单独创建专用认证 Token，只授予健康探测和卡密转换所需权限。不得把管理员 Token、主 API Token 或拥有账户管理/资金操作权限的高权限凭证填入此处。Token 应可独立轮换和撤销。生产接口必须使用 HTTPS；Zebra Store 保存凭证时加密，接口响应、日志、截图和错误信息不得回显 Token 或完整卡密。

请求使用 `Authorization: Bearer <专用 token>`。转换器以请求体中的 `order_no` 作为幂等标识，应持久化结果：相同订单号、相同请求返回原结果；相同订单号、不同请求返回 `409 idempotency_conflict`。

## 接口

转换器提供 `POST /integration/v1/exchanges` 和 `GET /health`。套餐、上游来源等业务值由管理员按商品配置在 `extra` 中；转换器自行校验，不要求类型发现接口。

## 转换

`POST /integration/v1/exchanges`

```http
Authorization: Bearer <converter-token>
Content-Type: application/json
```

```json
{
  "order_no": "ZS202609300001",
  "items": [{"index": 0, "upstream_card": "UPSTREAM-CARD-1"}, {"index": 1, "upstream_card": "UPSTREAM-CARD-2"}],
  "extra": {"region": "us", "merchant_order": "ZS202609300001", "custom_flag": true}
}
```

成功返回的 `items` 数量必须与请求数量完全相同，且每个输入 `index` 恰好对应一个非空输出；顺序可不同，但不得遗漏、重复或凭空增加。类型或输出数量不匹配时必须整体拒绝，不得返回部分成功。Zebra Store 只有在全部结果通过校验后才保存正式交付并完成订单。

```json
{
  "items": [
    {"index": 0, "card": "SYSTEM-CARD-1"},
    {"index": 1, "card": "SYSTEM-CARD-2"}
  ]
}
```

转换器不得将 `upstream_card` 原样作为成功输出，除非该程序明确将其作为有效系统卡密验证并接受。响应体应使用 UTF-8 JSON，默认限制 1 MiB；接口应在 15 秒内响应。若生成结果可能需要更长处理时间，应在超时前返回可轮询的异步任务能力（当前暂不定义异步结果端点）。

## 错误约定

错误使用 HTTP 状态码和以下 JSON 形状；不得在错误中回显卡密或凭证。

```json
{"error":{"code":"type_mismatch","message":"上游卡密类型与配置不匹配","retryable":false}}
```

| HTTP | code | retryable | 含义 |
|---|---|---:|---|
| 400 | `invalid_request` | 否 | 请求结构或字段无效 |
| 401/403 | `unauthorized` | 否 | 专用 Token 无效或权限不足 |
| 409 | `idempotency_conflict` | 否 | 同一订单号对应不同请求 |
| 422 | `type_mismatch` / `card_rejected` | 否 | 类型不匹配或上游卡密被拒绝 |
| 429 | `rate_limited` | 是 | 限流；可带 `Retry-After` |
| 5xx | `temporarily_unavailable` | 是 | 临时服务故障 |

网络超时、连接错误、429 和 5xx 可安全使用**同一个订单号**重试。Zebra Store 不自动跟随重定向；转换器端点必须直接返回结果。配置的服务地址应是公开 HTTPS 主机，不能指向回环、私网或云元数据地址。

## 采购成功后的失败恢复

Zebra Store 在收到上游采购成功结果后，会先保存原始交付作为内部待转换内容，再调用本接口。若转换失败，订单保持处理中，系统重试的是转换请求，不会再次向上游采购。重试会使用同一 `order_no` 和相同请求内容。转换器恢复健康后，店铺会唤醒待转换任务；只有转换全部成功后才写入买家可见的正式交付、完成订单并发送通知。永久类型错误或重试耗尽会进入待人工处理，管理员修正绑定后可重试原转换任务。原始卡密仅供内部转换与审计，不能由买家查单、下游供货 API、邮件或 Webhook 返回。

本地库存商品使用相同的交付边界：系统先锁定并标记本次库存卡密已使用，同时创建空的待处理交付，再调用转换器。失败时订单保持处理中；定时任务或管理员重试会复用同一批卡密和同一个订单号，不会再扣一批库存。转换器恢复且整批转换成功后，系统才填入正式交付、完成订单并通知买家。转换期间空的待处理记录不会向买家展示原始卡密。

管理员手动发货也遵循商品绑定：输入逐行卡密且数量正确时，在保存交付前调用转换器；失败时不保存交付，管理员可用相同输入重试，转换器通过相同订单号返回同一结果。上游返回的人工处理说明不会按卡密转换。

## 商品绑定与请求上下文

一条转换器连接代表一个第三方程序，可供多个商品/SKU 复用。管理员给每个商品/SKU 绑定转换器，并直接配置该商品的 `extra`。商品选择器应显示本地商品/SKU 及其已有上游来源，供管理员辨认；这不要求在手工导入库存时记录卡密来源。商品只要绑定转换器，购买交付就调用转换器；没有绑定则按原流程交付。

请求主体顶层只有订单号 `order_no`、待转换卡密列表 `items` 和管理员配置的 `extra`。站点、商品和 SKU 信息不会自动传给转换器。`extra` 的字段名由管理员按转换器要求自行定义，Store 不固定 `extra` 中的地区、套餐、来源等字段名。字段值支持 JSON 常量和这些上下文字段的占位符：`&#123;&#123;order.order_no&#125;&#125;`、`&#123;&#123;order.quantity&#125;&#125;`、`&#123;&#123;order.total_amount&#125;&#125;`、`&#123;&#123;site.domain&#125;&#125;`、`&#123;&#123;site.name&#125;&#125;`、`&#123;&#123;product.id&#125;&#125;`、`&#123;&#123;product.title&#125;&#125;`、`&#123;&#123;sku.id&#125;&#125;`、`&#123;&#123;sku.specifications&#125;&#125;`、`&#123;&#123;source.name&#125;&#125;`、`&#123;&#123;source.id&#125;&#125;`、`&#123;&#123;source.protocol&#125;&#125;`。后台普通模式提供变量选择器和字段表单，高级模式编辑完整 JSON；两种模式可互相同步。保存前必须先将高级模式内容解析成 JSON 对象，并校验条件规则与变量路径。占位符以 JSON 字符串保存，交付时在内存中替换为订单值，再由 JSON 序列化发送；不执行脚本。邮箱、手机号等个人信息默认不发送。后台可预览请求结构，并可编辑或复制已保存的绑定。

## AI Top-up 示例端点

AI Top-up 提供本协议的示例实现：`POST /integration/v1/exchanges`，并提供 `GET /health`。

其部署必须单独设置 `ZEBRA_STORE_TOKEN`，以 `X-Integration-Token` 传递。此 Token 与后台 `ADMIN_TOKEN` 分离；转换器请求不得使用管理员 Token。

```json
{
  "order_no": "ZS202609300001",
  "extra": {"category": "new_activation_go", "provider": "p1"},
  "items": [
    {"index": 0, "upstream_card": "UPSTREAM-CARD-1"}
  ]
}
```

请求头：`X-Integration-Token: <ZEBRA_STORE_TOKEN>` 或 `Authorization: Bearer <ZEBRA_STORE_TOKEN>`。成功响应包含 `items: [{"index":0,"card":"...","type_id":"...","provider":"p1"}]`。AI Top-up 校验卡密实际套餐与 `extra.category`，验证全部输入后在一个数据库事务中导入上游卡密并绑定生成系统卡密；卡密序号必须从 0 连续排列，类型不符时整体失败。对同一订单号重复提交相同请求返回原卡密，复用订单号但请求不同返回 HTTP 409。AI Top-up 的套餐类型在自身服务内固定校验；Store 按商品配置 `extra.category`。

AI Top-up 要求 `extra.category` 为其支持的套餐类型，`extra.provider` 为 `p1` 或 `p2`。Store 不内置这些业务值；管理员按商品配置。缺失或无效时整批拒绝。

## 条件生成 extra

普通模式可把任一字段切换为“条件规则”：按顺序添加条件，选择来源、订单值或“卡密”，再选择等于、不等于、大小比较、正则匹配或正则不匹配。第一条命中即输出指定的固定值或变量；均未命中时使用“否则”值。卡密条件要求本批每张卡密都匹配。上游来源取自实际采购订单关联的连接；本地库存或没有关联来源时为空，来源条件不会命中，可用默认值处理。

高级 JSON 中，同一规则写成以下结构，切回普通模式仍显示为可编辑的条件表：

```json
{
  "provider": {"$zs_rule": {
    "cases": [
      {"when": {"variable": "source.name", "operator": "eq", "value": "上游 A"}, "then": {"literal": "p1"}},
      {"when": {"variable": "card", "operator": "regex", "value": "^P2-"}, "then": {"literal": "p2"}}
    ],
    "else": {"literal": "p1"}
  }},
  "category": {"$zs_rule": {
    "cases": [{"when": {"variable": "order.quantity", "operator": "gt", "value": "1"}, "then": {"literal": "new_activation_go"}}],
    "else": {"literal": "new_activation_go"}
  }}
}
```

规则配置只保存在 Store，发送给转换器的是求值后的普通 JSON。条件比较失败或来源缺失时继续下一条；正则表达式和规则结构在保存前校验。`source.id` 比可改名的 `source.name` 更适合需要稳定匹配的配置。

后台的上游下拉框显示连接名称和 ID。例如选择「渠道1（ID 7）」时，规则保存 `source.id = "7"`；在「发送固定值」填 `p1` 后，命中规则的请求 `extra` 为 `{"provider":"p1"}`。连接名称只用于后台识别，不会自动发给转换器；在上游连接的名称输入框下也有此说明。规则条件与变量的正式清单及边界见 [`card-converter-extra-contract.json`](./card-converter-extra-contract.json)，前端与 Rust 测试均按此文件核对。

绑定保存时校验模板；打开或刷新转换器管理页时，后台重新检查现有绑定的商品、SKU、转换器与可用上游。商品删除时，绑定在同一事务内删除；SKU 删除时，对应的 SKU 绑定一并删除。商品、SKU、上游连接在绑定保存后更新，且模板引用了对应变量时，绑定列表显示复核提示；来源等于条件已无匹配时也显示提示。订单号、数量、金额来自具体订单；站点名称/域名及商品、SKU、上游变量在交付时以当前上下文求值，最后再校验并序列化为 JSON。更新站点设置后，使用站点变量的配置应在转换器页预览并复核。
