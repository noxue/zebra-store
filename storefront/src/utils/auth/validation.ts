export type PasswordStrength = 'weak' | 'medium' | 'strong'

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

export const isValidEmail = (value: string): boolean => EMAIL_PATTERN.test(value.trim())

/** Same scoring as the original storefront. */
export const getPasswordStrength = (password: string): PasswordStrength => {
  if (!password || password.length < 6) return 'weak'
  let score = 0
  if (password.length >= 8) score++
  if (password.length >= 12) score++
  if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score++
  if (/\d/.test(password)) score++
  if (/[^a-zA-Z0-9]/.test(password)) score++
  if (score <= 2) return 'weak'
  if (score <= 3) return 'medium'
  return 'strong'
}

/** Lower-cased, de-duplicated domains without leading '@'. */
export const normalizeEmailDomains = (raw: unknown): string[] => {
  if (!Array.isArray(raw)) return []
  const seen = new Set<string>()
  const out: string[] = []
  for (const item of raw) {
    const domain = String(item || '').trim().replace(/^@+/, '').toLowerCase()
    if (!domain || seen.has(domain)) continue
    seen.add(domain)
    out.push(domain)
  }
  return out
}

export const getEmailDomain = (value: string): string => {
  const normalized = value.trim().toLowerCase()
  const at = normalized.lastIndexOf('@')
  if (at <= 0 || at === normalized.length - 1) return ''
  return normalized.slice(at + 1)
}

/** local part + selected domain → full email ('' when either is missing). */
export const composeEmail = (localPart: string, domain: string): string => {
  const local = localPart.trim()
  const d = domain.trim()
  return local && d ? `${local}@${d}` : ''
}

/** Seconds left until an ISO timestamp, never negative. */
export const secondsUntil = (iso: string, now: number): number => {
  const target = new Date(iso).getTime()
  if (!Number.isFinite(target)) return 0
  return Math.max(0, Math.floor((target - now) / 1000))
}
