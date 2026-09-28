import { defineComponent, type PropType } from 'vue'

export type MascotMood = 'happy' | 'wink' | 'sleepy' | 'surprised'

/**
 * "Zebi" — Zebra Store's original chibi zebra mascot (hand-drawn SVG, no third-party character).
 * Colours come from the design tokens so it follows light/dark and admin theme overrides.
 */
export const Mascot = defineComponent({
  name: 'ZsMascot',
  props: {
    mood: { type: String as PropType<MascotMood>, default: 'happy' },
    size: { type: Number, default: 160 },
    float: Boolean,
  },
  setup(props) {
    return () => {
      const eyes =
        props.mood === 'sleepy' ? (
          <g stroke="var(--zs-mascot-ink)" stroke-width="3" stroke-linecap="round" fill="none">
            <path d="M74 112q8 6 16 0" />
            <path d="M110 112q8 6 16 0" />
          </g>
        ) : props.mood === 'wink' ? (
          <g>
            <ellipse cx="82" cy="110" rx="8" ry="10" fill="var(--zs-mascot-ink)" />
            <circle cx="85" cy="106" r="3" fill="var(--zs-mascot-fill)" />
            <path d="M110 112q8 -7 16 0" stroke="var(--zs-mascot-ink)" stroke-width="3" stroke-linecap="round" fill="none" />
          </g>
        ) : (
          <g>
            <ellipse cx="82" cy="110" rx="8" ry={props.mood === 'surprised' ? 11 : 10} fill="var(--zs-mascot-ink)" />
            <ellipse cx="118" cy="110" rx="8" ry={props.mood === 'surprised' ? 11 : 10} fill="var(--zs-mascot-ink)" />
            <circle cx="85" cy="106" r="3" fill="var(--zs-mascot-fill)" />
            <circle cx="121" cy="106" r="3" fill="var(--zs-mascot-fill)" />
            <circle cx="79" cy="114" r="1.5" fill="var(--zs-mascot-fill)" />
            <circle cx="115" cy="114" r="1.5" fill="var(--zs-mascot-fill)" />
          </g>
        )
      const mouth =
        props.mood === 'surprised' ? (
          <ellipse cx="100" cy="134" rx="5" ry="6" fill="var(--zs-primary)" />
        ) : (
          <path d="M92 131q8 8 16 0" stroke="var(--zs-mascot-ink)" stroke-width="2.5" stroke-linecap="round" fill="var(--zs-mascot-blush)" />
        )
      return (
        <svg
          class={props.float ? 'zs-float' : undefined}
          width={props.size}
          height={props.size}
          viewBox="0 0 200 200"
          role="img"
          aria-label="Zebi mascot"
        >
          <defs>
            <linearGradient id="zs-mascot-mane" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stop-color="var(--zs-primary)" />
              <stop offset="0.6" stop-color="var(--zs-secondary)" />
              <stop offset="1" stop-color="var(--zs-accent)" />
            </linearGradient>
          </defs>
          <ellipse cx="100" cy="186" rx="52" ry="7" fill="var(--zs-secondary)" opacity="0.15" />
          {/* body */}
          <path d="M62 150q-6 34 18 36h40q24-2 18-36z" fill="var(--zs-mascot-fill)" stroke="var(--zs-mascot-ink)" stroke-width="3" />
          <path d="M70 162h16M114 162h16M76 174h12M112 174h12" stroke="var(--zs-mascot-ink)" stroke-width="4" stroke-linecap="round" />
          {/* ears */}
          <path d="M58 64q-10-34 14-40q12 16 8 38z" fill="var(--zs-mascot-fill)" stroke="var(--zs-mascot-ink)" stroke-width="3" stroke-linejoin="round" />
          <path d="M142 64q10-34-14-40q-12 16-8 38z" fill="var(--zs-mascot-fill)" stroke="var(--zs-mascot-ink)" stroke-width="3" stroke-linejoin="round" />
          <path d="M64 56q-4-16 6-22q6 10 4 22z" fill="var(--zs-mascot-blush)" />
          <path d="M136 56q4-16-6-22q-6 10-4 22z" fill="var(--zs-mascot-blush)" />
          {/* head */}
          <ellipse cx="100" cy="104" rx="56" ry="50" fill="var(--zs-mascot-fill)" stroke="var(--zs-mascot-ink)" stroke-width="3" />
          {/* stripes */}
          <path d="M100 56q-4 12 0 22q4-10 0-22z" fill="var(--zs-mascot-ink)" />
          <path d="M72 64q2 12 10 18q-2-12-10-18z" fill="var(--zs-mascot-ink)" />
          <path d="M128 64q-2 12-10 18q2-12 10-18z" fill="var(--zs-mascot-ink)" />
          <path d="M46 100q10 2 14 10q-10-2-14-10z" fill="var(--zs-mascot-ink)" />
          <path d="M154 100q-10 2-14 10q10-2 14-10z" fill="var(--zs-mascot-ink)" />
          {/* mane */}
          <path d="M78 58q10-26 24-22q16-6 22 20q-10-8-22-6q-12-2-24 8z" fill="url(#zs-mascot-mane)" stroke="var(--zs-mascot-ink)" stroke-width="2.5" stroke-linejoin="round" />
          {/* muzzle */}
          <ellipse cx="100" cy="132" rx="26" ry="16" fill="var(--zs-mascot-blush)" stroke="var(--zs-mascot-ink)" stroke-width="2.5" />
          <ellipse cx="91" cy="127" rx="2.4" ry="2" fill="var(--zs-mascot-ink)" />
          <ellipse cx="109" cy="127" rx="2.4" ry="2" fill="var(--zs-mascot-ink)" />
          {eyes}
          {mouth}
          {/* blush */}
          <ellipse cx="66" cy="124" rx="9" ry="5" fill="var(--zs-primary)" opacity="0.35" />
          <ellipse cx="134" cy="124" rx="9" ry="5" fill="var(--zs-primary)" opacity="0.35" />
          {/* bow */}
          <g transform="translate(136 50) rotate(18)">
            <path d="M0 0l-18-10v20z" fill="var(--zs-primary)" stroke="var(--zs-mascot-ink)" stroke-width="2" stroke-linejoin="round" />
            <path d="M0 0l18-10v20z" fill="var(--zs-primary)" stroke="var(--zs-mascot-ink)" stroke-width="2" stroke-linejoin="round" />
            <circle r="5" fill="var(--zs-gold)" stroke="var(--zs-mascot-ink)" stroke-width="2" />
          </g>
          {/* sparkles */}
          <path d="M168 30l3 8l8 3l-8 3l-3 8l-3-8l-8-3l8-3z" fill="var(--zs-gold)" class="zs-sparkle" />
          <path d="M28 132l2 5l5 2l-5 2l-2 5l-2-5l-5-2l5-2z" fill="var(--zs-accent)" class="zs-sparkle" />
        </svg>
      )
    }
  },
})

export default Mascot
