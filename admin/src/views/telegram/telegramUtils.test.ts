import { describe, expect, it } from 'vitest'
import {
  broadcastStatusTone,
  canDeleteBroadcast,
  fileNameFromPath,
  formatBroadcastStatus,
  formatLicenseStatus,
  formatWarnings,
  formatWebhookStatus,
  licenseStatusTone,
  moveItem,
  parseTelegramBotSettings,
  toTelegramHtml,
} from './telegramUtils'
import { buildChannelClientUpdate, parseChannelClient } from './useChannelClients'

const t = (key: string) => key

describe('parseTelegramBotSettings', () => {
  it('uses the original defaults for missing data', () => {
    const f = parseTelegramBotSettings(undefined)
    expect(f.enabled).toBe(false)
    expect(f.default_locale).toBe('zh-CN')
    expect(f.help.enabled).toBe(true)
    expect(f.menu.items).toEqual([])
  })

  it('parses nested sections, localized text and items', () => {
    const f = parseTelegramBotSettings({
      enabled: true,
      default_locale: 'en-US',
      config_version: 3,
      basic: { display_name: 'Bot', description: { 'zh-CN': '简介', fr: 'x' }, support_url: 'https://t.me/s', cover_url: '/c.png' },
      welcome: { enabled: true, message: { 'en-US': 'hi' } },
      help: { enabled: false, title: {}, items: [{ key: 'shop', order: 2, show_support_link: true, summary: { 'zh-CN': 's' } }, 'bad'] },
      menu: { items: [{ key: 'm', action: { type: 'web_app', value: 'https://x' } }, { action: { type: 'weird' } }] },
    })
    expect(f.enabled).toBe(true)
    expect(f.default_locale).toBe('en-US')
    expect(f.basic.description).toEqual({ 'zh-CN': '简介', 'zh-TW': '', 'en-US': '' })
    expect(f.welcome.message['en-US']).toBe('hi')
    expect(f.help.enabled).toBe(false)
    expect(f.help.items).toHaveLength(2)
    expect(f.help.items[0]).toMatchObject({ key: 'shop', order: 2, show_support_link: true, enabled: true })
    expect(f.help.items[1]?.key).toBe('')
    expect(f.menu.items[0]?.action).toEqual({ type: 'web_app', value: 'https://x' })
    expect(f.menu.items[1]?.action.type).toBe('builtin')
    expect('config_version' in f).toBe(false)
  })

  it('falls back to zh-CN for an unknown default locale', () => {
    expect(parseTelegramBotSettings({ default_locale: 'ja-JP' }).default_locale).toBe('zh-CN')
  })
})

describe('moveItem', () => {
  it('swaps neighbours and ignores out-of-range moves', () => {
    const a = [1, 2, 3]
    expect(moveItem(a, 0, 'down')).toBe(true)
    expect(a).toEqual([2, 1, 3])
    expect(moveItem(a, 0, 'up')).toBe(false)
    expect(moveItem(a, 2, 'down')).toBe(false)
    expect(moveItem(a, 2, 'up')).toBe(true)
    expect(a).toEqual([2, 3, 1])
  })
})

describe('status labels', () => {
  it('maps webhook status', () => {
    expect(formatWebhookStatus(t, '')).toBe('-')
    expect(formatWebhookStatus(t, ' OK ')).toBe('telegramBot.status.webhookStatusActive')
    expect(formatWebhookStatus(t, 'disconnected')).toBe('telegramBot.status.webhookStatusInactive')
    expect(formatWebhookStatus(t, 'pending')).toBe('pending')
  })
  it('maps license status + tone', () => {
    expect(formatLicenseStatus(t, undefined)).toBe('telegramBot.status.licenseStatusUnknown')
    expect(formatLicenseStatus(t, 'Expired')).toBe('telegramBot.status.licenseStatusExpired')
    expect(formatLicenseStatus(t, 'other')).toBe('other')
    expect(licenseStatusTone('active')).toBe('success')
    expect(licenseStatusTone('revoked')).toBe('danger')
    expect(licenseStatusTone('')).toBe('neutral')
  })
  it('joins warnings', () => {
    expect(formatWarnings(t, [])).toBe('telegramBot.status.licenseWarningsEmpty')
    expect(formatWarnings(t, ['license_lease_expired', 'x'])).toBe('telegramBot.status.warningLeaseExpired / x')
  })
})

