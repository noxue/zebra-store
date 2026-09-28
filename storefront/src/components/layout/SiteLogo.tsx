import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useAppStore } from '@/stores/app'

/** Logo image (brand.site_logo) or gradient initial + site name. */
export const SiteLogo = defineComponent({
  name: 'SiteLogo',
  props: { compact: Boolean },
  setup(props) {
    const appStore = useAppStore()
    return () => (
      <RouterLink to="/" class="group flex min-w-0 items-center gap-2.5">
        {appStore.siteLogo ? (
          <img src={appStore.siteLogo} alt={appStore.siteName} class="h-9 w-auto max-w-[140px] object-contain" />
        ) : (
          <span class="zs-gradient-bg flex size-9 shrink-0 items-center justify-center rounded-zs-sm text-lg text-on-primary shadow-zs transition-transform group-hover:rotate-[-8deg] zs-title">
            {appStore.siteName.slice(0, 1).toUpperCase()}
          </span>
        )}
        {!props.compact && <span class="zs-title truncate text-xl zs-gradient-text">{appStore.siteName}</span>}
      </RouterLink>
    )
  },
})
