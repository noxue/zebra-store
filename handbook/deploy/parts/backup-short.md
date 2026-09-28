### 备份

要备份的只有三样：**数据库**、**上传目录** `uploads/`、**配置文件** `config.yml`（里面有加密密钥）。

```bash
# SQLite 在线备份（不用停服务，也不需要安装 sqlite3）
zebra-store --config 程序目录/config.yml backup --output /root/backup/zebra-$(date +%F).db
tar czf /root/backup/uploads-$(date +%F).tgz -C 程序目录 uploads
cp 程序目录/config.yml /root/backup/config-$(date +%F).yml
```

MySQL / PostgreSQL、定时备份和恢复步骤见 [备份与升级](/deploy/backup-upgrade)。
