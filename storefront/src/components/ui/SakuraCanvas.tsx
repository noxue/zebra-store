import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue'

/** Max petals on screen (DESIGN.md: ≤ 25). */
const MAX_PETALS = 25

interface Petal {
  x: number
  y: number
  size: number
  speedY: number
  speedX: number
  angle: number
  spin: number
  sway: number
  hue: number
}

/** Lightweight falling sakura petals. Disabled with prefers-reduced-motion. */
export const SakuraCanvas = defineComponent({
  name: 'ZsSakuraCanvas',
  setup() {
    const canvas = ref<HTMLCanvasElement | null>(null)
    let raf = 0
    let petals: Petal[] = []
    let running = false

    const spawn = (w: number, h: number, initial: boolean): Petal => ({
      x: Math.random() * w,
      y: initial ? Math.random() * h : -20,
      size: 6 + Math.random() * 8,
      speedY: 0.35 + Math.random() * 0.7,
      speedX: -0.3 + Math.random() * 0.6,
      angle: Math.random() * Math.PI * 2,
      spin: -0.02 + Math.random() * 0.04,
      sway: Math.random() * Math.PI * 2,
      hue: Math.random(),
    })

    const resize = () => {
      const el = canvas.value
      if (!el) return
      const dpr = Math.min(window.devicePixelRatio || 1, 2)
      el.width = window.innerWidth * dpr
      el.height = window.innerHeight * dpr
      el.getContext('2d')?.setTransform(dpr, 0, 0, dpr, 0, 0)
    }

    const drawPetal = (ctx: CanvasRenderingContext2D, p: Petal) => {
      ctx.save()
      ctx.translate(p.x, p.y)
      ctx.rotate(p.angle)
      ctx.globalAlpha = 0.75
      const grad = ctx.createLinearGradient(-p.size, 0, p.size, 0)
      grad.addColorStop(0, p.hue > 0.5 ? '#ffc4dd' : '#ffd6e8')
      grad.addColorStop(1, p.hue > 0.5 ? '#ff8fc0' : '#f9a8d4')
      ctx.fillStyle = grad
      ctx.beginPath()
      ctx.moveTo(0, -p.size * 0.6)
      ctx.bezierCurveTo(p.size * 0.9, -p.size * 0.8, p.size * 0.9, p.size * 0.5, 0, p.size * 0.7)
      ctx.bezierCurveTo(-p.size * 0.9, p.size * 0.5, -p.size * 0.9, -p.size * 0.8, 0, -p.size * 0.6)
      ctx.fill()
      ctx.restore()
    }

    const tick = () => {
      const el = canvas.value
      const ctx = el?.getContext('2d')
      if (!el || !ctx || !running) return
      const w = window.innerWidth
      const h = window.innerHeight
      ctx.clearRect(0, 0, w, h)
      for (const p of petals) {
        p.sway += 0.01
        p.x += p.speedX + Math.sin(p.sway) * 0.4
        p.y += p.speedY
        p.angle += p.spin
        if (p.y > h + 20 || p.x < -30 || p.x > w + 30) Object.assign(p, spawn(w, h, false))
        drawPetal(ctx, p)
      }
      raf = requestAnimationFrame(tick)
    }

    const onVisibility = () => {
      if (document.hidden) {
        running = false
        cancelAnimationFrame(raf)
      } else if (!running) {
        running = true
        raf = requestAnimationFrame(tick)
      }
    }

    onMounted(() => {
      if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return
      resize()
      const count = window.innerWidth < 640 ? 12 : MAX_PETALS
      petals = Array.from({ length: count }, () => spawn(window.innerWidth, window.innerHeight, true))
      running = true
      raf = requestAnimationFrame(tick)
      window.addEventListener('resize', resize)
      document.addEventListener('visibilitychange', onVisibility)
    })
    onBeforeUnmount(() => {
      running = false
      cancelAnimationFrame(raf)
      window.removeEventListener('resize', resize)
      document.removeEventListener('visibilitychange', onVisibility)
    })
    return () => <canvas ref={canvas} class="pointer-events-none fixed inset-0 z-[1] size-full" aria-hidden="true" />
  },
})
