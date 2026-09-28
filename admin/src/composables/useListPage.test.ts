import { describe, expect, it } from 'vitest'
import { normalizeListResult, resolveJumpTarget, useListPage } from './useListPage'

const envelope = (page: number, total: number) => ({
  status_code: 0,
  msg: 'ok',
  data: [{ id: page }],
  pagination: { page, page_size: 20, total, total_page: Math.ceil(total / 20) },
})

describe('useListPage', () => {
  it('normalizes envelope and plain results', () => {
    expect(normalizeListResult({ status_code: 0, msg: '', data: null }).items).toEqual([])
    expect(normalizeListResult({ items: [1, 2] }).items).toEqual([1, 2])
  })

  it('clamps jump targets', () => {
    expect(resolveJumpTarget('9', 1, 3)).toBe(3)
    expect(resolveJumpTarget('0', 2, 3)).toBe(1)
    expect(resolveJumpTarget('2', 2, 3)).toBeNull()
    expect(resolveJumpTarget('x', 1, 3)).toBeNull()
    expect(resolveJumpTarget('', 1, 3)).toBeNull()
  })

  it('fetches pages, tracks pagination and ignores out-of-range navigation', async () => {
    const calls: Array<[number, number]> = []
    const list = useListPage<{ id: number }>({
      fetchFn: async (page, size) => {
        calls.push([page, size])
        return envelope(page, 45)
      },
    })
    await list.fetchData(1)
    expect(list.items.value).toEqual([{ id: 1 }])
    expect(list.pagination.value.total_page).toBe(3)
    list.changePage(5)
    expect(calls.length).toBe(1)
    list.changePageSize(50)
    await Promise.resolve()
    expect(calls[1]).toEqual([1, 50])
  })

  it('keeps only the latest response when requests race', async () => {
    const resolvers: Array<() => void> = []
    const list = useListPage<{ id: number }>({
      fetchFn: (page) => new Promise((resolve) => resolvers.push(() => resolve(envelope(page, 100)))),
    })
    const first = list.fetchData(1)
    const second = list.fetchData(2)
    resolvers[1]?.()
    await second
    resolvers[0]?.()
    await first
    expect(list.items.value).toEqual([{ id: 2 }])
    expect(list.loading.value).toBe(false)
  })

  it('clears items on failure', async () => {
    const list = useListPage<{ id: number }>({ fetchFn: async () => Promise.reject(new Error('boom')) })
    await list.fetchData(1)
    expect(list.items.value).toEqual([])
    expect(list.loading.value).toBe(false)
  })
})
