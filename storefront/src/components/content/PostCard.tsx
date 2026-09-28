import { defineComponent, type PropType } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowRight, Bell, CalendarDays, ChevronRight, Newspaper } from 'lucide-vue-next'
import type { Post } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { Badge, SmartImage } from '@/components/ui'

/** Blog / notice card with optional thumbnail, date, title, summary. */
export const PostCard = defineComponent({
  name: 'PostCard',
  props: {
    post: { type: Object as PropType<Post>, required: true },
    date: { type: String, default: '' },
  },
  setup(props) {
    const { t } = useI18n()
    const { getLocalizedText } = useLocalized()
    return () => {
      const p = props.post
      const isNotice = p.type === 'notice'
      const summary = getLocalizedText(p.summary)
      return (
        <RouterLink to={`/blog/${p.slug}`} class="zs-card zs-card-hover group flex flex-col overflow-hidden">
          {p.thumbnail && (
            <div class="aspect-[16/9] overflow-hidden">
              <SmartImage src={p.thumbnail} alt={getLocalizedText(p.title)} imgClass="transition-transform duration-500 group-hover:scale-105" />
            </div>
          )}
          <div class="flex flex-1 flex-col p-5 sm:p-6">
            <div class="mb-3 flex items-center justify-between gap-2">
              <Badge tone={isNotice ? 'warning' : 'info'} size="xs">
                {isNotice ? <Bell class="size-3" /> : <Newspaper class="size-3" />}
                {isNotice ? t('nav.notice') : t('nav.blog')}
              </Badge>
              {props.date && (
                <span class="inline-flex items-center gap-1 text-xs text-muted zs-num">
                  <CalendarDays class="size-3.5" />
                  {props.date}
                </span>
              )}
            </div>
            <h3 class="zs-title line-clamp-2 text-lg text-fg transition group-hover:text-primary-text">{getLocalizedText(p.title)}</h3>
            {summary && <p class="mt-2 line-clamp-3 text-sm leading-relaxed text-muted">{summary}</p>}
            <div class="mt-auto pt-5">
              <div class="zs-divider mb-4" />
              <span class="inline-flex items-center gap-1.5 text-sm font-bold text-primary-text">
                {t('blog.readMore')}
                <ArrowRight class="size-4 transition-transform group-hover:translate-x-1" />
              </span>
            </div>
          </div>
        </RouterLink>
      )
    }
  },
})

/** Notice row (original notice page is a single-column list: bell icon, badge + date, title, summary, chevron). */
export const NoticeRow = defineComponent({
  name: 'NoticeRow',
  props: {
    post: { type: Object as PropType<Post>, required: true },
    date: { type: String, default: '' },
  },
  setup(props) {
    const { t } = useI18n()
    const { getLocalizedText } = useLocalized()
    return () => {
      const p = props.post
      const summary = getLocalizedText(p.summary)
      return (
        <RouterLink to={`/blog/${p.slug}`} class="zs-card zs-card-hover group flex items-center gap-4 p-5 sm:gap-5 sm:p-6">
          <span class="zs-gradient-bg flex size-12 shrink-0 items-center justify-center rounded-2xl text-on-primary shadow-zs sm:size-14">
            <Bell class="size-6" />
          </span>
          <div class="min-w-0 flex-1">
            <div class="mb-1.5 flex flex-wrap items-center gap-2">
              <Badge tone="warning" size="xs">
                {t('nav.notice')}
              </Badge>
              {props.date && <span class="text-xs text-muted zs-num">{props.date}</span>}
            </div>
            <h3 class="zs-title truncate text-lg text-fg transition group-hover:text-primary-text">{getLocalizedText(p.title)}</h3>
            {summary && <p class="mt-1 line-clamp-2 text-sm text-muted">{summary}</p>}
          </div>
          <ChevronRight class="size-5 shrink-0 text-muted transition-transform group-hover:translate-x-1" />
        </RouterLink>
      )
    }
  },
})
