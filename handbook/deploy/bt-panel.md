# 宝塔面板部署

宝塔中的 Nginx 只负责 HTTPS 和整站反向代理。GitHub Release 的 `zebra-store` 单文件已经包含用户前台、`/admin/` 管理后台、API 和后台任务，不需要创建静态网站目录，不需要上传 `dist`，也不需要克隆仓库。

```text
浏览器 / Cloudflare → 宝塔 Nginx :443 → zebra-store 127.0.0.1:8081
```

## 准备

- 64 位 Linux，支持 `x86_64` 或 `aarch64`；
- 宝塔面板已经安装 Nginx；
- 域名已经解析到服务器；
- 防火墙和云安全组开放 80、443，不开放 8081。

## 1. 安装 Release 单文件

打开宝塔左侧的 **终端**，切换到 root 后执行：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

脚本只下载当前服务器架构对应的 Release 压缩包和 `SHA256SUMS`，不会下载整个仓库。校验成功后会：

1. 安装 `/usr/local/bin/zebra-store`；
2. 创建 `/opt/zebra-store/config.yml`；
3. 创建 SQLite 数据和上传目录；
4. 创建并启动 `zebra-store` systemd 服务；
5. 在终端显示初始后台账号和随机密码。

请立即保存终端显示的密码。检查服务：

```bash
systemctl status zebra-store --no-pager
curl -fsS http://127.0.0.1:8081/health
```

## 2. 在宝塔添加站点

进入 **网站 → 添加站点**：

- 域名填写 `shop.example.com`；
- PHP 版本选择“纯静态”；
- 数据库选择“不创建”；
- 网站根目录可以保持宝塔默认值，Zebra Store 不会使用这个目录。

## 3. 设置整站反向代理

进入站点的 **设置 → 反向代理 → 添加反向代理**：

| 项目 | 填写内容 |
|---|---|
| 代理名称 | `zebra-store` |
| 目标 URL | `http://127.0.0.1:8081` |
| 发送域名 | `$host` |

保存后，所有路径都应转发给 Zebra Store。不要单独添加 `/api/` 或 `/admin/` 规则，也不要配置前后台静态目录。

如果使用 [Docker Release 单文件部署](./docker)，目标 URL 改成 `http://127.0.0.1:18180`。

需要手动编辑 Nginx 配置时，核心内容只有：

```nginx
client_max_body_size 20m;

location / {
    proxy_pass http://127.0.0.1:8081;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_read_timeout 120s;
}
```

## 4. 开启 HTTPS

进入 **网站 → 设置 → SSL**，申请 Let's Encrypt 证书并开启“强制 HTTPS”。使用 Cloudflare 小黄云时，SSL/TLS 模式建议选择“完全（严格）”。

## 5. 验证

```bash
curl -I https://shop.example.com/
curl -I https://shop.example.com/admin/
curl -fsS https://shop.example.com/health
curl -fsS https://shop.example.com/api/v1/public/config | head -c 200
```

- `https://shop.example.com/` 是用户前台；
- `https://shop.example.com/admin/` 是管理后台；
- `/health` 应返回 `{"status":"ok"}`。

## 后台账号和密码

默认用户名是 `admin`。初始密码会在安装完成时显示，也可以在宝塔终端执行：

```bash
grep -E 'default_admin_(username|password)' /opt/zebra-store/config.yml
```

这只代表数据库首次初始化使用的密码。后台修改密码以后，数据库只保存哈希，不能读取原密码。忘记密码时执行：

```bash
NEW_PASSWORD="Zs9-$(openssl rand -hex 10)"
zebra-store --config /opt/zebra-store/config.yml admin reset-password \
  --username admin --password "$NEW_PASSWORD"
printf '新后台密码：%s\n' "$NEW_PASSWORD"
```

## 升级

再次执行安装命令即可。脚本保留配置、数据库和上传文件，只下载并替换新的 Release 单文件：

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```
