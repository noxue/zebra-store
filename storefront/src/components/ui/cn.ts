export type ClassValue = string | false | null | undefined | 0 | ClassValue[] | Record<string, boolean | undefined | null>

/** Minimal class name joiner. */
export const cn = (...values: ClassValue[]): string => {
  const out: string[] = []
  const walk = (v: ClassValue) => {
    if (!v) return
    if (typeof v === 'string') out.push(v)
    else if (Array.isArray(v)) v.forEach(walk)
    else if (typeof v === 'object') for (const [k, on] of Object.entries(v)) if (on) out.push(k)
  }
  values.forEach(walk)
  return out.join(' ')
}
