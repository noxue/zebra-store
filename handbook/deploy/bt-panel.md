# 宝塔面板部署

::: tip 推荐先用单文件版本
现在的 GitHub Release 已把用户前台和管理后台嵌入 `zebra-store`。新部署请优先按照
[单文件二进制部署](./binary) 执行一键安装，然后在宝塔中把整个域名反向代理到
`http://127.0.0.1:8081`。下面的步骤保留给需要自己编译、分别管理静态文件的用户。
:::

## 适合谁

服务器上已经装了（或者准备装）[宝塔 Linux 面板](https://www.bt.cn)，希望尽量用鼠标完成部署的新手。
全程只有少量命令需要在宝塔的“终端”里粘贴执行。

最终效果：

- `https://shop.example.com/` → 用户前台
- `https://shop.example.com/admin/` → 管理后台
- 后端程序由宝塔的「进程守护管理器」守护，崩溃或重启服务器后自动拉起。

## 准备

| 项目 | 要求 |
|---|---|
| 服务器 | 64 位 Linux（CentOS 7+/Debian/Ubuntu 均可），1 核 1 GB 内存起步 |
| 域名 | 例如 `shop.example.com`，已添加 A 记录指向服务器 IP |
| 宝塔面板 | 7.x 或更新版本，已安装 **Nginx**（在软件商店安装，版本任意） |
| 文件 | `zebra-store` 程序、`storefront/dist`、`admin/dist`、`config.example.yml`（见下方“获取程序和网页”） |

<!--@include: ./parts/build.md-->

## 步骤

### 1. 安装宝塔和 Nginx

如果还没装宝塔，按 [宝塔官网](https://www.bt.cn/new/download.html) 的一键安装命令安装，装完用浏览器登录面板。
第一次登录时，宝塔会推荐安装 LNMP / LAMP 套件：**只勾选 Nginx** 即可（数据库默认用 SQLite，不需要 MySQL；想用 MySQL 再勾选）。

> 宝塔不同版本的软件商店布局会变化，请以当前面板中的“软件商店 → Nginx”为准。此步骤的截图需要独立宝塔测试环境，采集状态见源码中的 `handbook/SCREENSHOTS.md`。

### 2. 放开防火墙端口

宝塔左侧 **安全** → 确认 **80** 和 **443** 端口已放行。
**不要**放行 8081：后端只给本机的 Nginx 访问。如果云服务商有“安全组”，也要放行 80 和 443。

### 3. 创建网站

宝塔左侧 **网站** → **添加站点**：

- 域名：`shop.example.com`
- 根目录：保持默认 `/www/wwwroot/shop.example.com`
- FTP：不创建；数据库：不创建；PHP 版本：**纯静态**

> “添加站点”表单在不同宝塔版本中字段顺序可能不同；只要按上面的值填写即可。截图采集状态见源码中的 `handbook/SCREENSHOTS.md`。

### 4. 上传前台和后台网页

宝塔左侧 **文件** → 进入 `/www/wwwroot/shop.example.com`：

1. 删除宝塔自动生成的 `index.html`、`404.html`（保留 `.user.ini` 等隐藏文件即可）；
2. 把 `storefront/dist` **里面的所有文件**上传到这个目录（上传后这里应该直接有 `index.html` 和 `assets/`）；
3. 在这个目录里新建文件夹 `admin`，把 `admin/dist` **里面的所有文件**上传进去（上传后应有 `admin/index.html`）。

::: tip 上传压缩包更快
先在本机把 `dist` 打成 zip，上传后在宝塔文件管理器里右键 **解压**。
:::

上传完成后请直接核对目录：站点根目录必须有 `index.html` 与 `assets/`，并且 `admin/` 下也必须有独立的 `index.html` 与 `assets/`。截图采集状态见源码中的 `handbook/SCREENSHOTS.md`。

### 5. 上传后端程序

在 **文件** 里新建目录 `/www/zebra`，上传：

- `zebra-store`（后端程序）
- `config.example.yml`，上传后重命名为 `config.yml`

然后打开宝塔 **终端**（左侧菜单），粘贴执行：

```bash
cd /www/zebra
mkdir -p data uploads
chmod +x zebra-store
chown -R www:www /www/zebra
chmod 600 config.yml
```

`www` 是宝塔运行网站用的用户，后端也用它运行，这样它能读写 `data/` 和 `uploads/`。

### 6. 修改配置文件

在宝塔文件管理器里双击 `/www/zebra/config.yml` 编辑。

<!--@include: ./parts/config-min.md-->

宝塔部署时 `config.yml` 放在 `/www/zebra`，下面守护进程的“运行目录”也设成 `/www/zebra`，
所以数据库文件是 `/www/zebra/data/zebra.db`，图片在 `/www/zebra/uploads/`。

### 7. 用「进程守护管理器」运行后端

1. 宝塔 **软件商店** → 搜索 **进程守护管理器**（Supervisor）→ 安装；
2. 打开它 → **添加守护进程**：

| 字段 | 填写 |
|---|---|
| 名称 | `zebra-store` |
| 启动用户 | `www` |
| 运行目录 | `/www/zebra` |
| 启动命令 | `/www/zebra/zebra-store --config /www/zebra/config.yml serve` |
| 进程数量 | `1`（必须是 1） |

3. 保存后状态应为“运行中”。点 **日志** 能看到启动日志，出现 `listening` 和 `0.0.0.0:8081` 或 `127.0.0.1:8081` 就成功了。

<!-- 截图待补（见 SCREENSHOTS.md）：![进程守护管理器添加 zebra-store](/screenshots/deploy/bt-supervisor.png) -->

在终端里再确认一次：

```bash
curl -s http://127.0.0.1:8081/api/v1/public/config | head -c 120
```

能看到以 `{"status_code":0` 开头的内容就说明后端正常。

::: details 不想装进程守护管理器？用 systemd
在终端里创建 `/etc/systemd/system/zebra-store.service`：

```ini
[Unit]
Description=Zebra Store
After=network-online.target
Wants=network-online.target

[Service]
User=www
Group=www
WorkingDirectory=/www/zebra
ExecStart=/www/zebra/zebra-store --config /www/zebra/config.yml serve
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

然后执行 `systemctl daemon-reload && systemctl enable --now zebra-store`。两种方式**只能选一种**，否则会抢同一个端口。
:::

### 8. 配置 Nginx：转发接口 + 单页应用

<!--@include: ./parts/proxy-paths.md-->

宝塔自带的“反向代理”页面一次只能转发一个目录，而且会改写一些默认设置，所以最稳妥的做法是**直接改站点配置文件**：

**网站** → 点 `shop.example.com` 的 **设置** → **配置文件**，找到 `server { … }` 里
`#REWRITE-END` 这一行，在它的**下面**粘贴：

```nginx
    # ---------- Zebra Store ----------
    client_max_body_size 20m;

    location ~ ^/(api|uploads|shared|plugin/open-api)/ {
        proxy_pass http://127.0.0.1:8081;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 120s;
    }
    location = /sitemap.xml { proxy_pass http://127.0.0.1:8081; proxy_set_header Host $host; }
    location = /robots.txt  { proxy_pass http://127.0.0.1:8081; proxy_set_header Host $host; }

    # 管理后台（单页应用）
    location = /admin { return 308 /admin/; }
    location /admin/ {
        try_files $uri $uri/ /admin/index.html;
    }
    # 用户前台（单页应用）
    location / {
        try_files $uri $uri/ /index.html;
    }
    # ---------- Zebra Store end ----------
```

保存。如果宝塔提示 `location /` 重复，说明站点的 **伪静态** 里已经有 `location /`：到 **伪静态** 标签页把内容清空保存，再回来保存配置文件。

::: warning 不要开启宝塔的“缓存”
如果之前用宝塔的「反向代理」页面加过 `/api`，请删除那条反向代理，改用上面的配置。
接口响应一旦被缓存，买家会看到别人的数据或过期的订单状态。
:::

<!-- 截图待补（见 SCREENSHOTS.md）：![宝塔站点配置文件](/screenshots/deploy/bt-nginx-conf.png) -->

### 9. 申请 SSL 证书

**网站** → `shop.example.com` 的 **设置** → **SSL** → **Let's Encrypt** → 勾选域名 → **申请**。
申请成功后打开右上角的 **强制 HTTPS**。宝塔会自动续期。

<!-- 截图待补（见 SCREENSHOTS.md）：![宝塔申请 Let's Encrypt 证书](/screenshots/deploy/bt-ssl.png) -->

<!--@include: ./parts/real-ip.md-->

宝塔的 Nginx 和后端在同一台机器上，保持默认即可。

### 10. 数据库选择

- **SQLite（默认，推荐新手）**：什么都不用做，数据就在 `/www/zebra/data/zebra.db`。
- **MySQL**：宝塔左侧 **数据库** → **添加数据库**：数据库名 `zebra`、用户名 `zebra`、设置密码、访问权限选 **本地服务器**，
  字符集选 **utf8mb4**。然后把 `config.yml` 改成：

  ```yaml
  database:
    url: mysql://zebra:你的数据库密码@127.0.0.1:3306/zebra
  ```

  在进程守护管理器里 **重启** `zebra-store`，表会自动创建。密码里有 `@`、`:`、`/`、`#` 等符号时要先做 URL 编码
  （例如 `@` 写成 `%40`），或者干脆换一个只含字母数字的密码。更多说明见 [切换数据库](./database)。

::: warning 切换数据库不会搬数据
从 SQLite 换成 MySQL 后是一个**全新的空库**，原来的数据不会自动迁移。请在开张之前决定好。
:::

<!--@include: ./parts/verify.md-->

## 升级

1. 先按下面的方法**备份**；
2. 在本机编译新版本，得到新的 `zebra-store`、`storefront/dist`、`admin/dist`；
3. 进程守护管理器里 **停止** `zebra-store`；
4. 用新文件覆盖 `/www/zebra/zebra-store`（覆盖后在终端执行 `chmod +x /www/zebra/zebra-store`）；
5. 覆盖网站目录里的前台文件、`admin/` 里的后台文件（旧的 `assets/` 可以先删除再上传）；
6. **启动** `zebra-store`。新版本启动时会自动补齐新表和新字段，业务数据保留。

## 备份

- 宝塔 **计划任务** → 添加 **备份目录**：目录选 `/www/zebra`（包含数据库、图片和配置），周期每天；
- 用 MySQL 时再加一条 **备份数据库** 任务；
- 建议同时勾选上传到对象存储或另一台机器。

<!--@include: ./parts/backup-short.md-->

## 常见问题

**打开网站是宝塔的默认页 / 404**
网站根目录里还留着宝塔生成的 `index.html`，或者 `storefront/dist` 是整个文件夹传上去的（变成了 `dist/index.html`）。
根目录下应该**直接**是 `index.html` 和 `assets/`。

**打开 `/admin/` 一片空白**
后台打包时没有加 `--base=/admin/`，或者后台文件没放在 `admin/` 子目录里。按 F12 看控制台，如果 JS 文件 404 就是这个原因。

**页面能打开但一直加载、提示网络错误**
后端没有运行或 `/api/` 没有转发。在终端执行 `curl http://127.0.0.1:8081/api/v1/public/config` 检查后端；
再检查配置文件里的 `location ~ ^/(api|…` 是否粘贴在了 `server { }` 里面。

**进程守护管理器显示已停止，日志提示 secret**
三个密钥没改或者相同，按日志提示修改 `config.yml` 后重启。

**日志提示 Permission denied / unable to open database file**
`/www/zebra` 的所有者不是 `www`，重新执行 `chown -R www:www /www/zebra`。

**上传图片失败**
检查 `client_max_body_size 20m;` 是否加上；再检查 `/www/zebra/uploads` 是否属于 `www`。
