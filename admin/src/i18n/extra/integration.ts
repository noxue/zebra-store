import type { ExtraMessages } from '../index'

// Zebra Store additions for the integration views (site connections, mappings, procurement, reconciliation, API credentials).
const messages: ExtraMessages = {
  'zh-CN': {
    procurement: {
      status: {
        manual_review: '待人工核对',
      },
      manualReviewHint: '供货方可能已扣款：不会自动退款。重试 = 用同一请求号重新提交（或重新查单）；取消 = 标记失败并回退本地订单。',
    },
    siteConnections: {
      "protocolHint": "选择上游站点所用的对接协议，不同协议需要填写的字段与支持的能力不同。",
      "protocolsLoading": "正在加载对接协议…",
      "protocolsLoadFailed": "对接协议列表加载失败，暂时按通用字段（地址 / Key / Secret）填写。",
      "retry": "重试",
      "protocolUnknown": "未注册的协议（{id}）",
      "recommended": "推荐",
      "credentialsSection": "连接凭证",
      "secretKeepPlaceholder": "留空则保持不变",
      "connectionCode": {
        "title": "一键接入：粘贴连接码",
        "hint": "在上游站点「个人中心 → API 对接」点击「生成连接码」，复制后粘贴到这里，会自动填写地址、Key、Secret 并测试连接。",
        "placeholder": "在此粘贴连接码",
        "parse": "解析",
        "parsing": "正在解析连接码…",
        "orManual": "或手动填写下方字段"
      },
      "wizard": {
        "test": "测试连接",
        "testing": "握手中…",
        "missingTitle": "还差几项信息",
        "missingDetail": "请先填写：{fields}",
        "fieldSep": "、",
        "handshakeFailed": "连接失败",
        "handshakeFailedHint": "请检查地址、Key、Secret 是否正确，以及上游站点能否从本站访问。",
        "redirectedHint": "上游把 API 请求重定向到了网页。请在上游反向代理或防 CC/WAF 中放行 API 路径，关闭这些路径的浏览器验证后再试。",
        "invalidCode": "连接码无法识别",
        "invalidCodeHint": "请确认复制了完整的连接码（没有被截断），或请上游重新生成一个。",
        "successTitle": "握手成功，已连上「{name}」",
        "site": "站点",
        "currency": "结算货币",
        "balance": "账户余额",
        "version": "协议版本",
        "capabilities": "支持的能力",
        "noCapabilities": "仅基础能力",
        "suggestedRate": "建议汇率",
        "suggestedRateHint": "上游价格 × 汇率 = 本站基础价格",
        "suggestedCallback": "建议回调地址",
        "apply": "应用",
        "applied": "已应用",
        "editSecretHint": "编辑时 Secret 留空表示不修改；测试连接需要重新填写 Secret。"
      },
      "features": {
        "changes": "增量同步",
        "categories": "分类同步",
        "webhooks": "实时推送",
        "quote": "报价锁价",
        "multiItem": "多商品下单",
        "idempotency": "幂等下单",
        "encryptedDelivery": "加密交付"
      },
      "columns": {
        "supplierCurrency": "结算货币",
        "syncMode": "同步方式",
        "webhook": "推送"
      },
      "syncMode": {
        "incremental": "增量",
        "full": "全量"
      },
      "webhookStatus": {
        "active": "已注册",
        "registered": "已注册",
        "ok": "正常",
        "pending": "待注册",
        "failed": "注册失败",
        "error": "异常",
        "none": "未启用",
        "disabled": "未启用"
      }
    },
    admin: {
      zebraIntegration: {
        invalidUrl: '请输入有效的 URL（需包含 http:// 或 https://）',
        from: '开始',
        to: '结束',
      },
    },
  },
  'zh-TW': {
    procurement: {
      status: {
        manual_review: '待人工核對',
      },
      manualReviewHint: '供貨方可能已扣款：不會自動退款。重試 = 用同一請求號重新提交（或重新查單）；取消 = 標記失敗並回退本地訂單。',
    },
    siteConnections: {
      "protocolHint": "選擇上游站點所用的對接協議，不同協議需要填寫的欄位與支援的能力不同。",
      "protocolsLoading": "正在載入對接協議…",
      "protocolsLoadFailed": "對接協議列表載入失敗，暫時按通用欄位（地址 / Key / Secret）填寫。",
      "retry": "重試",
      "protocolUnknown": "未註冊的協議（{id}）",
      "recommended": "推薦",
      "credentialsSection": "連線憑證",
      "secretKeepPlaceholder": "留空則保持不變",
      "connectionCode": {
        "title": "一鍵接入：貼上連線碼",
        "hint": "在上游站點「個人中心 → API 對接」點擊「產生連線碼」，複製後貼到這裡，會自動填寫地址、Key、Secret 並測試連線。",
        "placeholder": "在此貼上連線碼",
        "parse": "解析",
        "parsing": "正在解析連線碼…",
        "orManual": "或手動填寫下方欄位"
      },
      "wizard": {
        "test": "測試連線",
        "testing": "握手中…",
        "missingTitle": "還差幾項資訊",
        "missingDetail": "請先填寫：{fields}",
        "fieldSep": "、",
        "handshakeFailed": "連線失敗",
        "handshakeFailedHint": "請檢查地址、Key、Secret 是否正確，以及上游站點能否從本站存取。",
        "redirectedHint": "上游把 API 請求重新導向到網頁。請在上游反向代理或防 CC/WAF 中放行 API 路徑，關閉這些路徑的瀏覽器驗證後再試。",
        "invalidCode": "連線碼無法識別",
        "invalidCodeHint": "請確認複製了完整的連線碼（沒有被截斷），或請上游重新產生一個。",
        "successTitle": "握手成功，已連上「{name}」",
        "site": "站點",
        "currency": "結算貨幣",
        "balance": "帳戶餘額",
        "version": "協議版本",
        "capabilities": "支援的能力",
        "noCapabilities": "僅基礎能力",
        "suggestedRate": "建議匯率",
        "suggestedRateHint": "上游價格 × 匯率 = 本站基礎價格",
        "suggestedCallback": "建議回呼地址",
        "apply": "套用",
        "applied": "已套用",
        "editSecretHint": "編輯時 Secret 留空表示不修改；測試連線需要重新填寫 Secret。"
      },
      "features": {
        "changes": "增量同步",
        "categories": "分類同步",
        "webhooks": "即時推送",
        "quote": "報價鎖價",
        "multiItem": "多商品下單",
        "idempotency": "冪等下單",
        "encryptedDelivery": "加密交付"
      },
      "columns": {
        "supplierCurrency": "結算貨幣",
        "syncMode": "同步方式",
        "webhook": "推送"
      },
      "syncMode": {
        "incremental": "增量",
        "full": "全量"
      },
      "webhookStatus": {
        "active": "已註冊",
        "registered": "已註冊",
        "ok": "正常",
        "pending": "待註冊",
        "failed": "註冊失敗",
        "error": "異常",
        "none": "未啟用",
        "disabled": "未啟用"
      }
    },
    admin: {
      zebraIntegration: {
        invalidUrl: '請輸入有效的 URL（需包含 http:// 或 https://）',
        from: '開始',
        to: '結束',
      },
    },
  },
  'en-US': {
    procurement: {
      status: {
        manual_review: 'Manual review',
      },
      manualReviewHint: 'The supplier may have charged us: no automatic refund. Retry resubmits with the same request number (or re-checks the order); cancel marks it failed and rolls the local order back.',
    },
    siteConnections: {
      "protocolHint": "Pick the integration protocol the upstream site speaks. Each protocol has its own fields and capabilities.",
      "protocolsLoading": "Loading protocols…",
      "protocolsLoadFailed": "Could not load the protocol list — falling back to the generic fields (URL / Key / Secret).",
      "retry": "Retry",
      "protocolUnknown": "Unregistered protocol ({id})",
      "recommended": "Recommended",
      "credentialsSection": "Credentials",
      "secretKeepPlaceholder": "Leave blank to keep the current secret",
      "connectionCode": {
        "title": "One-click setup: paste a connection code",
        "hint": "On the upstream site open Account → API and click \"Generate connection code\". Paste it here and the URL, key and secret are filled in and tested automatically.",
        "placeholder": "Paste the connection code here",
        "parse": "Parse",
        "parsing": "Reading the connection code…",
        "orManual": "or fill in the fields below manually"
      },
      "wizard": {
        "test": "Test connection",
        "testing": "Handshaking…",
        "missingTitle": "A few details are missing",
        "missingDetail": "Please fill in: {fields}",
        "fieldSep": ", ",
        "handshakeFailed": "Connection failed",
        "handshakeFailedHint": "Check the URL, key and secret, and make sure the upstream site is reachable from this server.",
        "redirectedHint": "The upstream redirected the API request to a web page. Allow the API paths through its reverse proxy or WAF without a browser challenge, then try again.",
        "invalidCode": "Connection code not recognised",
        "invalidCodeHint": "Make sure the whole code was copied (not truncated), or ask the upstream to generate a new one.",
        "successTitle": "Connected to \"{name}\"",
        "site": "Site",
        "currency": "Settlement currency",
        "balance": "Account balance",
        "version": "Protocol version",
        "capabilities": "Capabilities",
        "noCapabilities": "Basic features only",
        "suggestedRate": "Suggested exchange rate",
        "suggestedRateHint": "Upstream price × rate = local base price",
        "suggestedCallback": "Suggested callback URL",
        "apply": "Apply",
        "applied": "Applied",
        "editSecretHint": "When editing, leave the secret blank to keep it; testing the connection needs the secret re-entered."
      },
      "features": {
        "changes": "Incremental sync",
        "categories": "Category sync",
        "webhooks": "Real-time push",
        "quote": "Price-locked quotes",
        "multiItem": "Multi-item orders",
        "idempotency": "Idempotent orders",
        "encryptedDelivery": "Encrypted delivery"
      },
      "columns": {
        "supplierCurrency": "Currency",
        "syncMode": "Sync mode",
        "webhook": "Push"
      },
      "syncMode": {
        "incremental": "Incremental",
        "full": "Full"
      },
      "webhookStatus": {
        "active": "Registered",
        "registered": "Registered",
        "ok": "OK",
        "pending": "Pending",
        "failed": "Failed",
        "error": "Error",
        "none": "Off",
        "disabled": "Off"
      }
    },
    admin: {
      zebraIntegration: {
        invalidUrl: 'Please enter a valid URL (including http:// or https://)',
        from: 'From',
        to: 'To',
      },
    },
  },
}

export default messages
