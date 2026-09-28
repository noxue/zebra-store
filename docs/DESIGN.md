# 二次元视觉设计规范（storefront 与 admin 共用的设计语言）

两个前端各自实现一份组件库，但必须遵守同一套 token 与风格，保证观感统一。
布局与信息结构按原项目截图 `docs/reference/screenshots/` 还原，视觉换成本规范。

## 关键词
樱花粉 · 梦幻紫 · 天空蓝 · 玻璃拟态 · 柔光描边 · 圆润 · 星光点缀 · 轻微弹性动效

## 颜色 token（CSS 变量，定义在 `src/styles/theme.css`）

| token | 亮色 | 暗色 | 用途 |
|---|---|---|---|
| `--zs-primary` | `#ff5fa2` | `#ff7ab6` | 主色（樱花粉），主按钮、选中态 |
| `--zs-primary-soft` | `#ffe3f0` | `rgba(255,122,182,.16)` | 主色浅底 |
| `--zs-secondary` | `#8b5cf6` | `#a78bfa` | 次色（梦幻紫） |
| `--zs-accent` | `#38bdf8` | `#5ccfff` | 强调（天空蓝），链接、信息 |
| `--zs-gold` | `#fbbf24` | `#fcd34d` | 价格高亮 / 星星 |
| `--zs-success` | `#34d399` | `#4ade80` | |
| `--zs-warning` | `#fb923c` | `#fdba74` | |
| `--zs-danger` | `#f43f5e` | `#fb7185` | |
| `--zs-bg` | `#fff6fb` | `#0e0b1f` | 页面背景 |
| `--zs-bg-pattern` | 粉紫柔和径向渐变 + 细点阵 | 深空渐变 + 星点 | `body` 背景 |
| `--zs-surface` | `rgba(255,255,255,.72)` | `rgba(30,24,60,.62)` | 玻璃卡片底 |
| `--zs-surface-solid` | `#ffffff` | `#1a1535` | 弹窗/下拉 |
| `--zs-border` | `rgba(255,95,162,.18)` | `rgba(167,139,250,.22)` | 描边 |
| `--zs-text` | `#2d2445` | `#efe9ff` | 正文 |
| `--zs-text-muted` | `#7a6f93` | `#a9a0c8` | 次要文字 |
| `--zs-gradient` | `linear-gradient(135deg,#ff5fa2,#8b5cf6 55%,#38bdf8)` | 同左提亮 | 标题渐变字、主按钮、进度条 |

**主色可由后台配置覆盖**：`/public/config` 的 `theme.primary_color / secondary_color / accent_color`
存在时在运行时写入 `:root` 覆盖对应变量（不存在时用上表默认值）。

## 形状与质感
- 圆角：`--zs-radius-sm 10px`、`--zs-radius 16px`、`--zs-radius-lg 24px`、胶囊 `999px`
- 阴影：`--zs-shadow` 柔和粉紫投影 `0 8px 30px -12px rgba(139,92,246,.35)`；悬浮时加 `--zs-glow` `0 0 0 1px var(--zs-border), 0 10px 40px -10px rgba(255,95,162,.45)`
- 玻璃卡片：`background: var(--zs-surface); backdrop-filter: blur(14px) saturate(140%); border: 1px solid var(--zs-border)`
- 主按钮：渐变底 + 白字 + 胶囊圆角，hover 上浮 2px + 光晕，active 轻微缩放 0.97
- 标题：`font-family: var(--zs-font-display)`，大标题可用渐变文字 `background-clip:text`
- 徽章：胶囊、浅色底 + 同色描边，可带小图标

## 字体
- 显示字体 `--zs-font-display`: `"ZCOOL KuaiLe", "M PLUS Rounded 1c", system-ui, sans-serif`（通过 @fontsource 本地引入，不依赖外网 CDN）
- 正文 `--zs-font-body`: `"M PLUS Rounded 1c", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif`
- 数字/价格 `--zs-font-num`: `"Baloo 2", ui-rounded, system-ui`

## 二次元元素（均可在后台关闭）
- **樱花飘落**：轻量 canvas 粒子（≤ 25 片，`prefers-reduced-motion` 或 `theme.effects.sakura=false` 时关闭）
- **看板娘**：首页 Hero 与登录页右侧的角色立绘，图片来自 `theme.mascot_image`；未配置时使用内置的原创 SVG 角色（不得使用有版权的角色）
- **星光点缀**：标题旁小星星 ✦ 闪烁动画、卡片 hover 时角落闪光
- **波浪分隔线**：区块之间 SVG 波浪
- **空状态插画**：内置原创 SVG（小猫/小狐狸吉祥物），配俏皮文案
- **加载**：跳动的樱花/猫爪 loading
- **Hero 背景**：`theme.background_image` 或内置渐变 + 漂浮光斑

## 后台可配置项（site_config.theme，新后端提供；原后端没有时全部走默认）
```json
{
  "theme": {
    "primary_color": "#ff5fa2",
    "secondary_color": "#8b5cf6",
    "accent_color": "#38bdf8",
    "background_image": "",
    "mascot_image": "",
    "login_background": "",
    "effects": { "sakura": true, "sparkle": true },
    "default_mode": "system"
  }
}
```
站点名 `brand.site_name`、Logo `brand.site_logo`、Favicon `brand.site_icon` 始终来自配置。

## 管理后台差异
- 同一 token，但更克制：侧边栏玻璃质感 + 渐变选中条；表格行 hover 粉色浅底；
  仪表盘 KPI 卡片带渐变图标徽章；登录页使用看板娘 + 樱花背景。
- 数据密度优先：表格、表单保持原项目的字段与列顺序。

## 响应式
- 断点：`sm 640 / md 768 / lg 1024 / xl 1280`
- 前台移动端底部导航（首页/商品/购物车/我的），后台移动端侧边栏变抽屉。

## 可访问性
- 文字与背景对比度 ≥ 4.5:1（渐变按钮上用白字 + 文字阴影保证可读）
- 所有交互元素有 focus-visible 样式（粉色外环）
- 动效尊重 `prefers-reduced-motion`
