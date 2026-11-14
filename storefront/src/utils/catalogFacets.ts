import type { CatalogFacetFilters, CatalogFacetKey } from '@/api/types'

/** Browse order follows the customer's choices; duration remains a real SKU option. */
export const CATALOG_FACET_KEYS: CatalogFacetKey[] = ['family', 'project', 'ip_type', 'line', 'country', 'city']
export const facetParam = (key: CatalogFacetKey): `facet_${CatalogFacetKey}` => `facet_${key}`
export const validFacetId = (value: unknown): value is string => typeof value === 'string' && /^[A-Za-z0-9_:.-]{1,128}$/.test(value)
export type FacetQueryResult = { ok: true; filters: CatalogFacetFilters } | { ok: false }

export function parseFacetQuery(query: Record<string, unknown>): FacetQueryResult {
  const filters: CatalogFacetFilters = {}
  const known = new Set(CATALOG_FACET_KEYS.map(facetParam))
  for (const [key, value] of Object.entries(query)) {
    if (!key.startsWith('facet_')) continue
    if (!known.has(key as `facet_${CatalogFacetKey}`) || !validFacetId(value)) return { ok: false }
    filters[key as `facet_${CatalogFacetKey}`] = value
  }
  return { ok: true, filters }
}

export function selectFacet(filters: CatalogFacetFilters, key: CatalogFacetKey, value: string | null): CatalogFacetFilters {
  if (value !== null && !validFacetId(value)) return filters
  const next = { ...filters }
  const index = CATALOG_FACET_KEYS.indexOf(key)
  for (const child of CATALOG_FACET_KEYS.slice(index + 1)) delete next[facetParam(child)]
  if (value === null) delete next[facetParam(key)]
  else next[facetParam(key)] = value
  return next
}

export function withoutFacetQuery<T extends Record<string, unknown>>(query: T): Record<string, unknown> {
  return Object.fromEntries(Object.entries(query).filter(([key]) => !key.startsWith('facet_')))
}
