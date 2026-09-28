# 环境变量覆盖

配置文件里的**每一项**都可以用环境变量覆盖，环境变量的优先级**高于** `config.yml`。
适合 Docker、systemd，或者不想把密码写进文件的情况。

## 命名规则

```text
ZS__<段名>__<键名>
```

- 以 `ZS__` 开头（**两个下划线**）；
- 每一级之间用**两个下划线**分隔；
- 大小写都可以，习惯写成大写。

| 配置文件 | 环境变量 |
|---|---|
| `server.port: 9000` | `ZS__SERVER__PORT=9000` |
| `database.url: postgres://…` | `ZS__DATABASE__URL=postgres://…` |
| `log.level: debug` | `ZS__LOG__LEVEL=debug` |
| `app.secret_key: …` | `ZS__APP__SECRET_KEY=…` |
| `jwt.secret: …` | `ZS__JWT__SECRET=…` |
| `reseller.enabled: true` | `ZS__RESELLER__ENABLED=true` |
| `security.login_rate_limit.max_attempts: 10` | `ZS__SECURITY__LOGIN_RATE_LIMIT__MAX_ATTEMPTS=10` |
| `integration.allow_private_addresses: true` | `ZS__INTEGRATION__ALLOW_PRIVATE_ADDRESSES=true` |
| `cors.allowed_origins: [a, b]` | `ZS__CORS__ALLOWED_ORIGINS=https://a.com,https://b.com`（逗号分隔） |
| `server.trusted_proxies: [a, b]` | `ZS__SERVER__TRUSTED_PROXIES=10.0.0.0/8,172.16.0.0/12`（逗号分隔） |

列表类的配置（`cors.allowed_origins`、`server.trusted_proxies`、`upload.allowed_types`、`upload.allowed_extensions`、
`reseller.main_hosts`）都用逗号分隔多个值。

`true` / `false` 和数字会自动识别类型。

## 在 systemd 里使用

在 `[Service]` 段加 `Environment=`，或者用一个只有 root 能读的文件：

```ini
[Service]
EnvironmentFile=/etc/zebra-store.env
```

`/etc/zebra-store.env`：

```bash
ZS__APP__SECRET_KEY=……
ZS__JWT__SECRET=……
ZS__USER_JWT__SECRET=……
ZS__DATABASE__URL=postgres://zebra:密码@127.0.0.1:5432/zebra
```

```bash
chmod 600 /etc/zebra-store.env && systemctl daemon-reload && systemctl restart zebra-store
```

## 在 Docker Compose 里使用

```yaml
services:
  zebra:
    environment:
      ZS__LOG__LEVEL: debug
      ZS__DATABASE__URL: postgres://zebra:密码@db:5432/zebra
```

## 常见错误

- 只写了一个下划线：`ZS_SERVER_PORT` **不会生效**；
- 环境变量名里的段名拼错不会报错，只是不生效。改完后可以看启动日志或者访问效果来确认。
