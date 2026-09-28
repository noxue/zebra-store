import { defineComponent, type PropType } from 'vue'
import { useAppStore } from '@/stores/app'

export type MascotMood = 'happy' | 'wink' | 'sad' | 'surprised'

/**
 * "Shima" — the store's original chibi mascot: a pastel girl with zebra-striped
 * cat ears, star hair clip and a gift box. Fully original artwork (no
 * third-party character). `theme.mascot_image` replaces it when configured.
 */
export const Mascot = defineComponent({
  name: 'ZsMascot',
  props: {
    mood: { type: String as PropType<MascotMood>, default: 'happy' },
    /** Ignore configured image and always draw the built-in SVG. */
    builtin: Boolean,
    float: { type: Boolean, default: true },
  },
  setup(props) {
    const appStore = useAppStore()
    const eyes = (mood: MascotMood) => {
      if (mood === 'wink') {
        return [
          <g key="l">
            <ellipse cx="84" cy="118" rx="9" ry="12" fill="#3b2a5c" />
            <circle cx="87" cy="113" r="3.5" fill="#fff" />
            <circle cx="81" cy="122" r="1.6" fill="#fff" opacity=".8" />
          </g>,
          <path key="r" d="M108 120 q9 -9 18 0" stroke="#3b2a5c" stroke-width="3.5" fill="none" stroke-linecap="round" />,
        ]
      }
      if (mood === 'sad') {
        return [
          <path key="l" d="M76 116 q8 8 17 0" stroke="#3b2a5c" stroke-width="3.5" fill="none" stroke-linecap="round" />,
          <path key="r" d="M108 116 q8 8 17 0" stroke="#3b2a5c" stroke-width="3.5" fill="none" stroke-linecap="round" />,
          <path key="t" d="M80 126 q-3 8 0 11 q3 -3 0 -11z" fill="#7dd3fc" />,
        ]
      }
      const ry = mood === 'surprised' ? 13 : 12
      return [84, 117].map((cx, i) => (
        <g key={i}>
          <ellipse cx={cx} cy="118" rx="9" ry={ry} fill="#3b2a5c" />
          <ellipse cx={cx} cy="123" rx="6" ry="5" fill="#8b5cf6" opacity=".55" />
          <circle cx={cx + 3} cy="113" r="3.5" fill="#fff" />
          <circle cx={cx - 3} cy="122" r="1.6" fill="#fff" opacity=".8" />
        </g>
      ))
    }
    const mouth = (mood: MascotMood) => {
      if (mood === 'sad') return <path d="M94 142 q6 -5 12 0" stroke="#c2185b" stroke-width="3" fill="none" stroke-linecap="round" />
      if (mood === 'surprised') return <ellipse cx="100" cy="143" rx="5" ry="6" fill="#c2185b" />
      return <path d="M92 138 q8 9 16 0 q-8 4 -16 0z" fill="#e11d74" stroke="#c2185b" stroke-width="1.5" stroke-linejoin="round" />
    }
    return () => {
      const img = appStore.mascotImage
      if (img && !props.builtin) {
        return <img src={img} alt="" class={['size-full object-contain', props.float && 'zs-float']} />
      }
      return (
        <svg viewBox="0 0 200 240" class={['size-full', props.float && 'zs-float']} role="img" aria-label="mascot">
          <defs>
            <linearGradient id="zs-hair" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stop-color="#ffb3d4" />
              <stop offset="1" stop-color="#c4a8ff" />
            </linearGradient>
            <linearGradient id="zs-dress" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stop-color="#a78bfa" />
              <stop offset="1" stop-color="#7dd3fc" />
            </linearGradient>
            <linearGradient id="zs-gift" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stop-color="#ff5fa2" />
              <stop offset="1" stop-color="#ff8fc0" />
            </linearGradient>
          </defs>
          {/* soft shadow */}
          <ellipse cx="100" cy="232" rx="46" ry="6" fill="#8b5cf6" opacity=".18" />
          {/* body / dress */}
          <path d="M62 236 q4 -52 38 -60 q34 8 38 60z" fill="url(#zs-dress)" />
          <path d="M84 178 l16 14 l16 -14" fill="#fff" opacity=".9" />
          <circle cx="100" cy="196" r="4" fill="#fbbf24" />
          {/* back hair */}
          <path d="M44 118 q-6 70 26 86 q-8 -40 4 -60z M156 118 q6 70 -26 86 q8 -40 -4 -60z" fill="url(#zs-hair)" />
          {/* ears (zebra striped) */}
          <g>
            <path d="M52 78 L46 26 L88 56 z" fill="#fff" stroke="#3b2a5c" stroke-width="3" stroke-linejoin="round" />
            <path d="M55 62 l14 -6 M52 48 l14 -2 M60 72 l12 -8" stroke="#3b2a5c" stroke-width="4" stroke-linecap="round" />
            <path d="M58 64 L55 42 L74 56 z" fill="#ffb3d4" opacity=".75" />
            <path d="M148 78 L154 26 L112 56 z" fill="#fff" stroke="#3b2a5c" stroke-width="3" stroke-linejoin="round" />
            <path d="M145 62 l-14 -6 M148 48 l-14 -2 M140 72 l-12 -8" stroke="#3b2a5c" stroke-width="4" stroke-linecap="round" />
            <path d="M142 64 L145 42 L126 56 z" fill="#ffb3d4" opacity=".75" />
          </g>
          {/* face */}
          <ellipse cx="100" cy="116" rx="54" ry="50" fill="#fff4ee" />
          {/* bangs */}
          <path d="M46 112 q2 -58 54 -58 q52 0 54 58 q-10 -18 -22 -22 q-2 14 -12 18 q0 -16 -8 -24 q-8 16 -22 20 q2 -12 -2 -20 q-10 14 -22 16 q2 -12 -2 -18 q-12 10 -18 30z" fill="url(#zs-hair)" />
          {/* star clip */}
          <path d="M140 74 l4 8 9 1 -7 6 2 9 -8 -5 -8 5 2 -9 -7 -6 9 -1z" fill="#fbbf24" stroke="#fff" stroke-width="1.5" />
          {/* blush */}
          <ellipse cx="70" cy="134" rx="10" ry="5" fill="#ff8fc0" opacity=".55" />
          <ellipse cx="130" cy="134" rx="10" ry="5" fill="#ff8fc0" opacity=".55" />
          {eyes(props.mood)}
          {mouth(props.mood)}
          {/* arms + gift box */}
          <g>
            <rect x="80" y="196" width="40" height="30" rx="6" fill="url(#zs-gift)" />
            <rect x="96" y="196" width="8" height="30" fill="#fff" opacity=".9" />
            <path d="M100 196 q-14 -14 -18 -4 q4 6 18 4 q14 2 18 -4 q-4 -10 -18 4z" fill="#fbbf24" />
            <ellipse cx="78" cy="208" rx="8" ry="7" fill="#fff4ee" />
            <ellipse cx="122" cy="208" rx="8" ry="7" fill="#fff4ee" />
          </g>
          {/* sparkles */}
          <path d="M26 150 l3 7 7 3 -7 3 -3 7 -3 -7 -7 -3 7 -3z" fill="#38bdf8" opacity=".8" />
          <path d="M176 120 l2 5 5 2 -5 2 -2 5 -2 -5 -5 -2 5 -2z" fill="#ff5fa2" opacity=".8" />
        </svg>
      )
    }
  },
})

