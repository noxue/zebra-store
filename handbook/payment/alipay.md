# 支付宝官方

直接对接支付宝开放平台，需要企业或个体工商户资质。

## 在支付宝开放平台准备

1. 在 [支付宝开放平台](https://open.alipay.com) 创建应用，签约“电脑网站支付”“手机网站支付”或“当面付”；
2. 用支付宝提供的工具生成**应用私钥**和**应用公钥**，把应用公钥上传到开放平台；
3. 记下 **APPID** 和**支付宝公钥**（如果用证书模式，还要下载证书并记下证书序列号）。

## 渠道配置

通用字段：提供方选 **官方**，渠道类型选 `alipay`，交互方式可选 **二维码**（当面付）、**手机网页**、**电脑网页**。

| 字段 | 说明 |
|---|---|
| app_id | 应用的 APPID |
| gateway_url | 支付宝网关，正式环境 `https://openapi.alipay.com/gateway.do` |
| private_key | 应用私钥 |
| alipay_public_key | 支付宝公钥（公钥模式） |
| app_cert_sn / alipay_root_cert_sn | 证书模式时的应用证书序列号和支付宝根证书序列号 |
| sign_type | 一般是 `RSA2` |
| notify_url | `https://你的域名/api/v1/payments/callback` |
| return_url | `https://你的域名/pay` |
| exchange_rate / target_currency | 同币种不用填 |

![支付宝渠道配置](/screenshots/payment/alipay.png)

## 常见问题

**验签失败**：`alipay_public_key` 填成了应用公钥。这里要填**支付宝公钥**。
