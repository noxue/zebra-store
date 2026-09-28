import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { PageHeader } from '@/components/ui'
import { useAuthz } from './authz/useAuthz'
import { AuthzAdminsSection } from './authz/AuthzAdminsSection'
import { AuthzRolesSection } from './authz/AuthzRolesSection'
import { AuthzPoliciesSection } from './authz/AuthzPoliciesSection'
import { AuthzAdminRolesSection } from './authz/AuthzAdminRolesSection'

export default defineComponent({
  name: 'AuthzView',
  setup() {
    const { t } = useI18n()
    const p = useAuthz()
    onMounted(() => void p.init())

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.authz.title')} subtitle={t('admin.authz.subtitle')} />
        <AuthzAdminsSection p={p} />
        <div class="grid grid-cols-1 gap-6 xl:grid-cols-3">
          <AuthzRolesSection p={p} />
          <div class="min-w-0 xl:col-span-2">
            <AuthzPoliciesSection p={p} />
          </div>
        </div>
        <AuthzAdminRolesSection p={p} />
      </div>
    )
  },
})
