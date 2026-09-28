# 易支付 epay

“易支付”是很多第三方聚合支付平台通用的接口，通常同时提供支付宝、微信、QQ 钱包。

## 在平台上准备

在你使用的易支付平台注册商户，拿到：**接口地址**、**商户 ID**、**商户密钥**（v1 用 MD5 密钥；v2 用 RSA 密钥对）。

## 渠道配置

通用字段：提供方选 **易支付（epay）**，渠道类型选 `alipay`、`wechat` 或 `qqpay`，交互方式一般选 **跳转**。
想同时提供支付宝和微信，就建两个渠道。

| 字段 | 说明 |
|---|---|
| 接口版本 | `v1`（MD5 签名，最常见）或 `v2`（RSA 签名） |
| 网关地址（gateway_url） | 平台给的接口地址，例如 `https://pay.example.com/` |
| 商户 ID（merchant_id） | |
| 商户密钥（merchant_key） | v1 使用 |
| 商户私钥（private_key） | v2 使用 |
| 平台公钥（platform_public_key） | v2 使用 |
| 通知地址（notify_url） | `https://你的域名/api/v1/payments/callback` |
| 返回地址（return_url） | `https://你的域名/pay` |
| 汇率 / 目标货币 | 同币种不用填 |

![易支付渠道配置](/screenshots/payment/epay.png)

## 常见问题

**签名错误**：v1 / v2 选错，或者密钥复制时带了空格。

**回调没到**：检查平台后台是否也设置了通知地址，以及本站是否是 HTTPS 且能从外网访问。
