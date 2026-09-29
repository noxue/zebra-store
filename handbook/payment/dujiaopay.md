# DujiaoPay

DujiaoPay 是加密货币收款服务，支持“交易模式”（本站显示地址和金额）和“收银台模式”（跳转）。

## 渠道配置

通用字段：提供方选 **DujiaoPay**，渠道类型（token_id）按 DujiaoPay 提供的币种标识**手动填写**。

| 字段 | 说明 |
|---|---|
| api_base_url | DujiaoPay 接口地址 |
| api_key_id / api_secret | API 凭证 |
| webhook_secret | 通知签名密钥 |
| order_mode | 交易模式 / 收银台模式（收银台模式只能跳转） |
| allowed_methods | 允许的支付方式 |
| fiat_currency | 标价法币，例如 `CNY` |
| success_url / cancel_url | 一般填 `https://你的域名/pay` |

在 DujiaoPay 后台把 Webhook 地址设为：

```text
https://你的域名/api/v1/payments/webhook/dujiaopay?channel_id=渠道ID
```
