# 前台总览

用户前台就是买家看到的商店。这一部分按功能介绍每个页面怎么用，也适合站长了解买家的购物流程，
方便回答客服问题。

## 页面地图

| 页面 | 地址 | 需要登录 |
|---|---|---|
| 首页 | `/` | 否 |
| 全部商品 / 分类 | `/products`、`/categories/分类别名` | 否 |
| 商品详情 | `/products/商品别名` | 否 |
| 购物车 | `/cart` | 否 |
| 结算 | `/checkout` | 否（游客可买） |
| 支付 | `/pay` | 否 |
| 游客查单 | `/guest/orders` | 否 |
| 博客 / 公告 / 关于 / 条款 / 隐私 | `/blog`、`/notice`、`/about`、`/terms`、`/privacy` | 否 |
| 登录 / 注册 / 找回密码 | `/auth/login`、`/auth/register`、`/auth/forgot` | 否 |
| 个人中心 | `/me`（概览）、`/me/orders`、`/me/wallet`、`/me/gift-cards`、`/me/affiliate`、`/me/api`、`/me/security`、`/me/profile` | 是 |
| 订单详情 | `/orders/订单号` | 是 |
| 分销控制台 | `/reseller` | 是，且需要成为分销商 |

![前台首页](/screenshots/storefront/home.png)

## 一次典型的购物

1. 买家在首页或商品页找到商品，选择规格和数量；
2. 点 **立即购买**（或加入购物车后去结算）；
3. 游客填写邮箱和查询密码，会员直接下单；
4. 选择支付方式付款；
5. 付款成功后自动跳到订单详情，自动发货的商品直接显示卡密。

每一步的细节见左侧菜单对应的页面。

## 电脑和手机

前台是响应式网页。手机上底部有导航栏（首页 / 商品 / 购物车 / 我的），商品详情页往下滑时底部会出现购买按钮。

![手机端首页](/screenshots/storefront/home-mobile.png)
