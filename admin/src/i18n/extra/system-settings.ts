import type { ExtraMessages } from '../index'

// Zebra Store additions for 站点设置 / 通知中心 (system-settings group).
const messages: ExtraMessages = {
  'zh-CN': {
    admin: {
      systemSettings: {
        invalidColor: '颜色格式无效，请输入 #RGB 或 #RRGGBB',
      },
    },
  },
  'zh-TW': {
    admin: {
      systemSettings: {
        invalidColor: '顏色格式無效，請輸入 #RGB 或 #RRGGBB',
      },
    },
  },
  'en-US': {
    admin: {
      systemSettings: {
        invalidColor: 'Invalid colour, use #RGB or #RRGGBB',
      },
    },
  },
}

export default messages
