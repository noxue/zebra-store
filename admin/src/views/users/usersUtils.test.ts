import { describe, expect, it } from 'vitest'
import type { AdminMemberLevel } from '@/api/types'
import {
  formatScopeProducts,
  isImagePath,
  memberLevelLabel,
  nextSortState,
  normalizeCurrency,
  sanitizeChannelIds,
  toggleId,
  validateWalletAdjust,
} from './usersUtils'

describe('nextSortState', () => {
  it('cycles none → desc → asc → none', () => {
    const a = nextSortState({ by: '', order: '' }, 'created_at')
    expect(a).toEqual({ by: 'created_at', order: 'desc' })
    const b = nextSortState(a, 'created_at')
    expect(b).toEqual({ by: 'created_at', order: 'asc' })
    expect(nextSortState(b, 'created_at')).toEqual({ by: '', order: '' })
  })
  it('switching column starts at desc', () => {
    expect(nextSortState({ by: 'created_at', order: 'asc' }, 'wallet_balance')).toEqual({ by: 'wallet_balance', order: 'desc' })
  })
})

describe('normalizeCurrency', () => {
  it('uppercases valid codes and falls back to CNY', () => {
    expect(normalizeCurrency(' usd ')).toBe('USD')
    expect(normalizeCurrency('dollars')).toBe('CNY')
    expect(normalizeCurrency(undefined)).toBe('CNY')
  })
})

describe('memberLevelLabel', () => {
  const level = { id: 2, name: { 'zh-CN': '黄金', 'en-US': 'Gold' }, icon: '🥇' } as unknown as AdminMemberLevel
  const img = { id: 3, name: { 'zh-CN': '钻石' }, icon: '/uploads/a.png' } as unknown as AdminMemberLevel
  const map = new Map([
    [2, level],
    [3, img],
  ])
  it('handles none / unknown / known', () => {
    expect(memberLevelLabel(0, map)).toBe('-')
    expect(memberLevelLabel(9, map)).toBe('#9')
    expect(memberLevelLabel(2, map).startsWith('🥇 ')).toBe(true)
    expect(memberLevelLabel(3, map).includes('/')).toBe(false)
  })
})

describe('misc helpers', () => {
  it('isImagePath', () => {
    expect(isImagePath('🥇')).toBe(false)
    expect(isImagePath('/uploads/x.png')).toBe(true)
    expect(isImagePath('')).toBe(false)
  })
  it('formatScopeProducts truncates after 3', () => {
    expect(formatScopeProducts([])).toBe('-')
    const p = (s: string) => ({ title: { 'zh-CN': s, 'zh-TW': s, 'en-US': s } })
    expect(formatScopeProducts([p('A'), p('B')])).toBe('A, B')
    expect(formatScopeProducts([p('A'), p('B'), p('C'), p('D')])).toBe('A, B, C...')
  })
  it('validateWalletAdjust', () => {
    expect(validateWalletAdjust('', 'x')).toBe('invalidAmount')
    expect(validateWalletAdjust('-1', 'x')).toBe('invalidAmount')
    expect(validateWalletAdjust('abc', 'x')).toBe('invalidAmount')
    expect(validateWalletAdjust('1.00', '  ')).toBe('remarkRequired')
    expect(validateWalletAdjust('1.00', 'ok')).toBeNull()
  })
  it('sanitizeChannelIds / toggleId', () => {
    expect(sanitizeChannelIds([1, '2', 0, -1, 3])).toEqual([1, 3])
    expect(sanitizeChannelIds(null)).toEqual([])
    expect(toggleId([1, 2], 2)).toEqual([1])
    expect(toggleId([1], 2)).toEqual([1, 2])
  })
})
