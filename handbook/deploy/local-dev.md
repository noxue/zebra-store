# 本地开发运行

这一页教你在自己的电脑（Windows / macOS / Linux）上把三个项目都跑起来。

## 1. 准备工具

- **Rust**（≥ 1.90）：打开 [rustup.rs](https://rustup.rs) 按提示安装，装完执行 `rustc --version` 确认。
- **Node.js**（≥ 20）：从 [nodejs.org](https://nodejs.org) 下载 LTS 版本，装完执行 `node -v`。
- **Git**：用来拉代码。

## 2. 拉取代码

```bash
git clone <仓库地址> zebra-store
cd zebra-store
```

## 3. 启动后端

```bash
cd backend
cp config.example.yml config.yml
```

用文本编辑器打开 `config.yml`，**必须修改这四项**：

```yaml
app:
  secret_key: 这里填一串至少16位的随机字符A
jwt:
  secret: 这里填另一串至少16位的随机字符B
user_jwt:
  secret: 这里再填一串至少16位的随机字符C
bootstrap:
  default_admin_password: 你的管理员密码
```

::: warning 三个密钥必须不同
`app.secret_key`、`jwt.secret`、`user_jwt.secret` 必须是**三个不同**的值，每个至少 16 个字符，
也不能保留示例里的 `change-me-…`。否则服务器会拒绝启动并告诉你是哪一项不合格。
可以用 `openssl rand -hex 24` 生成随机字符串。
:::

然后启动：

```bash
cargo run -p zs-server
```

第一次编译需要几分钟。看到日志里出现监听 `0.0.0.0:8081` 就说明启动成功了。
程序会自动在 `backend/data/zebra.db` 创建 SQLite 数据库、建好所有表、创建 6 个内置角色和超级管理员。

## 4. 启动用户前台

新开一个终端：

```bash
cd storefront
npm install
VITE_API_TARGET=http://localhost:8081 npm run dev
```

浏览器打开 <http://localhost:5185>。

## 5. 启动管理后台

再开一个终端：

```bash
cd admin
npm install
VITE_API_TARGET=http://localhost:8081 npm run dev
```

浏览器打开 <http://localhost:5186>，用 `admin` 和你刚才设置的密码登录。

::: tip Windows 用户
Windows 的 cmd 不支持 `VAR=值 命令` 的写法。可以先执行 `set VITE_API_TARGET=http://localhost:8081`，
再执行 `npm run dev`；PowerShell 里用 `$env:VITE_API_TARGET="http://localhost:8081"`。
:::

## 6. 灌入演示数据（可选）

仓库里有演示数据脚本，会创建分类、商品、卡密等：

```bash
cd backend
# 参数依次是：后端地址、管理员用户名、管理员密码；只在空数据库上运行一次
python3 scripts/seed_demo.py http://localhost:8081 admin 你的管理员密码
```

## 常见问题

- **端口被占用**：修改 `config.yml` 的 `server.port`，同时把 `VITE_API_TARGET` 改成新端口。
- **前台打开一片空白**：先确认后端在运行，并打开 <http://localhost:8081/api/v1/public/config>，能看到 JSON 就说明后端正常。
- **忘记管理员密码**：见 [常见问题](/faq/#忘记管理员密码)。
