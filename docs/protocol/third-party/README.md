# 第三方发卡/卡网系统对接调研

目标：找出有**站点间对接协议**（对接 / 货源 / 上游 / 共享店铺 / 分站进货 / API 进货）的开源发卡系统，
为每个系统做两个方向的实现：

- (a) **采购方适配器**：本站把对方当上游进货（`SupplierAdapter`，见 `docs/BACKEND_GUIDE.md` §9）；
- (b) **供货方兼容接口**：对方站点把本站当上游接入。

调研日期：2026-09-25。源码 `git clone --depth 1` 到会话 scratchpad 的 `thirdparty/` 目录后阅读；
star 数与最后推送时间取自 GitHub API（同一天）。

## 结论速览

- 值得做适配器的只有已经在做或已完成的三个：**dujiao-next**（已实现）、**异次元 acg-faka** 与
  **萌次元 mcy-shop**（由另一个 agent 负责，见 `acg-faka.md` / `mcy-shop.md`）。
- 这次检查的其余开源系统**都没有站点间对接协议**：既没有供货方 API，也没有对接别家的采购方代码。
  它们都是单店铺系统，所以本目录里只有这份调研报告，没有对应的协议规格。
- 彩虹（代刷 / 云商城）和鲸发卡在圈内对接生态中确实常用，但它们是**闭源商业授权软件**，
  公开能拿到的只有"破解 / 开心版"泄露包，不算开源来源，本项目不据此实现（原因见下）。

## 调研表