/** Small original kitty used for empty states and 404. */
export const Kitty = defineComponent({
  name: 'ZsKitty',
  props: { mood: { type: String as PropType<'happy' | 'sad' | 'sleepy'>, default: 'happy' } },
  setup(props) {
    return () => (
      <svg viewBox="0 0 160 140" class="size-full" aria-hidden="true">
        <ellipse cx="80" cy="132" rx="46" ry="6" fill="var(--zs-secondary)" opacity=".15" />
        <path d="M122 104 q30 -6 22 -34" stroke="#c4a8ff" stroke-width="10" fill="none" stroke-linecap="round" />
        <ellipse cx="80" cy="100" rx="44" ry="32" fill="#fff" stroke="#e9d5ff" stroke-width="3" />
        <path d="M40 58 L44 18 L70 40z M120 58 L116 18 L90 40z" fill="#fff" stroke="#e9d5ff" stroke-width="3" stroke-linejoin="round" />
        <path d="M47 44 L49 28 L60 38z M113 44 L111 28 L100 38z" fill="#ffb3d4" />
        <ellipse cx="80" cy="64" rx="44" ry="36" fill="#fff" stroke="#e9d5ff" stroke-width="3" />
        {props.mood === 'sleepy' ? (
          <g stroke="#3b2a5c" stroke-width="3" stroke-linecap="round" fill="none">
            <path d="M58 64 q6 5 12 0" />
            <path d="M90 64 q6 5 12 0" />
          </g>
        ) : props.mood === 'sad' ? (
          <g>
            <path d="M58 62 q6 -5 12 0 M90 62 q6 -5 12 0" stroke="#3b2a5c" stroke-width="3" stroke-linecap="round" fill="none" />
            <path d="M60 68 q-3 7 0 10 q3 -3 0 -10z" fill="#7dd3fc" />
          </g>
        ) : (
          <g fill="#3b2a5c">
            <ellipse cx="64" cy="64" rx="5.5" ry="7" />
            <ellipse cx="96" cy="64" rx="5.5" ry="7" />
            <circle cx="66" cy="61" r="2" fill="#fff" />
            <circle cx="98" cy="61" r="2" fill="#fff" />
          </g>
        )}
        <path d="M76 74 q4 3 8 0" stroke="#e11d74" stroke-width="2.5" fill="none" stroke-linecap="round" />
        <ellipse cx="52" cy="76" rx="7" ry="4" fill="#ff8fc0" opacity=".5" />
        <ellipse cx="108" cy="76" rx="7" ry="4" fill="#ff8fc0" opacity=".5" />
        <path d="M30 70 h14 M30 78 h14 M116 70 h14 M116 78 h14" stroke="#d8c7ff" stroke-width="2" stroke-linecap="round" />
        <path d="M128 22 l2 5 5 2 -5 2 -2 5 -2 -5 -5 -2 5 -2z" fill="#fbbf24" />
      </svg>
    )
  },
})
