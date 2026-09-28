import { computed, defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, CheckCircle2, Info, Megaphone } from 'lucide-vue-next'
import type { AnnouncementConfig } from '@/api/types'
import { RichContent } from '@/components/common/RichContent'
import { useAnnouncement } from '@/composables/useAnnouncement'
import { useLocalized } from '@/composables/useLocalized'
import { Button, Modal, cn } from '@/components/ui'

/** Home announcement dialog with session / today / forever dismissal. */
export const AnnouncementModal = defineComponent({
  name: 'AnnouncementModal',
  props: {
    announcement: { type: Object as PropType<AnnouncementConfig>, required: true },
    open: Boolean,
  },
  emits: { close: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const { getLocalizedText } = useLocalized()
    const { dismissForever, dismissToday, closeForSession, versionOf } = useAnnouncement()
    const version = computed(() => versionOf(props.announcement))
    const style = computed(() => {
      switch (props.announcement.type) {
        case 'warning':
          return { cls: 'bg-warning-soft text-warning-text', icon: AlertTriangle }
        case 'info':
          return { cls: 'bg-accent-soft text-accent-text', icon: Info }
        case 'success':
          return { cls: 'bg-success-soft text-success-text', icon: CheckCircle2 }
        default:
          return { cls: 'bg-primary-soft text-primary-text', icon: Megaphone }
      }
    })
    const closeSession = () => {
      closeForSession(version.value)
      emit('close')
    }
    return () => {
      const Icon = style.value.icon
      return (
        <Modal open={props.open} size="md" onClose={closeSession}>
          {{
            header: () => (
              <div class="flex items-center gap-3">
                <span class={cn('flex size-11 shrink-0 items-center justify-center rounded-zs', style.value.cls)}>
                  <Icon class="size-5" />
                </span>
                <h3 class="zs-title line-clamp-2 text-xl text-fg">{getLocalizedText(props.announcement.title)}</h3>
              </div>
            ),
            default: () => <RichContent html={getLocalizedText(props.announcement.content)} />,
            footer: () => (
              <div class="flex w-full flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div class="flex items-center justify-center gap-3 text-xs sm:justify-start">
                  <button
                    type="button"
                    class="text-muted hover:text-primary-text"
                    onClick={() => {
                      dismissToday(version.value)
                      emit('close')
                    }}
                  >
                    {t('announcement.dismissToday')}
                  </button>
                  <span class="text-muted opacity-40">·</span>
                  <button
                    type="button"
                    class="text-muted hover:text-primary-text"
                    onClick={() => {
                      dismissForever(version.value)
                      emit('close')
                    }}
                  >
                    {t('announcement.dismissForever')}
                  </button>
                </div>
                <Button onClick={closeSession}>{t('announcement.close')}</Button>
              </div>
            ),
          }}
        </Modal>
      )
    }
  },
})
