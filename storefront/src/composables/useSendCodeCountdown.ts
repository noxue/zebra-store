import { onBeforeUnmount, ref } from 'vue'

/** Resend cooldown after sending an email verification code (60s like the original). */
export const SEND_CODE_COOLDOWN_SECONDS = 60

export function useSendCodeCountdown(seconds = SEND_CODE_COOLDOWN_SECONDS) {
  const countdown = ref(0)
  let timer: ReturnType<typeof setInterval> | undefined
  const stop = () => {
    if (timer) clearInterval(timer)
    timer = undefined
  }
  const start = () => {
    stop()
    countdown.value = seconds
    timer = setInterval(() => {
      countdown.value -= 1
      if (countdown.value <= 0) {
        countdown.value = 0
        stop()
      }
    }, 1000)
  }
  onBeforeUnmount(stop)
  return { countdown, start, stop }
}
