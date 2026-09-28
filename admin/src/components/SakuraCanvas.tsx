import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue'

const MAX_PETALS = 25

interface Petal {
  x: number
  y: number
  r: number
  vy: number
  vx: number
  rot: number
  vr: number
  sway: number
}

/** Lightweight falling-sakura canvas (≤25 petals, disabled under prefers-reduced-motion). */
export const SakuraCanvas = defineComponent({
  name: 'SakuraCanvas',
  props: { count: { type: Number, default: 18 } },
  setup(props) {
    const canvas = ref<HTMLCanvasElement | null>(null)
    let raf = 0
    let petals: Petal[] = []
    const reduced = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches

    const spawn = (w: number, h: number, top = false): Petal => ({
      x: Math.random() * w,
      y: top ? -20 : Math.random() * h,
      r: 5 + Math.random() * 6,
      vy: 0.4 + Math.random() * 0.9,
      vx: -0.3 + Math.random() * 0.6,
      rot: Math.random() * Math.PI,
      vr: -0.02 + Math.random() * 0.04,
      sway: Math.random() * Math.PI * 2,
    })

    onMounted(() => {
      const el = canvas.value
      if (!el || reduced) return
      const ctx = el.getContext('2d')
      if (!ctx) return
      const resize = () => {
        el.width = el.offsetWidth * devicePixelRatio
        el.height = el.offsetHeight * devicePixelRatio
      }
      resize()
      window.addEventListener('resize', resize)
      const color = getComputedStyle(document.documentElement).getPropertyValue('--zs-primary').trim() || '#ff5fa2'
      petals = Array.from({ length: Math.min(props.count, MAX_PETALS) }, () => spawn(el.width, el.height))
      const draw = () => {
        ctx.clearRect(0, 0, el.width, el.height)
        for (const p of petals) {
          p.sway += 0.01
          p.x += p.vx + Math.sin(p.sway) * 0.4
          p.y += p.vy * devicePixelRatio
          p.rot += p.vr
          if (p.y > el.height + 20) Object.assign(p, spawn(el.width, el.height, true))
          ctx.save()
          ctx.translate(p.x, p.y)
          ctx.rotate(p.rot)
          ctx.globalAlpha = 0.75
          ctx.fillStyle = color
          const r = p.r * devicePixelRatio
          ctx.beginPath()
          ctx.moveTo(0, -r)
          ctx.bezierCurveTo(r, -r, r, r * 0.6, 0, r)
          ctx.bezierCurveTo(-r, r * 0.6, -r, -r, 0, -r)
          ctx.fill()
          ctx.restore()
        }
        raf = requestAnimationFrame(draw)
      }
      draw()
      onBeforeUnmount(() => window.removeEventListener('resize', resize))
    })
    onBeforeUnmount(() => cancelAnimationFrame(raf))
    return () => <canvas ref={canvas} class="pointer-events-none absolute inset-0 h-full w-full" aria-hidden="true" />
  },
})

export default SakuraCanvas
