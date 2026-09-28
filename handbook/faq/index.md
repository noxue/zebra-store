# 常见问题

## 安装与启动

### 启动失败，日志提示 secret / 密钥

`app.secret_key`、`jwt.secret`、`user_jwt.secret` 三项必须都设置、每个至少 16 个字符、互不相同，并且不能是示例里的
`change-me-…`。用 `openssl rand -hex 24` 生成三次分别填入。

### 忘记管理员密码

在服务器上执行（路径换成你的）：

```bash
/opt/zebra/zebra-store --config /opt/zebra/config.yml admin reset-password --username admin
```

按提示输入新密码。管理员的两步验证丢了用 `admin reset-2fa --username admin`。
查看所有管理员：`admin list-admins`。Docker 部署时在命令前加 `docker compose exec zebra`，
并把程序路径换成 `zebra-store --config /app/config.yml`。

### 改了配置文件里的管理员密码，登录还是旧密码

`bootstrap.default_admin_password` 只在**第一次**创建管理员时使用。之后请在后台 **安全设置** 修改，或用上面的命令重置。

### 需要安装 Redis 吗

不需要。缓存、限流和任务队列都内置了。

### 能用 MySQL / PostgreSQL 吗

可以，只改 `database.url`，见 [切换数据库](/deploy/database)。注意换库不会自动搬数据。

## 使用

### 买家付了款，订单还是待支付

见 [排错指南](./troubleshooting#买家付款后订单没有变成已支付)。

### 卡密导入了，前台还是售罄

确认导入到了正确的**规格**；商品已上架；所在分类已启用。

### 前台改了站点名或 Logo 没变化

按 Ctrl+F5 强制刷新浏览器。

### 不想让游客购买

编辑商品，把购买类型改为 **仅会员**。

### 怎么先收钱再发货（人工发货）

商品发货方式选 **人工发货**，设置人工库存（`-1` 为无限），需要买家填写信息时加上 **下单表单**。
买家付款后在 **订单列表** 点 **发货**。

### 可以只用余额支付吗

可以：**用户管理 → 钱包配置 → 仅余额支付**。

### 没有支付渠道，怎么测试下单

后台 **用户详情 → 钱包 → 调整余额** 给测试账号加钱，前台下单时勾选 **使用余额**。

## 对接与分站

### 对接的商品一直“发货中”

看 **对接管理 → 采购单管理** 里这一单的状态，按 [采购单与故障处理](/integration/procurement) 处理。

### 分站打不开

检查：分销商状态是否为启用、子域名是否已分配、DNS 是否解析、Web 服务器是否包含了这个子域名并申请了证书、
`reseller.enabled` 是否为 `true`、主域名是否在 `reseller.main_hosts` 里。

### 分站的颜色能单独设置吗

不能，主题颜色跟随主站。站点名、Logo、公告、SEO 可以单独设置。
