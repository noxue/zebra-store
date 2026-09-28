---
layout: home

hero:
  name: 斑马小铺
  text: Zebra Store 使用手册
  tagline: 一个会自动发卡的二次元小商店。这份手册从零开始教你部署、上架、收款、对接货源、开分站。
  image:
    src: /logo.svg
    alt: 斑马小铺
  actions:
    - theme: brand
      text: 五分钟了解
      link: /guide/
    - theme: alt
      text: 开始部署
      link: /deploy/
    - theme: alt
      text: 对接货源
      link: /integration/
    - theme: alt
      text: GitHub 源码
      link: https://github.com/noxue/zebra-store

features:
  - title: 自动发卡
    details: 买家付款后系统自动把卡密发到订单里，半夜也不用守着。也支持人工发货和自定义下单表单。
    link: /admin/card-secrets
    linkText: 卡密怎么导入
  - title: 多种收款方式
    details: 易支付、支付宝、微信、PayPal、Stripe、USDT 等网关，外加站内钱包余额支付。
    link: /payment/
    linkText: 配置支付渠道
  - title: 站点之间对接
    details: 可以从别的发卡站进货，也可以给别人供货。支持 dujiao-next、Zebra Store、异次元发卡、萌次元四种协议。
    link: /integration/
    linkText: 什么是上游和下游
  - title: 分站 / 分销
    details: 让别人用子域名开一个自己的分站，自己设置加价，系统自动记账、结算、提现。
    link: /reseller/
    linkText: 分站怎么玩
  - title: 一个数据库配置搞定
    details: 默认 SQLite，零依赖直接跑；想换 MySQL 或 PostgreSQL，只需要改一行配置。
    link: /deploy/database
    linkText: 切换数据库
  - title: 二次元外观
    details: 樱花粉、梦幻紫配色，亮暗双主题，看板娘和樱花飘落都可以在后台自定义或关闭。
    link: /admin/theme
    linkText: 自定义主题
---
