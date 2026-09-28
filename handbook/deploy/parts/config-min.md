### 写配置文件

从仓库复制 `backend/config.example.yml`，改名为 `config.yml`，至少改下面这些
（每一项的含义见 [配置文件详解](/deploy/config)）：

```yaml
app:
  secret_key: 第一串随机字符          # openssl rand -hex 24 生成
server:
  host: 127.0.0.1                    # 只让本机的反向代理访问后端
  port: 8081
  mode: release
jwt:
  secret: 第二串随机字符
user_jwt:
  secret: 第三串随机字符
bootstrap:
  default_admin_username: admin
  default_admin_password: 你的管理员密码   # 只在第一次启动时用来创建超级管理员
database:
  url: sqlite://data/zebra.db?mode=rwc    # 相对“运行目录”，即 <程序目录>/data/zebra.db
upload:
  dir: uploads                            # 相对“运行目录”
cors:
  allowed_origins: ["https://shop.example.com"]
```

::: warning 三个密钥
`app.secret_key`、`jwt.secret`、`user_jwt.secret` 必须是**三个不同**的值，每个至少 16 个字符，
不能保留示例里的 `change-me-…`，否则程序拒绝启动并在日志里写明是哪一项。
`app.secret_key` 用来加密数据库里的敏感字段（支付密钥等），**上线后不要再改**，改了之前加密的数据就解不开了。
:::