describe('broadcast helpers', () => {
  it('status label, tone and delete rule', () => {
    expect(formatBroadcastStatus(t, 'COMPLETED')).toBe('telegramBot.broadcasts.statusCompleted')
    expect(formatBroadcastStatus(t, 'queued')).toBe('telegramBot.broadcasts.statusPending')
    expect(broadcastStatusTone('failed')).toBe('danger')
    expect(canDeleteBroadcast('completed')).toBe(true)
    expect(canDeleteBroadcast('failed')).toBe(true)
    expect(canDeleteBroadcast('running')).toBe(false)
    expect(canDeleteBroadcast('pending')).toBe(false)
  })
  it('extracts file names', () => {
    expect(fileNameFromPath('/uploads/telegram/2026/a.png')).toBe('a.png')
    expect(fileNameFromPath('https://x.com/f/b.pdf?x=1')).toBe('b.pdf')
  })
})

describe('toTelegramHtml', () => {
  it('returns empty for empty editor content', () => {
    expect(toTelegramHtml('')).toBe('')
    expect(toTelegramHtml('<p></p>')).toBe('')
  })
  it('converts paragraphs and keeps supported inline tags', () => {
    expect(toTelegramHtml('<p>Hello <strong>world</strong></p><p><em>a</em> <u>b</u> <s>c</s></p>')).toBe('Hello <b>world</b>\n<i>a</i> <u>b</u> <s>c</s>')
  })
  it('turns br into newlines, headings into bold, lists into bullets', () => {
    expect(toTelegramHtml('<h2>Title</h2><p>a<br>b</p><ul><li><p>x</p></li><li><p>y</p></li></ul><ol><li>one</li></ol>')).toBe(
      '<b>Title</b>\na\nb\n• x\n• y\n1. one',
    )
  })
  it('keeps safe links, drops unsafe ones, images and unknown tags', () => {
    expect(toTelegramHtml('<p><a href="https://e.com?a=1&b=2" target="_blank">go</a> <a href="javascript:alert(1)">bad</a><img src="x.png"><span style="color:red">red</span></p>')).toBe(
      '<a href="https://e.com?a=1&amp;b=2">go</a> badred',
    )
  })
  it('escapes text and strips scripts', () => {
    expect(toTelegramHtml('<p>1 &lt; 2 &amp; 3</p><script>alert(1)</script>')).toBe('1 &lt; 2 &amp; 3')
  })
  it('UPL-02: strips onerror, javascript: hrefs and style attributes from rendered HTML', () => {
    const out = toTelegramHtml('<p><img src=x onerror=alert(1)><a href="javascript:alert(1)">x</a><a style="color:red" href="https://e.com">y</a></p>')
    expect(out).not.toMatch(/onerror|javascript:|style=/i)
    expect(out).toBe('x<a href="https://e.com">y</a>')
  })
  it('keeps code blocks and quotes', () => {
    expect(toTelegramHtml('<pre><code>x = 1</code></pre><blockquote><p>q</p></blockquote>')).toBe('<pre><code>x = 1</code></pre>\n<blockquote>q</blockquote>')
  })
})

describe('channel clients', () => {
  it('parses a raw client', () => {
    const c = parseChannelClient({ id: 3, name: 'n', channel_key: 'k', bot_token_set: true, status: 1, last_used_at: null })
    expect(c).toMatchObject({ id: 3, name: 'n', channel_key: 'k', bot_token_set: true, status: 1, callback_url: '', last_used_at: null })
  })
  it('only sends bot_token when provided', () => {
    expect(buildChannelClientUpdate({ name: 'a', description: '', bot_token: '', callback_url: 'u' })).toEqual({ name: 'a', description: '', callback_url: 'u' })
    expect(buildChannelClientUpdate({ name: 'a', description: 'd', bot_token: 'tok', callback_url: '' })).toEqual({ name: 'a', description: 'd', callback_url: '', bot_token: 'tok' })
  })
})
