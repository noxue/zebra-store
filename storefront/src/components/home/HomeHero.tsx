import { defineComponent, Transition, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowRight, ChevronLeft, ChevronRight, Sparkles } from 'lucide-vue-next'
import type { useBannerCarousel } from '@/composables/useBannerCarousel'
import { useAppStore } from '@/stores/app'
import { Button, Mascot, cn } from '@/components/ui'

type Carousel = ReturnType<typeof useBannerCarousel>

/** Banner carousel; falls back to a mascot hero when no banners. */
export const HomeHero = defineComponent({
  name: 'HomeHero',
  props: { carousel: { type: Object as PropType<Carousel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const appStore = useAppStore()

    /** 看板娘: `theme.mascot_image` or the built-in SVG (handled by <Mascot>). */
    const heroMascot = (cls: string) => (
      <div class={cn('pointer-events-none', cls)} aria-hidden="true">
        <div class="zs-hero-mascot-halo absolute inset-x-[-10%] bottom-0 top-[15%] rounded-full" />
        <div class="zs-hero-mascot relative size-full">
          <Mascot float={false} />
        </div>
      </div>
    )

    const mascotHero = () => (
      <div class="zs-card relative overflow-hidden p-0">
        {appStore.heroBackground ? (
          <img src={appStore.heroBackground} alt="" class="absolute inset-0 size-full object-cover opacity-40" />
        ) : (
          <div class="zs-soft-bg absolute inset-0" />
        )}
        <div class="pointer-events-none absolute -left-10 top-10 size-40 rounded-full bg-primary/25 blur-3xl zs-float" />
        <div class="pointer-events-none absolute right-1/3 -top-10 size-48 rounded-full bg-secondary/25 blur-3xl zs-float" style={{ animationDelay: '1.5s' }} />
        <div class="pointer-events-none absolute bottom-0 right-10 size-40 rounded-full bg-accent/25 blur-3xl zs-float" style={{ animationDelay: '3s' }} />
        <div class="relative grid items-center gap-6 px-6 py-10 sm:px-12 md:grid-cols-[1.3fr_1fr] md:py-14">
          <div class="space-y-5">
            <span class="inline-flex items-center gap-2 rounded-full border border-line bg-surface-strong px-3 py-1 text-xs font-bold text-primary-text">
              <Sparkles class="size-3.5" />
              {t('home.hero.badge')}
            </span>
            <h1 class="zs-title text-4xl leading-tight sm:text-5xl">
              <span class="zs-gradient-text">{appStore.siteName}</span>
            </h1>
            <p class="max-w-md text-base text-muted">{t('zs.mascotHello')}</p>
            <div class="flex flex-wrap gap-3">
              <Button size="lg" to="/products">
                {t('home.hero.cta')}
                <ArrowRight class="size-4" />
              </Button>
            </div>
          </div>
          {heroMascot('relative mx-auto h-40 w-32 sm:h-56 sm:w-48 md:mr-0 md:h-72 md:w-60 lg:h-80 lg:w-64')}
        </div>
      </div>
    )

    return () => {
      const c = props.carousel
      if (c.bannerLoading.value) return <div class="zs-skeleton aspect-[16/9] w-full rounded-zs-lg sm:aspect-[21/8]" />
      if (c.bannerCount.value === 0) return mascotHero()
      const cur = c.current.value
      return (
        <section
          class="group relative flex min-h-[22rem] flex-col overflow-hidden rounded-zs-lg border border-line shadow-zs-lg sm:aspect-[21/8] sm:min-h-0"
          onTouchstart={c.onTouchStart}
          onTouchend={c.onTouchEnd}
        >
          <Transition name="zs-fade" mode="out-in">
            {cur && <img key={cur.id} src={c.imageOf(cur)} alt={c.titleOf(cur)} class="absolute inset-0 size-full object-cover" />}
          </Transition>
          <div class="absolute inset-0 bg-gradient-to-r from-black/65 via-black/30 to-transparent" />
          {heroMascot('absolute bottom-0 right-[6%] hidden aspect-[5/6] h-[88%] md:block')}
          <div class="relative flex flex-1 flex-col justify-end gap-4 p-6 pb-10 sm:justify-center sm:p-12 md:pr-[38%] lg:pr-[34%]">
            <span class="inline-flex w-fit items-center gap-2 rounded-full border border-white/30 bg-white/15 px-3 py-1 text-xs font-bold text-white backdrop-blur">
              <span class="size-1.5 rounded-full bg-success" />
              {c.titleOf(cur)}
            </span>
            <h1 class="zs-title max-w-2xl text-3xl leading-tight text-white drop-shadow sm:text-5xl">{c.titleOf(cur)}</h1>
            <p class="max-w-xl text-sm text-white/85 sm:text-lg">{c.subtitleOf(cur)}</p>
            <div class="flex flex-wrap gap-3">
              <Button size="lg" onClick={c.goToHeroLink}>
                {c.primaryButtonText.value}
                <ArrowRight class="size-4" />
              </Button>
              <Button size="lg" variant="ghost" class="border border-white/40 text-white! hover:bg-white/15!" to="/products">
                {t('home.featured.viewAll')}
              </Button>
            </div>
          </div>
          {c.bannerCount.value > 1 && (
            <>
              <button
                type="button"
                aria-label={t('common.previousBanner')}
                class="absolute left-3 top-1/2 flex size-10 -translate-y-1/2 items-center justify-center rounded-full bg-white/20 text-white opacity-0 backdrop-blur transition group-hover:opacity-100 hover:bg-white/35"
                onClick={c.prev}
              >
                <ChevronLeft class="size-5" />
              </button>
              <button
                type="button"
                aria-label={t('common.nextBanner')}
                class="absolute right-3 top-1/2 flex size-10 -translate-y-1/2 items-center justify-center rounded-full bg-white/20 text-white opacity-0 backdrop-blur transition group-hover:opacity-100 hover:bg-white/35"
                onClick={c.next}
              >
                <ChevronRight class="size-5" />
              </button>
              <div class="absolute bottom-4 right-6 flex gap-2">
                {c.banners.value.map((b, i) => (
                  <button
                    key={b.id}
                    type="button"
                    aria-label={t('common.switchBanner', { n: i + 1 })}
                    class={cn('h-2 rounded-full transition-all', i === c.currentIndex.value ? 'w-7 bg-white' : 'w-2 bg-white/50')}
                    onClick={() => c.select(i)}
                  />
                ))}
              </div>
            </>
          )}
        </section>
      )
    }
  },
})
