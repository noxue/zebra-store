import type { ExtraMessages } from '../index'

// Zebra Store additions for the users group (strings not present in the original admin i18n).
const messages: ExtraMessages = {
  'zh-CN': { admin: { users: { form: { emailInvalid: '邮箱格式不正确' } }, userDetail: { back: '返回用户列表' } } },
  'zh-TW': { admin: { users: { form: { emailInvalid: '信箱格式不正確' } }, userDetail: { back: '返回使用者列表' } } },
  'en-US': { admin: { users: { form: { emailInvalid: 'Invalid email address' } }, userDetail: { back: 'Back to users' } } },
}

export default messages
