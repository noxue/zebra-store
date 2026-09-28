import { describe, expect, it } from 'vitest'
import { orderEmailSceneKeys } from './orderEmailTemplates'

describe('order email scenes', () => {
  it('does not define canceled while retaining actionable refund notifications', () => {
    expect(orderEmailSceneKeys).not.toContain('canceled')
    expect(orderEmailSceneKeys).toContain('refunded')
    expect(orderEmailSceneKeys).toContain('partially_refunded')
  })
})
