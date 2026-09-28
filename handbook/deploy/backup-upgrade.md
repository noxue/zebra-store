# 备份与升级

## 要备份什么

| 内容 | 位置 | 为什么重要 |
|---|---|---|
| 数据库 | SQLite：`data/zebra.db`；或者 MySQL / PostgreSQL | 所有商品、卡密、订单、用户、余额 |
| 上传目录 | `uploads/` | 商品图、Logo、Banner |
| 配置文件 | `config.yml`（以及环境变量文件） | **`app.secret_key` 丢了，数据库里加密的支付密钥、对接密钥就再也解不开** |

网页文件（`storefront/`、`admin/`）可以随时重新打包，不用备份。

## SQLite 备份

SQLite 正在写入时直接复制文件可能得到损坏的副本。用程序自带的 `backup` 命令做在线备份（内部是 SQLite 的
`VACUUM INTO`），**不需要停服务，也不需要安装 `sqlite3`**：

```bash
mkdir -p /root/backup
zebra-store --config /opt/zebra/config.yml backup --output /root/backup/zebra-$(date +%F).db
tar czf /root/backup/uploads-$(date +%F).tgz -C /opt/zebra uploads
cp /opt/zebra/config.yml /root/backup/config-$(date +%F).yml
```

- `--output` 指定的文件必须**还不存在**（不会覆盖旧备份）；不写 `--output` 时存到 `data/backup/zebra-<时间>.db`；
- 命令读取 `config.yml` 里的 `database.url`，所以要用和服务相同的 `--config`；
- 装了 `sqlite3` 的话也可以用 `sqlite3 /opt/zebra/data/zebra.db ".backup '/root/backup/zebra.db'"`，
  并用 `sqlite3 备份文件 "PRAGMA integrity_check;"` 检查（输出 `ok` 表示完好）。

### Docker 部署

Docker 镜像里没有 `sqlite3`，数据库在 Docker 卷里。在正在运行的容器里执行 `backup`（容器的工作目录是 `/app`，
`data/` 就是数据卷），再把文件拷出来：

```bash
cd /opt/zebra-store/release && mkdir -p backup
T=$(date +%F-%H%M)
docker compose exec store zebra-store --config /app/config.yml backup --output data/backup/zebra-$T.db
docker compose cp store:/app/data/backup/zebra-$T.db backup/
docker compose exec store rm /app/data/backup/zebra-$T.db
```

## MySQL / PostgreSQL 备份

内置的 `backup` 命令只支持 SQLite，MySQL / PostgreSQL 用数据库自带的工具（同样不用停服务）：

```bash
mysqldump --single-transaction -u zebra -p zebra > /root/backup/zebra-$(date +%F).sql
pg_dump -Fc -U zebra -h 127.0.0.1 zebra > /root/backup/zebra-$(date +%F).dump
```

Docker 里的 PostgreSQL：`docker compose exec db pg_dump -Fc -U zebra zebra > backup/zebra-$(date +%F).dump`。

## 每天自动备份

把下面的脚本保存为 `/root/zebra-backup.sh`，按你的实际路径修改：

```bash
#!/bin/sh
set -e
D=/root/backup; mkdir -p "$D"; T=$(date +%F)
zebra-store --config /opt/zebra/config.yml backup --output "$D/zebra-$T.db"
tar czf "$D/uploads-$T.tgz" -C /opt/zebra uploads
cp /opt/zebra/config.yml "$D/config-$T.yml"
find "$D" -type f -mtime +14 -delete        # 只保留 14 天
```

```bash
chmod +x /root/zebra-backup.sh
(crontab -l 2>/dev/null; echo "30 4 * * * /root/zebra-backup.sh") | crontab -
```

::: tip 异地备份
备份和网站放在同一台服务器上，服务器坏了就一起没了。至少每周把 `/root/backup` 下载一份到别处，
或者用 rclone 同步到对象存储。
:::

宝塔用户可以直接用 **计划任务 → 备份目录 / 备份数据库**，见 [宝塔面板部署](./bt-panel#备份)。

## 恢复

```bash
systemctl stop zebra-store
cp /root/backup/zebra-2026-01-01.db /opt/zebra/data/zebra.db
rm -f /opt/zebra/data/zebra.db-wal /opt/zebra/data/zebra.db-shm
rm -rf /opt/zebra/uploads && tar xzf /root/backup/uploads-2026-01-01.tgz -C /opt/zebra
chown -R zebra:zebra /opt/zebra
systemctl start zebra-store
```

恢复时 `config.yml` 里的 `app.secret_key` 必须和备份时**一致**。

::: warning 把备份恢复到测试机时
恢复出来的数据库带着原站的站点对接配置，程序一启动就会按计划去同步真实的上游（拉库存、查采购单）。
在测试机上恢复时，先在后台 **站点对接** 里停用这些连接（或在启动前改掉 `site_connections` 的地址），再让它长时间运行。
:::

## 升级

Release 单文件已经包含前台和后台，不需要分别替换网页目录。

直接安装的用户重新运行安装脚本即可：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

Docker 用户在 `deploy/release` 目录指定新版本：

```bash
RELEASE=v新版本 docker compose build --no-cache
RELEASE=v新版本 docker compose up -d --no-build
curl -fsS http://127.0.0.1:18180/health
```

这两个方式都会保留原配置、数据库和上传文件。新版本启动时会自动补齐数据库结构。

新版本第一次启动时会自动：

- 补齐新增的表和字段（不会删除已有数据）；
- 执行新的数据迁移（例如新增的内置角色权限）。

::: info 后台的“检测更新”按钮
后台右上角的“检测更新”显示当前版本和最新版本。斑马小铺不支持在网页里一键升级，对话框会提示“当前部署方式不支持一键升级”，按上面的步骤手动升级即可。
:::

## 回滚

新版本有问题时：停止后端 → 换回旧程序和旧网页 → **用升级前的备份恢复数据库** → 启动。
新版本可能已经给数据库加了字段，旧程序一般可以忽略它们，但用备份恢复最稳妥。
