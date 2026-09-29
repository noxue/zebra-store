import { defineConfig, type DefaultTheme } from 'vitepress'
import { screenshotPlaceholder } from './screenshot-placeholder'

const SITE_TITLE = '斑马小铺 Zebra Store 使用手册'

const sidebar: DefaultTheme.Sidebar = {
  '/guide/': [
    {
      text: '快速开始',
      items: [
        { text: '斑马小铺是什么', link: '/guide/' },
        { text: '三个项目的架构', link: '/guide/architecture' },
        { text: '环境要求', link: '/guide/requirements' },
        { text: '名词解释', link: '/guide/glossary' },
      ],
    },
  ],
  '/deploy/': [
    {
      text: '部署',
      items: [
        { text: '选择部署方式', link: '/deploy/' },
        { text: '单文件二进制（推荐）', link: '/deploy/binary' },
        { text: '宝塔面板部署', link: '/deploy/bt-panel' },
        { text: 'Docker Release 单文件（推荐）', link: '/deploy/docker' },
        { text: 'Nginx 部署', link: '/deploy/nginx' },
        { text: 'Caddy 部署', link: '/deploy/caddy' },
        { text: '本地开发运行', link: '/deploy/local-dev' },
      ],
    },
    {
      text: '部署之后',
      items: [
        { text: '切换数据库', link: '/deploy/database' },
        { text: '配置文件详解', link: '/deploy/config' },
        { text: '环境变量覆盖', link: '/deploy/env' },
        { text: '备份与升级', link: '/deploy/backup-upgrade' },
        { text: '部署本手册（可选）', link: '/deploy/handbook-site' },
      ],
    },
  ],
  '/storefront/': [
    {
      text: '前台使用（买家）',
      items: [
        { text: '前台总览', link: '/storefront/' },
        { text: '浏览、搜索、语言与主题', link: '/storefront/browse' },
        { text: '注册、登录与账户安全', link: '/storefront/account' },
        { text: '购物车与下单', link: '/storefront/checkout' },
        { text: '优惠券、活动价、会员价、批发价', link: '/storefront/discounts' },
        { text: '支付', link: '/storefront/payment' },
        { text: '订单查询与交付', link: '/storefront/orders' },
        { text: '售后与退款', link: '/storefront/after-sales' },
        { text: '钱包与礼品卡', link: '/storefront/wallet' },
        { text: '推广返利', link: '/storefront/affiliate' },
        { text: 'API 对接（个人中心）', link: '/storefront/api' },
      ],
    },
  ],
  '/admin/': [
    {
      text: '入门',
      items: [
        { text: '后台总览与首次登录', link: '/admin/' },
        { text: '仪表盘', link: '/admin/dashboard' },
      ],
    },
    {
      text: '系统',
      items: [
        { text: '站点设置', link: '/admin/settings' },
        { text: '品牌与主题外观', link: '/admin/theme' },
        { text: '权限管理与审计', link: '/admin/rbac' },
        { text: '安全设置（管理员）', link: '/admin/security' },
        { text: '通知中心与邮件', link: '/admin/notifications' },
        { text: 'Telegram Bot', link: '/admin/telegram' },
      ],
    },
    {
      text: '商品与订单',
      items: [
        { text: '分类与商品', link: '/admin/products' },
        { text: '卡密库存、导入、导出', link: '/admin/card-secrets' },
        { text: '订单与退款', link: '/admin/orders' },
        { text: '订单风控', link: '/admin/risk-control' },
        { text: '支付渠道与支付记录', link: '/admin/payments' },
      ],
    },
    {
      text: '用户与营销',
      items: [
        { text: '用户与登录日志', link: '/admin/users' },
        { text: '钱包管理与配置', link: '/admin/wallet' },
        { text: '会员等级', link: '/admin/member-levels' },
        { text: '优惠券、活动价、批发价、礼品卡', link: '/admin/marketing' },
        { text: '文章、Banner、素材', link: '/admin/content' },
        { text: '推广返利', link: '/admin/affiliate' },
      ],
    },
    {
      text: '对接与分销',
      items: [
        { text: '对接管理', link: '/admin/integration' },
        { text: '分销商管理', link: '/admin/resellers' },
      ],
    },
  ],
  '/integration/': [
    {
      text: '站点对接',
      items: [
        { text: '概念：上游与下游', link: '/integration/' },
        { text: 'Zebra Store 协议', link: '/integration/zebra-store' },
        { text: '异次元发卡 acg-faka', link: '/integration/acg-faka' },
        { text: '萌次元 mcy-shop', link: '/integration/mcy' },
        { text: '采购单与故障处理', link: '/integration/procurement' },
      ],
    },
  ],
  '/reseller/': [
    {
      text: '分站（分销商）',
      items: [
        { text: '完整生命周期', link: '/reseller/' },
        { text: '加价与利润计算', link: '/reseller/pricing' },
        { text: '结算与提现', link: '/reseller/settlement' },
      ],
    },
  ],
  '/payment/': [
    {
      text: '支付渠道配置',
      items: [
        { text: '通用字段与回调地址', link: '/payment/' },
        { text: '易支付 epay', link: '/payment/epay' },
        { text: '支付宝官方', link: '/payment/alipay' },
        { text: '微信支付官方', link: '/payment/wechat' },
        { text: '汇付（支付宝 / 微信）', link: '/payment/huifu' },
        { text: 'PayPal', link: '/payment/paypal' },
        { text: 'Stripe', link: '/payment/stripe' },
        { text: '加密货币网关', link: '/payment/crypto' },
        { text: 'DujiaoPay', link: '/payment/dujiaopay' },
      ],
    },
  ],
  '/faq/': [
    {
      text: '常见问题',
      items: [
        { text: '常见问题', link: '/faq/' },
        { text: '排错指南', link: '/faq/troubleshooting' },
      ],
    },
  ],
}

