# 卡密转换器 HTTP 协议 v1

本文档定义 Zebra Store 与外部卡密转换程序的最小 HTTP 合约。转换器由第三方独立部署；它收到上游卡密并验证/导入，返回自身系统生成、可供最终用户使用的卡密。Zebra Store 只负责按商品映射调用，不依赖具体业务类型。

## 安全认证

每个转换器必须为 Zebra Store 单独创建专用认证 Token，只授予类型读取、健康探测和卡密转换所需权限。不得把管理员 Token、主 API Token 或拥有账户管理/资金操作权限的高权限凭证填入此处。Token 应可独立轮换和撤销。生产接口必须使用 HTTPS；Zebra Store 保存凭证时加密，接口响应、日志、截图和错误信息不得回显 Token 或完整卡密。

请求使用 `Authorization: Bearer <专用 token>`。转换请求另带 `Idempotency-Key`，其值对同一 Zebra Store 订单行和卡密序号稳定且唯一。转换器应持久化幂等结果：相同 key、相同请求返回原结果；相同 key、不同请求返回 `409 idempotency_conflict`。

## 类型发现

`GET /integration/v1/types`

```http
Authorization: Bearer <converter-token>
Accept: application/json
```

```json
{
  "protocol_version": "1",
  "types": [
    {
      "id": "new_activation_go",
      "name": "Go 新开通",
      "description": "可选说明",
      "fields": [
        {"key": "region", "label": "区域", "required": true, "kind": "select", "options": ["us", "eu"]},
        {"key": "campaign", "label": "活动代码", "required": false, "kind": "text"}
      ]
    }
  ]
}
```

`id` 是转换器自己的稳定类型 ID，创建后不应随展示名称改变。Zebra Store 不内置或推断这些值。`fields` 可选；提供后，管理后台据此生成商品参数表单，支持 `text`、`number`、`boolean`、`select`。必填项在绑定保存和下单前校验。未声明的版本 1 字段不要求店铺手工填写。类型发现是配置商品绑定前的必需步骤。健康探测使用 `GET /health`（可选）；不提供此端点的转换器由管理端通过类型发现和受控探测判断连通性。

## 转换

`POST /integration/v1/exchanges`

```http
Authorization: Bearer <converter-token>
Idempotency-Key: order-<opaque-id>-item-<index>
Content-Type: application/json
```

```json
{
  "protocol_version": "1",
  "type_id": "new_activation_go",
  "order": {"order_no": "ZS202609300001", "quantity": 2, "currency": "CNY"},
  "site": {"domain": "store.example.com", "name": "Example Store"},
  "product": {
    "id": 12,
    "slug": "activation",
    "title": "新开通",
    "variables": {"region": "us"}
  },
  "sku": {"id": 34, "code": "go-us", "specifications": {"region": "US"}},
  "items": [{"index": 0, "upstream_card": "UPSTREAM-CARD-1"}],
  "extra": {"merchant": "store-a", "custom_flag": true}
}
```

成功返回的 `items` 数量必须与请求数量完全相同，且每个输入 `index` 恰好对应一个非空输出；顺序可不同，但不得遗漏、重复或凭空增加。类型和数量不匹配时必须整体拒绝，不得返回部分成功。Zebra Store 只有在全部结果通过校验后才保存正式交付并完成订单。

```json
{
  "protocol_version": "1",
  "items": [
    {"index": 0, "card": "SYSTEM-CARD-1"},
    {"index": 1, "card": "SYSTEM-CARD-2"}
  ]
}
```

转换器不得将 `upstream_card` 原样作为成功输出，除非该程序明确将其作为有效系统卡密验证并接受。响应体应使用 UTF-8 JSON，默认限制 1 MiB；接口应在 15 秒内响应。若生成结果可能需要更长处理时间，应在超时前返回可轮询的异步任务能力（v1 初版暂不定义异步结果端点）。

## 错误约定

错误使用 HTTP 状态码和以下 JSON 形状；不得在错误中回显卡密或凭证。

```json
{"error":{"code":"type_mismatch","message":"上游卡密类型与配置不匹配","retryable":false}}
```

| HTTP | code | retryable | 含义 |
|---|---|---:|---|
| 400 | `invalid_request` | 否 | 请求结构或字段无效 |
| 401/403 | `unauthorized` | 否 | 专用 Token 无效或权限不足 |
| 409 | `idempotency_conflict` | 否 | 幂等键被不同请求复用 |
| 422 | `type_mismatch` / `card_rejected` | 否 | 类型不匹配或上游卡密被拒绝 |
| 429 | `rate_limited` | 是 | 限流；可带 `Retry-After` |
| 5xx | `temporarily_unavailable` | 是 | 临时服务故障 |

