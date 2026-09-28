import type { ExtraMessages } from '../index'

// Zebra Store additions for the telegram group (strings not present in the original admin i18n).
const messages: ExtraMessages = {
  'zh-CN': {
    telegramBot: {
      zebra: {
        createdFrom: '创建时间起',
        createdTo: '创建时间止',
        clearSelection: '清空已选',
        messagePlaceholder: '输入要群发的消息内容，可使用加粗、斜体、链接等格式',
        richHint: '富文本会自动转换为 Telegram 支持的 HTML（加粗、斜体、下划线、删除线、链接、代码、引用），图片与表格样式将被移除。',
      },
    },
  },
  'zh-TW': {
    telegramBot: {
      zebra: {
        createdFrom: '建立時間起',
        createdTo: '建立時間止',
        clearSelection: '清空已選',
        messagePlaceholder: '輸入要群發的訊息內容，可使用粗體、斜體、連結等格式',
        richHint: '富文本會自動轉換為 Telegram 支援的 HTML（粗體、斜體、底線、刪除線、連結、程式碼、引用），圖片與表格樣式將被移除。',
      },
    },
  },
  'en-US': {
    telegramBot: {
      zebra: {
        createdFrom: 'Created from',
        createdTo: 'Created to',
        clearSelection: 'Clear selection',
        messagePlaceholder: 'Write the broadcast message; bold, italic, links and more are supported',
        richHint: 'Rich text is converted to Telegram-supported HTML (bold, italic, underline, strikethrough, links, code, quotes); images and table styling are removed.',
      },
    },
  },
}

export default messages
