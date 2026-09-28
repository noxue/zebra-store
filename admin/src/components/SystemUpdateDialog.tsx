import { defineComponent, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Dialog, Loader, Mascot } from '@/components/ui'
import { useSystemUpdateInfo } from '@/composables/useSystemUpdateInfo'
import { useAppStore } from '@/stores/app'

/** Simplified system update dialog: version + capability (no self-update in Zebra Store). */
export const SystemUpdateDialog = defineComponent({
  name: 'SystemUpdateDialog',
  props: { open: Boolean },
  emits: { 'update:open': (_v: boolean) => true },
  setup(props, { emit }) {
    const { t, te } = useI18n()
    const app = useAppStore()
    const { loading, info, failed, load } = useSystemUpdateInfo(() => app.appVersion || '')
    watch(
      () => props.open,
      (open) => {
        if (open) void load()
      },
    )
    const reasonText = (reason: string) => {
      const key = `admin.systemUpdate.blocked.${reason}`
      return reason && reason !== 'source_build' && te(key) ? t(key) : t('admin.zebra.updateUnsupported')
    }
    return () => (
      <Dialog modelValue={props.open} onUpdate:modelValue={(v: boolean) => emit('update:open', v)} title={t('admin.zebra.systemVersionTitle')} description={t('admin.zebra.systemVersionDesc')} size="sm">
        {{
          default: () =>
            loading.value ? (
              <Loader />
            ) : (
              <div class="flex items-center gap-4">
                <Mascot size={72} mood="happy" />
                <div class="flex-1 space-y-3">
                  <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
                    <dt class="text-muted">{t('admin.updateCheck.currentLabel')}</dt>
                    <dd class="zs-num font-semibold text-fg">{info.value?.currentVersion || app.appVersion || t('admin.updateCheck.unknownVersion')}</dd>
                    {info.value
                      ? [
                          <dt key="latest-k" class="text-muted">
                            {t('admin.updateCheck.latestLabel')}
                          </dt>,
                          <dd key="latest-v" class="zs-num text-fg">
                            {info.value.latestVersion || '-'}
                          </dd>,
                        ]
                      : null}
                  </dl>
                  {info.value ? (
                    <p class="text-sm text-fg">{info.value.hasUpdate ? t('admin.updateCheck.hasUpdate', { version: info.value.latestVersion }) : t('admin.updateCheck.latest', { version: info.value.currentVersion })}</p>
                  ) : null}
                  {info.value && !info.value.canUpdate ? <p class="text-sm text-muted">{reasonText(info.value.blockReason)}</p> : null}
                  {failed.value ? <p class="text-sm text-muted">{t('admin.zebra.updateCheckFailed')}</p> : null}
                </div>
              </div>
            ),
          footer: () => <Button onClick={() => emit('update:open', false)}>{t('admin.updateCheck.close')}</Button>,
        }}
      </Dialog>
    )
  },
})

export default SystemUpdateDialog
