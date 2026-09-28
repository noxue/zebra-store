import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

/** Data created by earlier specs and consumed by later ones. */
export interface E2EState {
  runId?: string
  siteName?: string
  primaryColor?: string
  categorySlug?: string
  productSlug?: string
  productTitle?: string
  skuCode?: string
  couponCode?: string
  giftCardCode?: string
  guestEmail?: string
  guestPassword?: string
  guestOrderNo?: string
  memberEmail?: string
  memberPassword?: string
  memberOrderNos?: string[]
}

const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '.state')
const file = path.join(dir, 'state.json')

export function loadState(): E2EState {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8')) as E2EState
  } catch {
    return {}
  }
}

export function saveState(patch: Partial<E2EState>): E2EState {
  const next = { ...loadState(), ...patch }
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(file, JSON.stringify(next, null, 2))
  return next
}

/** Reads a value an earlier spec must have stored. */
export function need<K extends keyof E2EState>(key: K): NonNullable<E2EState[K]> {
  const v = loadState()[key]
  if (v === undefined || v === null) throw new Error(`e2e state "${key}" missing — run the specs in order (npm test)`)
  return v as NonNullable<E2EState[K]>
}
