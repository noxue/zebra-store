import { defineComponent } from 'vue'

const Petal = () => (
  <svg class="zs-loader-petal h-4 w-4" viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M12 2c2 3 5 5 5 9a5 5 0 0 1-10 0c0-4 3-6 5-9z"
      fill="var(--zs-primary)"
      opacity="0.9"
    />
  </svg>
)

/** Bouncing sakura-petal loader. */
export const Loader = defineComponent({
  name: 'ZsLoader',
  props: { label: String },
  setup(props) {
    return () => (
      <div class="flex flex-col items-center justify-center gap-2 py-8 text-xs text-muted" role="status">
        <div class="flex items-end gap-1.5">
          <Petal />
          <Petal />
          <Petal />
        </div>
        {props.label && <span>{props.label}</span>}
      </div>
    )
  },
})

export default Loader
