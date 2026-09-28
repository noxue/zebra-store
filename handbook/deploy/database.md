# 切换数据库

斑马小铺支持三种数据库，**只看 `database.url` 的开头**来决定用哪一种，不需要改代码、不需要重新编译：

| 数据库 | `database.url` 示例 | 适合 |
|---|---|---|
| SQLite（默认） | `sqlite://data/zebra.db?mode=rwc` | 新手、小店；一个文件，不用安装任何东西 |
| MySQL 8+ | `mysql://zebra:密码@127.0.0.1:3306/zebra` | 已经有 MySQL（比如宝塔）的服务器 |
| PostgreSQL 14+ | `postgres://zebra:密码@127.0.0.1:5432/zebra` | 订单量大、需要更稳的并发 |

也可以不改文件，用环境变量：`ZS__DATABASE__URL=postgres://…`（见 [环境变量覆盖](./env)）。

## SQLite 说明

- `sqlite://data/zebra.db` 是**相对路径**，相对于程序的运行目录（systemd 的 `WorkingDirectory`）；
- 想写绝对路径，就在 `sqlite://` 后面再加一个 `/`：`sqlite:///opt/zebra/data/zebra.db?mode=rwc`；
- `?mode=rwc` 表示“文件不存在就创建”，**不要删掉**；
- 数据库文件所在目录必须允许运行用户写入（SQLite 还会在旁边创建 `-wal`、`-shm` 临时文件）。

## 换成 MySQL

1. 创建一个**空**数据库和用户（字符集 `utf8mb4`）：

   ```sql
   CREATE DATABASE zebra CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
   CREATE USER 'zebra'@'127.0.0.1' IDENTIFIED BY '强密码';
   GRANT ALL PRIVILEGES ON zebra.* TO 'zebra'@'127.0.0.1';
   FLUSH PRIVILEGES;
   ```

   宝塔用户直接在 **数据库 → 添加数据库** 里建即可。

2. 修改配置：

   ```yaml
   database:
     url: mysql://zebra:强密码@127.0.0.1:3306/zebra
     max_connections: 10
   ```

3. 重启后端。启动时会自动建表，并执行 MySQL 专用的数据迁移（例如把时间列改成毫秒精度）。

## 换成 PostgreSQL

1. 创建空数据库：

   ```bash
   sudo -u postgres psql -c "CREATE USER zebra WITH PASSWORD '强密码';"
   sudo -u postgres psql -c "CREATE DATABASE zebra OWNER zebra;"
   ```

2. 修改配置：

   ```yaml
   database:
     url: postgres://zebra:强密码@127.0.0.1:5432/zebra
   ```

3. 重启后端。

## 需要注意

::: warning 切换数据库不会搬家
改了 `database.url` 之后，程序连接的是**另一个数据库**。新库是空的：程序会重新建表、重新创建内置角色和超级管理员，
**原来的商品、订单、用户都不会自动带过去**。请在正式开张前选好数据库。
:::

- 数据库本身必须**事先创建好**，程序只负责建表；
- 数据库用户需要建表、加列、建索引的权限（DDL 权限）；
- 密码里有 `@ : / ? # %` 等特殊字符时要做 URL 编码，例如 `p@ss` 写成 `p%40ss`；
- 表结构以程序为准：每次启动都会检查并**补齐**缺少的表和字段，不会删除已有数据。

## 连接池参数

| 配置项 | 默认 | 说明 |
|---|---|---|
| `database.max_connections` | 10 | 最多同时占用的数据库连接数 |
| `database.min_connections` | 1 | 空闲时保留的连接数 |
| `database.connect_timeout_seconds` | 10 | 连接超时（秒） |
| `database.idle_timeout_seconds` | 600 | 空闲连接多久后关闭（秒） |
| `database.sql_log` | false | 在日志里打印每一条 SQL，排查问题时临时打开 |
