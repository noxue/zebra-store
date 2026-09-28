import { computed, ref } from 'vue'

// UI splits the commitment into 4 boxes; boxes 3 and 4 are joined into backend segment3.
export const COMPLIANCE_EXPECTED = [
  '我已阅读并理解上述合规声明提醒',
  '知悉相关法律风险',
  '并确认自行承担部署',
  '运营和收费行为产生的法律责任',
] as const

export const COMPLIANCE_SEPARATORS = ['，', '，', '、', ''] as const

export type SegmentState = 'neutral' | 'valid' | 'invalid'

export function segmentState(value: string, expected: string): SegmentState {
  if (!value) return 'neutral'
  if (value === expected) return 'valid'
  return expected.startsWith(value) ? 'neutral' : 'invalid'
}

/** Build the acknowledge payload: segment3 = box3 + box4. */
export function toAcknowledgePayload(segments: readonly string[]) {
  return { segment1: segments[0] ?? '', segment2: segments[1] ?? '', segment3: `${segments[2] ?? ''}${segments[3] ?? ''}` }
}

export function useComplianceAck() {
  const segments = ref<string[]>(['', '', '', ''])
  const composing = ref<boolean[]>([false, false, false, false])
  const states = computed(() => segments.value.map((v, i) => segmentState(v, COMPLIANCE_EXPECTED[i] ?? '')))
  const allValid = computed(() => states.value.every((s) => s === 'valid'))
  const reset = () => {
    segments.value = ['', '', '', '']
    composing.value = [false, false, false, false]
  }
  /** Accept a new value unless it grew by >1 char outside IME composition (paste guard). Returns false when blocked. */
  const accept = (idx: number, next: string): boolean => {
    const prev = segments.value[idx] ?? ''
    if (!composing.value[idx] && next.length - prev.length > 1) return false
    const copy = [...segments.value]
    copy[idx] = next
    segments.value = copy
    return true
  }
  return { segments, composing, states, allValid, reset, accept }
}
