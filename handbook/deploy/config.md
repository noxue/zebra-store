# 配置文件详解

后端启动时读取 `config.yml`（用 `--config 路径` 指定，默认是当前目录下的 `config.yml`）。
仓库里的 `backend/config.example.yml` 是模板，复制一份改名即可。

- 配置文件里**没写的项**使用下表中的默认值；
- 每一项都可以用环境变量覆盖，见 [环境变量覆盖](./env)；
- 改完配置要**重启后端**才生效。

::: tip 哪些东西不在配置文件里
站点名、Logo、主题色、支付渠道、邮件 SMTP、验证码、注册开关等“运营设置”都在**管理后台**里改，
保存后立即生效，不用重启。配置文件只放“启动时就要确定”的东西。
:::

## app：应用密钥

| 键 | 默认 | 说明 |
|---|---|---|
| `app.secret_key` | 无，必填 | 加密数据库敏感字段（支付密钥、对接密钥等）用的密钥来源。至少 16 位，**上线后不要修改**，否则已加密的数据无法解密 |
| `app.totp_issuer` | `Zebra-Store` | 两步验证 App 里显示的发行方名称 |

## server：HTTP 服务

| 键 | 默认 | 说明 |
|---|---|---|
| `server.host` | `0.0.0.0` | 监听地址。前面有反向代理时建议 `127.0.0.1`；Docker 里保持 `0.0.0.0` |
| `server.port` | `8081` | 监听端口 |
| `server.mode` | `debug` | `debug` 或 `release`，生产环境用 `release` |
| `server.trusted_proxies` | `["127.0.0.1/32", "::1/128"]` | 信任哪些反向代理传来的真实 IP（`X-Forwarded-For`）。IP 或 CIDR 网段；拒绝 `0.0.0.0/0` |

## log：日志

| 键 | 默认 | 说明 |
|---|---|---|
| `log.level` | `info` | `trace` / `debug` / `info` / `warn` / `error`，也可以写更细的过滤规则，例如 `info,zs_infra=debug` |
| `log.json` | `false` | `true` 时每行输出一个 JSON，方便接入日志系统 |
| `log.dir` | 空 | 保留项（与原项目一致），当前版本日志只输出到标准输出，由 systemd / Docker 收集 |

## database：数据库

| 键 | 默认 | 说明 |
|---|---|---|
| `database.url` | `sqlite://data/zebra.db?mode=rwc` | 数据库地址，开头决定类型，见 [切换数据库](./database) |
| `database.max_connections` | `10` | 连接池上限 |
| `database.min_connections` | `1` | 连接池下限 |
| `database.connect_timeout_seconds` | `10` | 连接超时 |
| `database.idle_timeout_seconds` | `600` | 空闲连接回收时间 |
| `database.sql_log` | `false` | 打印 SQL |

## jwt / user_jwt：登录令牌

| 键 | 默认 | 说明 |
|---|---|---|
| `jwt.secret` | 无，必填 | 管理员登录令牌的签名密钥，至少 16 位 |
| `jwt.expire_hours` | `24` | 管理员登录多久后过期（小时） |
| `user_jwt.secret` | 无，必填 | 买家登录令牌的签名密钥，至少 16 位 |
| `user_jwt.expire_hours` | `24` | 买家登录有效期（小时） |
| `user_jwt.remember_me_expire_hours` | `168` | 勾选“记住我”时的有效期（小时，默认 7 天） |

`app.secret_key`、`jwt.secret`、`user_jwt.secret` 三个值**必须互不相同**，否则程序拒绝启动。
修改 `jwt.secret` 或 `user_jwt.secret` 会让所有人需要重新登录。

## bootstrap：首次启动

| 键 | 默认 | 说明 |
|---|---|---|
| `bootstrap.default_admin_username` | `admin` | 第一次启动时创建的超级管理员用户名 |
| `bootstrap.default_admin_password` | 空 | 这个管理员的密码。只在数据库里还没有该管理员时使用，之后修改它**不会**改变已有密码（改密码请在后台或用命令行 `admin reset-password`） |

## telegram_auth / google_auth：第三方登录的初始值

这两段是 Telegram 登录和 Google 登录的**初始值**，后台 **系统设置 → Telegram设置 / Google登录** 保存后以后台为准。

| 键 | 说明 |
|---|---|
| `telegram_auth.enabled`、`bot_username`、`bot_token`、`client_secret`、`oidc_redirect_uri`、`mini_app_url`、`login_expire_seconds`、`replay_ttl_seconds` | Telegram 登录（Widget / OIDC / Mini App） |
| `google_auth.enabled`、`client_id` | Google 登录 |

## redis：可选

| 键 | 默认 | 说明 |
|---|---|---|
| `redis.enabled` | `false` | 保留项。当前版本的缓存、限流和任务队列都在进程内和数据库中实现，**不需要 Redis** |
| `redis.url` | `redis://127.0.0.1:6379/0` | 同上 |
| `redis.prefix` | `zs` | 同上 |

## queue：后台任务

| 键 | 默认 | 说明 |
|---|---|---|
| `queue.concurrency` | `8` | 同时执行的后台任务数（发卡、邮件、采购、回调等） |
| `queue.poll_interval_ms` | `500` | 多久检查一次新任务（毫秒） |
| `queue.upstream_sync_interval` | `5m` | 上游商品库存/价格同步间隔（后台“上游同步”设置优先） |

