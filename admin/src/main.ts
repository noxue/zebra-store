import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App'
import router from './router'
import i18n from './i18n'
import { useAppStore } from './stores/app'
import './styles/theme.css'

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)
app.use(i18n)
app.use(router)
useAppStore(pinia).initTheme()
document.documentElement.lang = i18n.global.locale.value
app.mount('#app')
