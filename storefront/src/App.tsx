import { computed, defineComponent, Transition, type VNode } from 'vue'
import { RouterView, useRoute } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { SakuraCanvas } from '@/components/ui'
import { MobileBottomNav } from '@/components/layout/MobileBottomNav'
import { Navbar } from '@/components/layout/Navbar'
import { BackToTop, ConfirmHost, LoadingOverlay, ToastHost } from '@/components/layout/Overlays'
import { SiteFooter } from '@/components/layout/SiteFooter'

export default defineComponent({
  name: 'App',
  setup() {
    const route = useRoute()
    const appStore = useAppStore()
    const isConsole = computed(() => route.meta.resellerConsole === true)
    const view = () => (
      <RouterView>
        {{
          default: ({ Component }: { Component: VNode | undefined }) => (
            <Transition name="page-fade" mode="out-in">
              {Component}
            </Transition>
          ),
        }}
      </RouterView>
    )
    return () => (
      <div class="relative flex min-h-screen flex-col">
        {appStore.sakuraEnabled && !isConsole.value && <SakuraCanvas />}
        {isConsole.value ? (
          <main class="relative z-[2] flex-1">{view()}</main>
        ) : (
          <>
            <Navbar />
            <main class="relative z-[2] flex-1 pb-20 lg:pb-0">{view()}</main>
            <div class="relative z-[2]">
              <SiteFooter />
            </div>
            <BackToTop />
            <MobileBottomNav />
          </>
        )}
        <LoadingOverlay loading={appStore.loading || appStore.navigating} />
        <ToastHost />
        <ConfirmHost />
      </div>
    )
  },
})
