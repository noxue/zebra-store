const DECIMAL_PATTERN = /^[+-]?\d+(?:\.\d+)?$/

const parseDecimalToScaledInt = (value: unknown, scale: number): number | null => {
  if (scale < 0) return null
  if (value === null || value === undefined) return null
  const raw = String(value).trim()
  if (!raw) return null
  if (!DECIMAL_PATTERN.test(raw)) return null

  const negative = raw.startsWith('-')
  const normalized = raw.replace(/^[+-]/, '')
  const [intPartRaw, fracPartRaw = ''] = normalized.split('.')
  const intPart = intPartRaw || '0'
  const factor = 10 ** scale
  if (!Number.isSafeInteger(factor)) return null

  const mainFrac = fracPartRaw.padEnd(scale + 1, '0').slice(0, scale)
  const roundDigit = fracPartRaw.padEnd(scale + 1, '0').charAt(scale) || '0'

  const major = Number(intPart)
  const minor = mainFrac ? Number(mainFrac) : 0
  if (!Number.isSafeInteger(major) || !Number.isSafeInteger(minor)) return null

  let scaled = major * factor + minor
  if (!Number.isSafeInteger(scaled)) return null
  if (roundDigit >= '5') {
    scaled += 1
  }
  if (!Number.isSafeInteger(scaled)) return null

  return negative ? -scaled : scaled
}

export const amountToCents = (value: unknown): number | null => parseDecimalToScaledInt(value, 2)

export const rateToBasisPoints = (value: unknown): number | null => parseDecimalToScaledInt(value, 2)

export const centsToAmount = (cents: number): string => {
  if (!Number.isFinite(cents) || !Number.isSafeInteger(cents)) return '0.00'
  const negative = cents < 0
  const absolute = Math.abs(cents)
  const major = Math.floor(absolute / 100)
  const minor = String(absolute % 100).padStart(2, '0')
  return `${negative ? '-' : ''}${major}.${minor}`
}

export const basisPointsToPercent = (basisPoints: number): string => {
  if (!Number.isFinite(basisPoints) || !Number.isSafeInteger(basisPoints)) return '0.00'
  const negative = basisPoints < 0
  const absolute = Math.abs(basisPoints)
  const major = Math.floor(absolute / 100)
  const minor = String(absolute % 100).padStart(2, '0')
  return `${negative ? '-' : ''}${major}.${minor}`
}

export const calculateFeeCents = (baseCents: number, rateBasisPoints: number): number | null => {
  if (!Number.isSafeInteger(baseCents) || !Number.isSafeInteger(rateBasisPoints)) return null
  const multiplied = baseCents * rateBasisPoints
  if (!Number.isFinite(multiplied) || Math.abs(multiplied) > Number.MAX_SAFE_INTEGER) return null
  const fee = Math.round(multiplied / 10000)
  if (!Number.isSafeInteger(fee)) return null
  return fee
}

export const parseInteger = (value: unknown): number | null => {
  if (value === null || value === undefined) return null
  const num = Number(value)
  if (!Number.isFinite(num) || !Number.isInteger(num)) return null
  if (!Number.isSafeInteger(num)) return null
  return num
}

/**
 * `"12.30 CNY"` formatting used across the storefront. Empty → '-';
 * non-decimal input is echoed unchanged; `currency` null/'' → no suffix.
 */
export const formatMoney = (amount: string | number | null | undefined, currency?: string | null): string => {
  if (amount === null || amount === undefined || amount === '') return '-'
  const cents = amountToCents(amount)
  const display = cents === null ? String(amount) : centsToAmount(cents)
  if (!currency) return display
  return `${display} ${currency}`
}

/** Adds decimal strings exactly (via cents). */
export const addAmounts = (...values: Array<string | number | null | undefined>): string =>
  centsToAmount(values.reduce<number>((sum, v) => sum + (amountToCents(v) ?? 0), 0))

/** unit × quantity as a decimal string. */
export const multiplyAmount = (unit: string | number | null | undefined, quantity: number): string =>
  centsToAmount((amountToCents(unit) ?? 0) * Math.max(0, Math.floor(quantity)))

/** True when the amount parses and is > 0. */
export const isPositiveAmount = (value: unknown): boolean => (amountToCents(value) ?? 0) > 0
