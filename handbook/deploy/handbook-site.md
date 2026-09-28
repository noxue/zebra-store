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

## 上线

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
