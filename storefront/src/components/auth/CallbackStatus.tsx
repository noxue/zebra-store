import { defineComponent } from 'vue'
import { Button, Kitty, PetalLoader } from '@/components/ui'

/** Shared processing / error card for OAuth callback pages. */
export const CallbackStatus = defineComponent({
  name: 'CallbackStatus',
  props: {
    loading: Boolean,
    processingText: { type: String, required: true },
    error: { type: String, default: '' },
    backText: { type: String, required: true },
    backTo: { type: String, required: true },
  },
  setup(props) {
    return () => (
      <div class="zs-page flex min-h-[60vh] items-center justify-center py-12">
        <div class="zs-card w-full max-w-md p-8 text-center">
          {props.loading ? (
            <PetalLoader label={props.processingText} />
          ) : (
            <div class="space-y-4">
              <div class="mx-auto h-24 w-28">
                <Kitty mood="sad" />
              </div>
              <p class="text-sm text-danger-text">{props.error}</p>
              <Button variant="secondary" to={props.backTo}>
                {props.backText}
              </Button>
            </div>
          )}
        </div>
      </div>
    )
  },
})
