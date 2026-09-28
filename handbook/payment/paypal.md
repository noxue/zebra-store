# PayPal

## 在 PayPal 准备

1. 登录 [PayPal Developer](https://developer.paypal.com) → Apps & Credentials → 创建一个 App，得到 **Client ID** 和 **Secret**；
2. 在 App 里添加 **Webhook**：地址填 `https://你的域名/api/v1/payments/webhook/paypal?channel_id=渠道ID`（先在本站建好渠道拿到 ID），
   订阅支付完成相关事件，记下 **Webhook ID**。

## 渠道配置

通用字段：提供方选 **官方**，渠道类型选 `paypal`，交互方式选 **跳转**。

| 字段 | 说明 |
|---|---|
| base_url | 正式环境 `https://api-m.paypal.com`；沙箱 `https://api-m.sandbox.paypal.com` |
| client_id / client_secret | App 的 Client ID 和 Secret |
| webhook_id | Webhook ID，用于验证通知真伪 |
| brand_name | 付款页显示的商家名 |
| locale | 付款页语言，例如 `zh-CN` |
| return_url / cancel_url | 付款完成 / 取消后跳回的地址，一般都填 `https://你的域名/pay` |
| exchange_rate / target_currency | PayPal 不支持人民币收款时，填 `USD` 和汇率 |

买家从 PayPal 跳回后，支付页会自动完成扣款确认。

![PayPal 渠道配置](/screenshots/payment/paypal.png)
