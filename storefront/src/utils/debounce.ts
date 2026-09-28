export interface Debounced<A extends unknown[]> {
  (...args: A): void
  cancel: () => void
}

/** Trailing-edge debounce. */
export const debounce = <A extends unknown[]>(fn: (...args: A) => void, wait = 300): Debounced<A> => {
  let timer: ReturnType<typeof setTimeout> | null = null
  const debounced = ((...args: A) => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => {
      timer = null
      fn(...args)
    }, wait)
  }) as Debounced<A>
  debounced.cancel = () => {
    if (timer) clearTimeout(timer)
    timer = null
  }
  return debounced
}
