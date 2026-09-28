import type { ApiCredentialData } from '@/api/types'

/** Protocol whose buyers can onboard with a connection code (protocol spec §3). */
export const CONNECTION_CODE_PROTOCOL = 'zebra-store'

/** 一键对接 is offered for an approved credential when the site serves the zebra-store protocol. */
export const supportsConnectionCode = (credential: ApiCredentialData | null | undefined): boolean =>
  !!credential && credential.status === 'approved' && (credential.protocols ?? []).includes(CONNECTION_CODE_PROTOCOL)

const MASK_HEAD = 10
const MASK_TAIL = 4

/** Hide the middle of a connection code (it embeds the secret): `zsc1_eyJ2I••••••••Q9fQ`. */
export const maskConnectionCode = (code: string): string => {
  if (code.length <= MASK_HEAD + MASK_TAIL) return '•'.repeat(code.length)
  return `${code.slice(0, MASK_HEAD)}${'•'.repeat(8)}${code.slice(-MASK_TAIL)}`
}

export interface RotationState {
  pending: boolean
  expiresAt: string | null
}

/**
 * Rotation shown in the panel: a fresher value from the last code/rotate response wins over the
 * loaded credential; an expiry in the past means the grace period is over.
 */
export const resolveRotationState = (
  credential: Pick<ApiCredentialData, 'rotation_pending' | 'rotation_expires_at'> | null | undefined,
  local: string | null,
  now: number = Date.now(),
): RotationState => {
  const expiresAt = local ?? credential?.rotation_expires_at ?? null
  const pending = !!local || !!credential?.rotation_pending
  if (!pending) return { pending: false, expiresAt: null }
  if (expiresAt) {
    const ts = new Date(expiresAt).getTime()
    if (!Number.isNaN(ts) && ts <= now) return { pending: false, expiresAt: null }
  }
  return { pending: true, expiresAt }
}
