export interface ResellerManagementState {
  canApply: boolean
  canSubmitDomain: boolean
  statusKey: string
}

export const getResellerProfileStatusKey = (status?: string): string => {
  if (status === 'pending_review') return 'pendingReview'
  if (status === 'active') return 'active'
  if (status === 'rejected') return 'rejected'
  if (status === 'disabled') return 'disabled'
  return 'unknown'
}

export const getResellerDomainStatusKey = (status?: string): string => {
  if (status === 'pending_review') return 'pendingReview'
  if (status === 'active') return 'active'
  if (status === 'disabled') return 'disabled'
  return 'unknown'
}

export const isResellerProfileActive = (profile?: { status?: string } | null): boolean => profile?.status === 'active'

export const getResellerManagementState = (
  snapshot?: { opened?: boolean; can_apply?: boolean; profile?: { status?: string } | null } | null,
): ResellerManagementState => {
  if (!snapshot) return { canApply: false, canSubmitDomain: false, statusKey: 'unknown' }
  if (!snapshot.opened) return { canApply: snapshot.can_apply === true, canSubmitDomain: false, statusKey: 'notOpened' }
  return {
    canApply: snapshot.can_apply === true,
    canSubmitDomain: isResellerProfileActive(snapshot.profile),
    statusKey: getResellerProfileStatusKey(snapshot.profile?.status),
  }
}