## upload：上传

| 键 | 默认 | 说明 |
|---|---|---|
| `upload.dir` | `uploads` | 上传文件保存目录（相对运行目录） |
| `upload.max_size` | `10485760` | 单个文件最大字节数（10 MB） |
| `upload.allowed_types` | jpeg / png / gif / webp | 允许的 MIME 类型 |
| `upload.allowed_extensions` | `.jpg .jpeg .png .gif .webp` | 允许的扩展名 |
| `upload.max_width` / `max_height` | `4096` | 图片最大宽高（像素） |

## cors：跨域

| 键 | 默认 | 说明 |
|---|---|---|
| `cors.allowed_origins` | `["*"]` | 允许哪些网页来源调用接口。前后台和接口同域名部署时可以写你的域名，例如 `["https://shop.example.com"]` |
| `cors.allow_credentials` | `true` | 是否允许携带凭据 |
| `cors.max_age` | `600` | 预检请求缓存秒数 |

## security：登录安全

| 键 | 默认 | 说明 |
|---|---|---|
| `security.login_rate_limit.window_seconds` | `300` | 统计窗口（秒） |
| `security.login_rate_limit.max_attempts` | `5` | 窗口内最多失败次数 |
| `security.login_rate_limit.block_seconds` | `900` | 超过后锁定多久（秒） |
| `security.password_policy.min_length` | `8` | 密码最短长度 |
| `security.password_policy.require_upper` / `require_lower` / `require_number` / `require_special` | true / true / true / false | 密码必须包含大写 / 小写 / 数字 / 特殊字符 |

## email：邮件初始值

SMTP 的**初始值**。后台 **系统设置 → 邮件配置 (SMTP)** 保存后以后台为准。

| 键 | 默认 | 说明 |
|---|---|---|
| `email.enabled` | `false` | 是否启用邮件 |
| `email.host` / `port` | 空 / `465` | SMTP 服务器和端口 |
| `email.username` / `password` | 空 | SMTP 账号 |
| `email.from` / `from_name` | 空 | 发件地址和发件人名称 |
| `email.use_ssl` / `use_tls` | `true` / `false` | 465 端口一般用 SSL，587 端口用 TLS（STARTTLS） |
| `email.verify_code.expire_minutes` | `10` | 验证码有效期 |
| `email.verify_code.send_interval_seconds` | `60` | 重发间隔 |
| `email.verify_code.max_attempts` | `5` | 最多输错次数 |
| `email.verify_code.length` | `6` | 验证码位数 |

## order：订单

| 键 | 默认 | 说明 |
|---|---|---|
| `order.payment_expire_minutes` | `15` | 未付款订单多久自动取消（后台“基础配置”里的设置优先） |
| `order.max_refund_days` | `30` | 付款后多少天内允许退款 |

## reseller：分站 / 分销

| 键 | 默认 | 说明 |
|---|---|---|
| `reseller.enabled` | `false` | 是否开启分销功能 |
| `reseller.main_hosts` | `[localhost, 127.0.0.1, "::1"]` | **主站**的域名列表。用这些域名访问时是主站，其他已登记的域名是分站。**记得加上你的主域名** |
| `reseller.subdomain_base` | 空 | 分站子域名的基础域名，例如 `example.com`，分销商就能用 `xxx.example.com` |
| `reseller.self_apply_enabled` | `true` | 是否允许用户在前台自助申请成为分销商 |
| `reseller.settlement_confirm_days` | `7` | 分站利润的结算确认天数（0–3650），过了这么多天才能提现 |
| `reseller.trusted_forwarded_host` | `false` | 是否信任反向代理传来的 `X-Forwarded-Host`。只有反向代理会改写 Host 时才需要打开 |

## web：网页（保留项）

| 键 | 默认 | 说明 |
|---|---|---|
| `web.admin_path` | `/admin` | 保留项（与原项目一致）。当前版本后台的访问路径由打包参数 `--base` 和 Web 服务器配置决定 |
| `web.storefront_dir` / `web.admin_dir` | 空 | 保留项，当前版本由 Web 服务器直接提供网页 |

## integration：站点对接

| 键 | 默认 | 说明 |
|---|---|---|
| `integration.allow_private_addresses` | `false` | 是否允许对接请求访问内网 / 本机地址。**只在可信的内网测试环境打开**，打开后会有服务端请求伪造（SSRF）风险 |
| `integration.acg_faka_compat` | `true` | 是否提供异次元发卡“共享店铺”接口 `/shared/*`。用户没有生成兼容密钥之前接口不会生效 |
| `integration.mcy_compat` | `true` | 是否提供萌次元 OpenApi 接口 `/plugin/open-api/*` |

## 启动时的校验

下面任何一条不满足，程序都会拒绝启动，并在日志里说明原因：

- 三个密钥为空、少于 16 位、还是示例值，或者有两个相同；
- `trusted_proxies` 里有格式错误的地址，或者写了 `/0`；
- `database.url` 不是 `sqlite`、`mysql`、`postgres` 开头。
