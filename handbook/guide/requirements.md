# 环境要求

## 只想运行（生产部署）

| 项目 | 最低 | 推荐 |
|---|---|---|
| CPU / 内存 | 1 核 / 512 MB（只跑斑马小铺） | 2 核 / 2 GB |
| 系统 | 任意 64 位 Linux（x86_64 或 arm64） | Debian 12 / Ubuntu 22.04+ |
| 磁盘 | 1 GB + 上传的图片 | 10 GB 以上 |
| 数据库 | 不需要额外安装（默认 SQLite） | 商品和订单很多时用 PostgreSQL 14+ 或 MySQL 8+ |
| Redis | 不需要 | 不需要 |
| 域名 | 一个解析到服务器的域名 | 开分站时需要泛解析 `*.你的域名` |
| HTTPS | 强烈建议 | Caddy 自动申请证书 |

用 [Docker Compose](/deploy/docker) 部署时，服务器上构建镜像需要约 2 GB 内存（编译 Rust），运行时和上表相同。

## 想自己编译或开发

| 工具 | 版本 | 用途 |
|---|---|---|
| Rust | ≥ 1.90 | 编译后端 |
| Node.js | ≥ 20 | 编译前台和后台网页 |
| Git | 任意 | 拉取代码 |

::: tip 小白提示
只想用、不想改代码的话，**不需要安装 Rust 和 Node.js**：拿到编译好的 `zebra-store` 程序和两个 `dist` 网页文件夹，
按 [Caddy](/deploy/caddy)、[Nginx](/deploy/nginx) 或 [宝塔面板](/deploy/bt-panel) 部署即可。
:::

## 浏览器

前台和后台支持最新版 Chrome、Edge、Firefox、Safari，以及手机浏览器。
