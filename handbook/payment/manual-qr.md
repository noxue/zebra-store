# 人工收款码支付

## 配置渠道

在「支付渠道」新增 `manual_qr`，分别选择微信或支付宝，上传自己的收款码图片。两种渠道独立配置；启用哪个，买家结账时就能选择哪个。当前仅支持人民币订单，且仅用于商品订单。

确认页面基础地址应指向管理后台的实际入口，例如 `https://你的域名/admin`。后台创建渠道时会按当前后台地址预填；若使用自定义后台路径，请核对这项地址。

每笔支付在原应付金额至原应付金额加 `0.99` 元的 100 个两位小数金额中选一个。所有仍处于待支付状态的订单共同占用金额池；即使某次支付尝试标记为过期，在订单处理完成前也不会复用它的金额。金额池满时拒绝新支付，待已有订单处理后再试。买家看到的金额是最终应转账金额，管理员按这一金额核对实际到账。商品仍按原订单价格结算，差额是区分订单的收款尾数。

可配置一种或多种通知方式：

| 方式 | 配置 |
|---|---|
| Telegram | Bot Token；保存渠道后点击「绑定 Telegram」，打开生成的 `/start` 链接并向机器人发送，再点击「检查绑定」，自动写入 Chat ID |
| 企业微信 | 群机器人 Webhook URL，须为 `https://qyapi.weixin.qq.com/cgi-bin/webhook/send` |
| 钉钉 | 群机器人 Webhook URL，须为 `https://oapi.dingtalk.com/robot/send`；如机器人启用了加签，填入加签密钥 |
| 邮件 | SMTP 主机、端口、用户名、密码、发件地址、收件地址及 TLS/SSL 设置 |

Telegram 自动绑定通过 Bot API 的 `getUpdates` 读取 `/start` 消息；如果机器人已有 Webhook，Telegram 不允许同时读取更新，这时可直接填写 Chat ID，或使用专门用于此渠道的机器人。Telegram 的参数链接格式与限制见[官方 Deep Linking 文档](https://core.telegram.org/bots/features#deep-linking)，更新读取限制见[官方 Bot API](https://core.telegram.org/bots/api#getupdates)。

保存后点击「测试发送」验证通知。通知含订单号、商品摘要、应收金额和确认页面链接。测试消息中的订单号是示例，不对应真实订单。

## 买家报告已付款

人工收款码订单的付款页及未完成订单附近会显示「我已支付」。买家点击后填写**付款人姓名**，选择「付款后长时间未确认到账」或「支付金额与订单不符」，可补充说明。会员使用登录态、游客使用原订单凭据提交；姓名和反馈保存在对应的支付记录中，不能用于确认别人的订单。

提交后，系统用该收款渠道配置的 Telegram、企业微信、钉钉或邮件地址向管理员发送提示，列出订单号、应收金额、付款人姓名、问题和原确认页面链接。管理员应到收款账户核对交易，再在确认页查看反馈并按实际到账情况确认。买家点击「我已支付」本身**不会**把订单标记为已支付。为防止重复提醒，同一笔支付每小时最多提交三次；通知发送失败时可重试。

## 确认链接和密钥

每笔支付生成独立的随机确认链接，密钥放在 URL 的 `#key=` 片段中；浏览器打开页面后用它作为 Bearer 凭据调用下述同一组 API。片段不会随页面 HTTP 请求发送到服务器。不要把确认链接公开给买家。确认链接**没有固定到期时间**：订单待付款或已关闭但尚未确认时，链接仍可查看并用于相应的确认操作；确认完成后页面仍可查看状态，但不能再次确认。

渠道保存后会自动生成一个可复制的 API 密钥；也可在支付渠道配置中输入不少于 32 位的自定义密钥并点击「生成 / 更新密钥」。密钥明文仅在生成或修改时返回，系统只保存其摘要。修改渠道密钥后，旧渠道密钥立即失效；已经发出的逐笔确认链接继续有效，以免未处理订单的链接失效。关闭订单或软删除渠道也不会让尚未核实收款的链接失效；请保护好逐笔确认链接。请通过 HTTPS 使用链接和 API，避免在日志、截图或聊天记录中泄露密钥。

## 同一确认 API

买家反馈接口：会员调用 `POST /api/v1/payments/{payment_id}/manual-issue`，游客调用 `POST /api/v1/guest/payments/{payment_id}/manual-issue` 并携带原订单的 `Authorization: Guest …` 凭据。JSON 为 `{"payer_name":"张三","issue":"not_received","note":"已于 15:30 付款"}`；`issue` 也可为 `wrong_amount`。只有该支付对应订单的持有人能提交；接口只记录并通知，不会确认收款。

网页和外部程序调用同一 API。`Authorization: Bearer <渠道 API 密钥或该笔链接的 key>` 必填。接口使用标准响应封套 `{status_code,msg,data}`，业务错误可能仍返回 HTTP 200，应检查 `status_code`。

```http
GET /api/v1/manual-payments/{payment_id}
Authorization: Bearer <key>
```

返回 `order_no`、`channel_name`、`channel_type`、`amount`（例如 `"10.11"`）、`currency`、`payment_status`、`order_status`、`items`、`can_confirm` 和 `requires_force`。

核对收款账户到账金额后确认：

```http
POST /api/v1/manual-payments/{payment_id}/confirm
Authorization: Bearer <key>
Content-Type: application/json

{"received_amount":"10.11","force":false,"note":""}
```

`received_amount` 必须与支付记录金额完全一致。订单仍处于待支付状态时，即使计时已到或该笔支付尝试标记为过期，核实到账后仍可正常确认，并沿用既有支付成功与履约流程。订单已关闭时，确认页会显示强制确认按钮；确认实际到账后提交 `{"received_amount":"10.11","force":true,"note":"核对依据"}`。备注必填。强制确认把订单和支付记录转为已收款、待人工处理；此前释放的库存、优惠券或余额不会自动重新预留，也不会自动发货。重复确认会被拒绝。

外部监控程序先读取 GET 结果，以 `payment_id`、金额和到账事实匹配后再调用 POST。不要仅根据用户提供的截图或订单号确认收款。
