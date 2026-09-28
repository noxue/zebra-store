// Live QA 2026-09-26 I-15: the upload failure toast must carry the backend reason (QA-A15).
import { describe, expect, it } from 'vitest'
import { uploadFailureMessage } from './useMedia'

const t = (key: string, params?: Record<string, unknown>) => `${key}${params ? JSON.stringify(params) : ''}`

describe('media upload failure message', () => {
  it('QA-A15 lists the file and backend message instead of the failure count', () => {
    const msg = uploadFailureMessage(t, [{ name: 'evil.php', message: 'File extension not allowed: .php' }], 1)
    expect(msg).toBe('admin.media.errors.uploadFailed{"message":"evil.php: File extension not allowed: .php"}')
    expect(msg).not.toContain('"message":"1"')
  })

  it('QA-A15 partial failure keeps the count summary and adds the reasons', () => {
    const msg = uploadFailureMessage(t, [{ name: 'a.svg', message: 'bad' }], 3)
    expect(msg).toBe('admin.media.errors.uploadPartialFailed{"fail":1,"total":3} a.svg: bad')
  })
})