| 系统 | 仓库 | Star / 最后推送 | 语言 | 供货方 API？ | 采购方对接？ | 结论 |
|---|---|---|---|---|---|---|
| Dujiao-Next | [dujiao-next/dujiao-next](https://github.com/dujiao-next/dujiao-next) | 1318 / 2026-09-17 | Go | 有（`/api/v1/upstream/*`） | 有 | **已实现**（`dujiao-next` 适配器与供货方接口） |
| 异次元发卡 acg-faka | [lizhipay/acg-faka](https://github.com/lizhipay/acg-faka) | 5693 / 2026-09-21 | PHP | 有（共享店铺） | 有 | 见 `acg-faka.md`（另一个 agent） |
| 萌次元 mcy-shop | [lizhipay/mcy-shop](https://github.com/lizhipay/mcy-shop) | 492 / 2026-09-21 | PHP（GitHub 显示为 JS） | 有 | 有 | 见 `mcy-shop.md`（另一个 agent） |
| 独角数卡 dujiaoka | [assimon/dujiaoka](https://github.com/assimon/dujiaoka) | 12137 / 2026-03-12，**已归档** | PHP (Laravel) | 无 | 无 | **跳过**：没有对接 API。唯一的外呼是按商品配置的 `api_hook`（见下），后继项目 dujiao-next 已支持 |
| dujiaoka 社区分支 | [hiouttime/dujiaoka](https://github.com/hiouttime/dujiaoka)、[ZyphrZero/dujiaoka-mcq](https://github.com/ZyphrZero/dujiaoka-mcq)、[hiouttime/dujiaoka-modules](https://github.com/hiouttime/dujiaoka-modules)、[iLay1678/dujiaoka_mod](https://github.com/iLay1678/dujiaoka_mod) | 350 / 2025-09；88 / 2023-11；23 / 2023-03；124 / 2021-05 | PHP | 无 | 无 | **跳过**：查了路由和源码，没有对接或货源相关代码，插件仓库里只有支付和主题 |
| kamiFaka | [Baiyuetribe/kamiFaka](https://github.com/Baiyuetribe/kamiFaka) | 2317 / 2023-12（代码停在 2022-09） | Python (Flask) + Vue3 | 无 | 无 | **跳过**：只有前台 `/api/v2` 和管理 `/api/v4`，没有对接接口，已停更 |
| card-system（卡密商城） | [Tai7sy/card-system](https://github.com/Tai7sy/card-system) | 3031 / 2026-07-30 | PHP (Laravel，控制器混淆) | 无 | 无 | **跳过**：路由里只有 admin/shop 两组，没有对接；README 里的"商业版接口"是作者自营的支付网关 |
| ZFAKA | 原仓库 zlkbdotnet/zfaka 与 zfaka-plus/zfaka 均已 404；镜像 [qibinghua/faka](https://github.com/qibinghua/faka) | 9 / 2021-02 | PHP (Yaf) | 无 | 无 | **跳过**：模块只有 Admin/Crontab/Install/Member/Product，没有 API 模块；上游仓库已消失 |
| V发卡 vfkphp | [szvone/vfkphp](https://github.com/szvone/vfkphp) | 273 / 2020-05 | PHP (TP5.1) | 无 | 无 | **跳过**：只有 index/admin 两个控制器，只对接 V 免签支付；已停更 |
| 红盟云卡 hmyk | [w-hehe/hmyk](https://github.com/w-hehe/hmyk) | 319 / 2023-06 | PHP (FastAdmin/TP5) | 无 | 无 | **跳过**：`application/api` 只有订单支付状态等接口，开源版没有对接；插件市场是远程收费插件 |
| IDeeY 发卡 | [ideey/faka](https://github.com/ideey/faka) | 23 / 2023-01 | Node (Express + MongoDB) | 无 | 无 | **跳过**：路由都是后台 CRUD 和下单支付，没有对接 |
| BF 商城 bf_shop | [osuuu/bf_shop](https://github.com/osuuu/bf_shop) | 49 / 2020-05 | PHP | 无 | 无 | **跳过**：`includes/api/*` 是后台 AJAX；README 里的"对接"指支付 |
| 荔枝发卡 faka（异次元前身） | [lizhipay/faka](https://github.com/lizhipay/faka) | 427 / 2022-09 | PHP | 无 | 无 | **跳过**：已由 acg-faka 取代，源码里的"对接"只指支付 |
| kamihub | [cd-zwj/kamihub](https://github.com/cd-zwj/kamihub) | 1 / 2026-09-24（只有 1 个提交） | Go | 自有下游 API（Bearer 长期令牌，`POST /api/orders/purchase`） | 只有一个商业上游 `f16688`（16688 开放平台） | **跳过**：它是采购中台，不是发卡站；项目太新、没人用；它的 adapter 接口设计（Ping/ListGoods/GetGoods/Quote/Purchase/QueryOrder）可以参考 |
| 彩虹代刷 / 彩虹云商城 | 官方闭源（授权制）；GitHub 上只有破解包，如 [qq1063823095/Rainbow-brush](https://github.com/qq1063823095/Rainbow-brush)（v4.8，2018） | 7 / 2018-11 | PHP | 有（`api.php?act=…`，`key=apikey`） | 有（"社区对接"：亿乐等卡盟） | **跳过**：闭源商业软件，只能拿到破解泄露包（README 自称"破解仅供参考"，核心文件 `includes/ajax.func.php`、`core.func.php` 仍加密），不是合法开源来源；泄露的也是 2018 年的 v4.8，和现在线上的 v6.x 协议未必一致 |
| 鲸发卡 JingFaka | 官方闭源；[mrlihx/JingFaka_Happy](https://github.com/mrlihx/JingFaka_Happy) 是"v11.61 开心版"（破解） | 86 / 2023-08 | PHP (TP5) | 无跨站对接（只有平台内的商户/代理） | 无 | **跳过**：闭源破解包，而且它是多商户平台，代理只在平台内部，没有站点间协议 |
| 知宇发卡、星海/星辰发卡、如意发卡、企业发卡 | 在 GitHub/Gitee 上搜不到开源仓库（搜索结果都指向上面几个项目或收费源码站） | — | — | — | — | **跳过**：没有可读的开源来源 |

## 关键证据（文件:行号，路径相对于 `thirdparty/<repo>/`）

- **dujiaoka**：`routes/api.php` 只有 Laravel 默认的 `/user`；`routes/common/web.php`、`routes/common/pay.php`
  只有前台下单/查单和支付回调。唯一的外呼是 `app/Jobs/ApiHook.php:58-86`：订单完成后
  （`app/Service/OrderProcessService.php:432`），如果商品配置了 `api_hook` URL，就 `POST` 一段
  JSON `{title, order_sn, email, actual_price, order_info, good_id, gd_name}`。这个请求**没有签名**，
  也**拿不回卡密**，所以它是通知钩子，不是对接协议。
  - 可以考虑的最小兼容：本站提供一个接收 `api_hook` 的端点，让 dujiaoka 站长把"代充"订单转成本站订单。
    但请求没有任何鉴权，而且 dujiaoka 不读取响应、也不能回写发货结果，价值很低，**不建议做**。
- **kamiFaka**：`service/api/user.py:19`（`/api/v2`，只有前台 8 个接口）、`service/api/admin.py:30`
  （`/api/v4`，后台 CRUD）、`service/api/common.py:90`（支付回调）。
- **card-system**：`routes/api.php:2`（一行混淆路由）只有 `admin/*` 和 `shop/*`
  （product/coupon/buy/record）。
- **ZFAKA（qibinghua 镜像）**：`application/modules/{Admin,Crontab,Install,Member,Product}/controllers`，
  不存在 API/对接模块。
- **彩虹 v4.8 破解包**（只用来判断"有没有对接"，没有据此写规格）：`api.php:10-101` 有 `act=clone`（整站克隆）、
  `tools`（商品列表）、`orders`、`change`、`siteinfo`、`token`，鉴权是明文比较 `key == $conf['apikey']`（`api.php:37`、`api.php:51`）。
  这说明彩虹确实有供货方 API，但来源不合法、版本也过时。

## 建议

1. 只给 **acg-faka、mcy-shop**（等另一个 agent 的规格）和已有的 **dujiao-next** 做双向适配。
   这三个就是目前开源发卡圈里有真实对接生态的系统。
2. **彩虹**：如果以后确实需要兼容（它是最大的"社区对接"生态），应该找官方公开的对接文档或拿到正版授权后再做，
   不要照破解包实现。到时候按 `SupplierAdapter` 新增 `caihong` 适配器即可，核心代码不需要改。
3. **kamihub** 的上游错误分类（`transient` / `business` / `fatal`）、"下单可以换货源、等发货阶段绝不换"的两阶段采购，
   和我们的 `is_retryable_error_code`、UPS-11 思路一致，可以作为设计参考，但不需要为它做适配器。
