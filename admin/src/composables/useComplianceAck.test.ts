import { describe, expect, it } from 'vitest'
import { COMPLIANCE_EXPECTED, segmentState, toAcknowledgePayload, useComplianceAck } from './useComplianceAck'

describe('compliance acknowledgement', () => {
  it('tracks per-segment state', () => {
    expect(segmentState('', '知悉')).toBe('neutral')
    expect(segmentState('知', '知悉')).toBe('neutral')
    expect(segmentState('知悉', '知悉')).toBe('valid')
    expect(segmentState('错', '知悉')).toBe('invalid')
  })

  it('merges UI boxes 3+4 into backend segment3', () => {
    expect(toAcknowledgePayload(COMPLIANCE_EXPECTED)).toEqual({
      segment1: '我已阅读并理解上述合规声明提醒',
      segment2: '知悉相关法律风险',
      segment3: '并确认自行承担部署运营和收费行为产生的法律责任',
    })
  })

  it('blocks paste-like jumps outside IME composition', () => {
    const ack = useComplianceAck()
    expect(ack.accept(0, '我')).toBe(true)
    expect(ack.accept(0, '我已阅读')).toBe(false)
    ack.composing.value[0] = true
    expect(ack.accept(0, '我已阅读')).toBe(true)
  })

  it('is valid only when all 4 boxes match', () => {
    const ack = useComplianceAck()
    ack.segments.value = [...COMPLIANCE_EXPECTED]
    expect(ack.allValid.value).toBe(true)
    ack.segments.value = [...COMPLIANCE_EXPECTED.slice(0, 3), '']
    expect(ack.allValid.value).toBe(false)
  })
})
