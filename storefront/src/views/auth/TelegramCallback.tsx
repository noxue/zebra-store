import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { CallbackStatus } from '@/components/auth/CallbackStatus'
import { useTelegramCallback } from '@/composables/useTelegramCallback'

export default defineComponent({
  name: 'TelegramCallbackView',
  setup() {
    const { t } = useI18n()
    const { loading, errMsg, retryPath } = useTelegramCallback()
    return () => (
      <CallbackStatus
        loading={loading.value}
        processingText={t('auth.telegramCallback.processing')}
        error={errMsg.value}
        backText={t('auth.telegramCallback.backToLogin')}
        backTo={retryPath.value}
      />
    )
  },
})
