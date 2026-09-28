# 加密货币网关

斑马小铺支持四种自建或第三方的加密货币收款网关。买家在支付页会看到**收款地址**、**网络（链）**和**精确金额**，
必须按页面上的金额和网络付款。

::: warning 金额要精确
加密货币网关通常靠“金额”区分不同订单，买家少付或多付都可能无法自动到账。请在商品说明里提醒买家。
:::

## BEpusdt

支持 `usdt-trc20`、`usdc-trc20`、`trx` 等渠道类型。

| 字段 | 说明 |
|---|---|
| gateway_url | 你部署的 BEpusdt 地址 |
| auth_token | BEpusdt 的对接令牌 |
| trade_type | 交易类型（链与币种） |
| fiat | 法币，例如 `CNY` |
| currencies | 允许的币种 |
| order_mode | `transaction`（本站显示地址和金额）或 `cashier`（跳转到收银台，此时交互方式只能是跳转） |
| notify_url | `https://你的域名/api/v1/payments/callback` |
| return_url | `https://你的域名/pay` |

## epusdt

| 字段 | 说明 |
|---|---|
| gateway_url | epusdt 地址 |
| token / pid / secret_key | epusdt 后台的对接参数 |
| network / currency | 网络和币种 |
| order_mode | 同上 |
| notify_url / return_url | 同上 |

## OKPay

支持 `usdt`、`trx`。

| 字段 | 说明 |
|---|---|
| gateway_url | OKPay 接口地址 |
| merchant_id / merchant_token | 商户 ID 和令牌 |
| display_name | 收银台显示的名称 |
| exchange_rate | 汇率 |
| callback_url | `https://你的域名/api/v1/payments/callback` |
| return_url | `https://你的域名/pay` |

## TokenPay

| 字段 | 说明 |
|---|---|
| gateway_url | TokenPay 地址 |
| notify_secret | 通知密钥 |
| currency / base_currency | 收款币种和标价币种 |
| notify_url | `https://你的域名/api/v1/payments/callback` |
| redirect_url | `https://你的域名/pay` |

![加密货币支付页](/screenshots/payment/crypto-pay-page.png)
