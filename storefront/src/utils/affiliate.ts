import type { RouteLocationNormalized } from 'vue-router'
import { affiliateAPI } from '@/api/affiliate'

const AFFILIATE_STORAGE_KEY = 'dj_affiliate_attribution'
const AFFILIATE_VISITOR_KEY = 'dj_affiliate_visitor_key'
/** 30 days, same as original storefront. */
const AFFILIATE_TTL_MS = 30 * 24 * 60 * 60 * 1000

interface AffiliateAttribution {
  code: string
  expires_at: number
}

let clock: () => number = () => Date.now()
/** Lets the app store inject the server-corrected clock. */
export const setAffiliateClock = (fn: () => number) => {
  clock = fn
}

export const normalizeAffiliateCode = (raw: string): string => {
  const value = String(raw || '').trim().toUpperCase()
  return /^[A-Z0-9]{4,32}$/.test(value) ? value : ''
}

const readAttribution = (): AffiliateAttribution | null => {
  const raw = localStorage.getItem(AFFILIATE_STORAGE_KEY)
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw) as Partial<AffiliateAttribution>
    const code = normalizeAffiliateCode(String(parsed.code || ''))
    const expiresAt = Number(parsed.expires_at || 0)
    if (!code || !Number.isFinite(expiresAt) || expiresAt <= clock()) {
      localStorage.removeItem(AFFILIATE_STORAGE_KEY)
      return null
    }
    return { code, expires_at: expiresAt }
  } catch {
    localStorage.removeItem(AFFILIATE_STORAGE_KEY)
    return null
  }
}

const ensureVisitorKey = (): string => {
  const existing = String(localStorage.getItem(AFFILIATE_VISITOR_KEY) || '').trim()
  if (existing) return existing
  const next = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 12)}`
  localStorage.setItem(AFFILIATE_VISITOR_KEY, next)
  return next
}

export const getAffiliateCode = (): string => readAttribution()?.code || ''
export const getAffiliateVisitorKey = (): string => ensureVisitorKey()
export const clearAffiliateCode = () => localStorage.removeItem(AFFILIATE_STORAGE_KEY)

export const captureAffiliateFromRoute = async (route: RouteLocationNormalized): Promise<void> => {
  const raw = Array.isArray(route.query.aff) ? route.query.aff[0] : route.query.aff
  const code = raw ? normalizeAffiliateCode(String(raw)) : ''
  if (!code) return
  const payload: AffiliateAttribution = { code, expires_at: clock() + AFFILIATE_TTL_MS }
  localStorage.setItem(AFFILIATE_STORAGE_KEY, JSON.stringify(payload))
  try {
    await affiliateAPI.trackClick({
      affiliate_code: code,
      visitor_key: ensureVisitorKey(),
      landing_path: route.fullPath || route.path || '/',
      referrer: document.referrer,
    })
  } catch {
    // click tracking is best effort
  }
}
