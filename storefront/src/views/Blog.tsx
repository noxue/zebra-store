import { defineComponent } from 'vue'
import { PostListPage } from '@/components/content/PostListPage'

export default defineComponent({
  name: 'BlogView',
  setup() {
    return () => <PostListPage type="blog" />
  },
})
