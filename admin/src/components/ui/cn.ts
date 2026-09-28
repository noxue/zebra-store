export type ClassValue = string | false | null | undefined | 0 | ClassValue[] | Record<string, boolean | undefined>

/** Tiny class joiner (clsx-like). */
export function cn(...inputs: ClassValue[]): string {
  const out: string[] = []
  const walk = (v: ClassValue) => {
    if (!v) return
    if (typeof v === 'string') out.push(v)
    else if (Array.isArray(v)) v.forEach(walk)
    else for (const [k, on] of Object.entries(v)) if (on) out.push(k)
  }
  inputs.forEach(walk)
  return out.join(' ')
}

/** Icon component type (lucide icons). */
export type { LucideIcon as IconComponent } from 'lucide-vue-next'
