import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { CallbackStatus } from '@/components/auth/CallbackStatus'
import { useGoogleRedirectCallback } from '@/composables/useGoogleRedirectCallback'

export default defineComponent({
  name: 'GoogleCallbackView',
  setup() {
    const { t } = useI18n()
    const { loading, errMsg, retryPath } = useGoogleRedirectCallback()
    return () => (
      <CallbackStatus
        loading={loading.value}
        processingText={t('auth.googleCallback.processing')}
        error={errMsg.value}
        backText={t('auth.googleCallback.back')}
        backTo={retryPath.value}
      />
    )
  },
})
