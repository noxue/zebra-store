# Stripe

## 在 Stripe 准备

1. [Stripe Dashboard](https://dashboard.stripe.com) → Developers → API keys：记下 **Secret key** 和 **Publishable key**；
2. Developers → Webhooks → 添加端点：`https://你的域名/api/v1/payments/webhook/stripe?channel_id=渠道ID`，
   选择支付成功相关事件，记下 **Signing secret**（`whsec_` 开头）。

## 渠道配置

通用字段：提供方选 **官方**，渠道类型选 `stripe`，交互方式选 **跳转**。

| 字段 | 说明 |
|---|---|
| api_base_url | 一般保持默认 `https://api.stripe.com` |
| secret_key | `sk_live_…`（测试用 `sk_test_…`） |
| publishable_key | `pk_live_…` |
| webhook_secret | `whsec_…` |
| payment_method_types | 允许的支付方式，例如 `card`、`alipay`、`wechat_pay` |
| success_url / cancel_url | 一般都填 `https://你的域名/pay` |
| exchange_rate / target_currency | 例如 `USD` 和汇率 |

![Stripe 渠道配置](/screenshots/payment/stripe.png)
