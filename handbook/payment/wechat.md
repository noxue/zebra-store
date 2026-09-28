# 微信支付官方

对接微信支付 API v3，需要微信支付商户号。

## 在微信支付商户平台准备

1. 在 [微信支付商户平台](https://pay.weixin.qq.com) 获取 **商户号（mchid）**，并关联一个公众号或小程序的 **AppID**；
2. 申请 **API 证书**，得到商户私钥文件和**证书序列号**；
3. 设置 **APIv3 密钥**；
4. 下载平台证书，或在商户平台开启并下载 **微信支付公钥**（新商户推荐公钥模式）。

## 渠道配置

通用字段：提供方选 **官方**，渠道类型选 `wechat`，交互方式选 **二维码**（Native 扫码）或 **手机网页**（H5）。

| 字段 | 说明 |
|---|---|
| appid | 关联的公众号 / 小程序 AppID |
| mchid | 商户号 |
| merchant_serial_no | 商户 API 证书序列号 |
| merchant_private_key | 商户 API 私钥（`apiclient_key.pem` 的内容） |
| api_v3_key | APIv3 密钥 |
| verification_mode | 验签方式：平台证书 / 微信支付公钥 / 两者兼容 |
| wechatpay_public_key / wechatpay_public_key_id | 公钥模式时填写 |
| notify_url | `https://你的域名/api/v1/payments/callback` |
| h5_type / h5_wap_name / h5_wap_url / h5_redirect_url | H5 支付需要的场景信息 |
| exchange_rate / target_currency | 同币种不用填 |

保存后点 **测试公钥**，确认微信支付公钥配置正确。

![微信支付渠道配置](/screenshots/payment/wechat.png)

## 常见问题

**H5 支付提示商家参数格式有误**：H5 需要在商户平台单独开通，并配置授权域名。
