import { createApp } from 'vue'
import { createPinia } from 'pinia'
import './styles/theme.css'
import App from './App'
import i18n, { detectLocale, setI18nLocale } from './i18n'
import router from './router'
import { loadTelegramSdk } from './utils/auth/telegramSdk'

const app = createApp(App)
app.use(createPinia())
app.use(router)
app.use(i18n)

void Promise.all([setI18nLocale(detectLocale()), loadTelegramSdk()]).finally(() => {
  app.mount('#app')
})