export default defineConfig({
  lang: 'zh-CN',
  title: SITE_TITLE,
  titleTemplate: ':title | 斑马小铺手册',
  description: '斑马小铺（Zebra Store）发卡商城的部署、使用、对接与分站手册，写给第一次接触的新手。',
  cleanUrls: true,
  // include-only fragments and repo notes are not pages
  srcExclude: ['**/parts/**', 'SCREENSHOTS.md', 'README.md'],
  // local dev URLs in the tutorials are intentional
  ignoreDeadLinks: 'localhostLinks',
  lastUpdated: false,
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/logo.svg' }],
    ['meta', { name: 'theme-color', content: '#ff5fa2' }],
  ],
  markdown: {
    lineNumbers: false,
    config: (md) => {
      md.use(screenshotPlaceholder)
    },
  },
  themeConfig: {
    logo: '/logo.svg',
    siteTitle: SITE_TITLE,
    nav: [
      { text: '快速开始', link: '/guide/', activeMatch: '^/guide/' },
      { text: '部署', link: '/deploy/', activeMatch: '^/deploy/' },
      {
        text: '使用',
        items: [
          { text: '前台（买家）', link: '/storefront/' },
          { text: '管理后台', link: '/admin/' },
          { text: '支付渠道配置', link: '/payment/' },
        ],
      },
      { text: '对接', link: '/integration/', activeMatch: '^/integration/' },
      { text: '分站', link: '/reseller/', activeMatch: '^/reseller/' },
      { text: '常见问题', link: '/faq/', activeMatch: '^/faq/' },
      { text: 'GitHub 仓库', link: 'https://github.com/noxue/zebra-store' },
    ],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/noxue/zebra-store' },
    ],
    sidebar,
    outline: { level: [2, 3], label: '本页目录' },
    search: {
      provider: 'local',
      options: {
        translations: {
          button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
          modal: {
            displayDetails: '显示详细列表',
            resetButtonTitle: '清除查询',
            backButtonTitle: '关闭搜索',
            noResultsText: '没有找到相关结果',
            footer: { selectText: '选择', navigateText: '切换', closeText: '关闭' },
          },
        },
      },
    },
    docFooter: { prev: '上一页', next: '下一页' },
    darkModeSwitchLabel: '外观',
    lightModeSwitchTitle: '切换到浅色模式',
    darkModeSwitchTitle: '切换到深色模式',
    sidebarMenuLabel: '菜单',
    returnToTopLabel: '回到顶部',
    langMenuLabel: '语言',
    notFound: {
      title: '页面走丢了',
      quote: '这一页可能已经改名或者被移走了，试试左上角的搜索吧。',
      linkLabel: '回到首页',
      linkText: '回到首页',
    },
    footer: {
      message: '斑马小铺 Zebra Store：Rust + Vue 3 数字商品交易系统',
      copyright: '<a href="https://github.com/noxue/zebra-store">github.com/noxue/zebra-store</a>',
    },
  },
})
