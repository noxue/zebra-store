import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import type { CartItem } from './cart'

/** Single in-memory item for `/checkout?mode=buynow`. */
export const useBuyNowStore = defineStore('buyNow', () => {
  const item = ref<CartItem | null>(null)
  const hasItem = computed(() => item.value !== null)
  const setItem = (next: CartItem) => {
    item.value = { ...next }
  }
  const clear = () => {
    item.value = null
  }
  return { item, hasItem, setItem, clear }
})
