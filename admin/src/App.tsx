import { defineComponent } from 'vue'
import { RouterView } from 'vue-router'
import { ConfirmHost, NoticeHost } from '@/components/ui'

export default defineComponent({
  name: 'App',
  setup() {
    return () => (
      <>
        <RouterView />
        <NoticeHost />
        <ConfirmHost />
      </>
    )
  },
})
