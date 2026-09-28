# 部署本手册（可选）

这份手册本身是用 [VitePress](https://vitepress.dev) 写的静态网站，源码在仓库的 `handbook/` 目录。
你也可以把它部署成自己的文档站，比如给你的客服或分销商看。

## 本地预览

```bash
cd handbook
npm install
npm run docs:dev          # http://localhost:5190，修改 Markdown 会实时刷新
```

## 构建

```bash
npm run docs:build        # 输出到 handbook/.vitepress/dist
npm run docs:preview      # 预览构建结果
```

构建时会检查所有站内链接，有坏链接会直接报错。

## 推荐：原子发布

VitePress 的 JS 和 CSS 文件名包含构建哈希。不要使用 `rsync --delete` 直接覆盖 Web 服务器正在读取的目录：旧文件被删除后，浏览器中缓存的旧 HTML 会请求已经不存在的哈希文件，页面可能短暂白屏。

仓库提供了原子发布脚本：

```bash
DOCS_URL=https://docs.example.com \
  ./scripts/deploy-handbook.sh root@你的服务器
```

脚本会自动完成构建，并执行以下步骤：

1. 将完整站点上传到新的版本目录；
2. 保留旧版本的哈希资源，兼容浏览器缓存；
3. 校验 `index.html`、`404.html` 和资源目录；
4. 原子切换 `/opt/zebra-store/docs-current` 软链接；
5. 保留最近三个完整版本，以便快速回滚；
6. 检查线上首页和部署页面。

Web 服务器始终读取软链接，不会看到上传到一半的目录。Caddy 示例：

```text
docs.example.com {
	root * /opt/zebra-store/docs-current
	encode zstd gzip
	try_files {path} {path}.html {path}/ /404.html
	file_server
	header /assets/* Cache-Control "public, max-age=31536000, immutable"
}
```

服务器上的目标根目录可以通过 `DOCS_DEPLOY_ROOT` 修改。需要定期检查源站时，可以安装仓库中 `deploy/systemd/zebra-docs-healthcheck.*` 的 service 和 timer；它会验证首页及入口 JS，失败时重启 Caddy。

## 手动上线

`.vitepress/dist` 是纯静态文件，放到任何 Web 服务器即可。Caddy 示例：

```text
docs.example.com {
	root * /var/www/zebra-docs
	file_server
	try_files {path} {path}.html {path}/ =404
}
```

Nginx 示例：

```nginx
server {
    listen 443 ssl;
    server_name docs.example.com;
    root /var/www/zebra-docs;
    location / { try_files $uri $uri.html $uri/ =404; }
}
```

## 截图

应用界面的截图已经放在 `handbook/public/screenshots/`，构建时会直接写入站点。需要独立宝塔测试环境的图片及采集要求列在 `handbook/SCREENSHOTS.md`；正文在这些图片完成前使用可核对的文字步骤，不会显示损坏图片。