网络超时、连接错误、429 和 5xx 可安全使用**同一个幂等键**重试。Zebra Store 不自动跟随重定向；转换器端点必须直接返回结果。配置的服务地址应是公开 HTTPS 主机，不能指向回环、私网或云元数据地址。

## 采购成功后的失败恢复

Zebra Store 在收到上游采购成功结果后，会先保存原始交付作为内部待转换内容，再调用本接口。若转换失败，订单保持处理中，系统重试的是转换请求，不会再次向上游采购。重试会使用同一 `Idempotency-Key` 和相同请求内容。转换器恢复健康后，店铺会唤醒待转换任务；只有转换全部成功后才写入买家可见的正式交付、完成订单并发送通知。永久类型错误或重试耗尽会进入待人工处理，管理员修正绑定后可重试原转换任务。原始卡密仅供内部转换与审计，不能由买家查单、下游供货 API、邮件或 Webhook 返回。

本地库存商品使用相同的交付边界：系统先锁定并标记本次库存卡密已使用，同时创建空的待处理交付，再调用转换器。失败时订单保持处理中；定时任务或管理员重试会复用同一批卡密和同一个幂等键，不会再扣一批库存。转换器恢复且整批转换成功后，系统才填入正式交付、完成订单并通知买家。转换期间空的待处理记录不会向买家展示原始卡密。

管理员手动发货也遵循商品绑定：输入逐行卡密且数量正确时，在保存交付前调用转换器；失败时不保存交付，管理员可用相同输入重试，转换器通过稳定幂等键返回同一结果。上游返回的人工处理说明不会按卡密转换。

## 商品绑定与请求上下文

一条转换器连接代表一个第三方程序，可供多个商品/SKU 复用。管理员给每个商品/SKU 绑定连接返回的类型，并填写该商品的类型专属字段和 `extra`。商品选择器应显示本地商品/SKU 及其已有上游来源，供管理员辨认；这不要求在手工导入库存时记录卡密来源。商品只要绑定转换器，购买交付就调用转换器；没有绑定则按原流程交付。

Zebra Store 自动提供订单号/数量/币种、可信站点配置、商品及 SKU 快照和待转换的上游卡密。管理员可用 `extra` 设置常量或受限变量，例如 `&#123;&#123;site.domain&#125;&#125;`、`&#123;&#123;product.title&#125;&#125;`、`&#123;&#123;product.variables.region&#125;&#125;`。模板只做值替换，不执行脚本或表达式。邮箱、手机号等个人信息默认不发送；第三方只能拿到被显式选中的字段。后台应预览最终请求，并可复制已保存的商品绑定配置。

## AI Top-up 示例端点

AI Top-up 提供本协议的示例实现：

- `GET /integration/v1/types`
- `POST /integration/v1/exchanges`

其部署必须单独设置 `ZEBRA_STORE_TOKEN`，以 `X-Integration-Token` 传递。此 Token 与后台 `ADMIN_TOKEN` 分离；转换器请求不得使用管理员 Token。

```json
{
  "protocol_version": "1",
  "idempotency_key": "ZS202609300001:0",
  "type_id": "new_activation_go",
  "order": {"order_no": "ZS202609300001", "quantity": 1, "currency": "CNY"},
  "site": {"domain": "store.example.com", "name": "Example Store"},
  "product": {"id": 12, "slug": "activation", "title": "新开通", "variables": {"region": "us"}},
  "sku": {"id": 34, "code": "go-us", "specifications": {"region": "US"}},
  "extra": {"region": "us"},
  "items": [
    {"index": 0, "upstream_card": "P1-...", "provider": "p1"}
  ]
}
```

请求头：`X-Integration-Token: <ZEBRA_STORE_TOKEN>` 或 `Authorization: Bearer <ZEBRA_STORE_TOKEN>`。成功响应包含 `protocol_version` 与 `items: [{"index":0,"card":"...","type_id":"...","provider":"p1"}]`。AI Top-up 校验卡密实际套餐与 `type_id`，验证全部输入后在一个数据库事务中导入上游卡密并绑定生成系统卡密；输入数量和连续序号必须正确，类型不符时整体失败。对同一幂等键重复提交相同请求返回原卡密，复用幂等键但请求不同返回 HTTP 409。AI Top-up 的类型 ID 当前由其支持的类型动态发布；调用方仍必须先读取类型列表。
