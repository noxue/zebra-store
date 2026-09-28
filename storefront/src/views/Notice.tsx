import { defineComponent } from 'vue'
import { PostListPage } from '@/components/content/PostListPage'

export default defineComponent({
  name: 'NoticeView',
  setup() {
    return () => <PostListPage type="notice" />
  },
})
