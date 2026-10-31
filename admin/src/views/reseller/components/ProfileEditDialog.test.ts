import { expect, it } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import i18n from '@/i18n'
import type { AdminResellerProfile } from '@/api/types'
import { ProfileEditDialog } from './ProfileEditDialog'

const DialogStub = defineComponent({ setup(_, { slots }) { return () => h('div', [slots.default?.(), slots.footer?.()]) } })

const profile: AdminResellerProfile = {
  id: 2, user_id: 9, status: 'active', default_markup_percent: '5.00', max_markup_percent: '20.00',
  settlement_status: 'normal', created_at: '', updated_at: '', tags: ['张三', '朋友'], admin_note: '每月结算\n周五核对',
}

it('prefills, saves and clears metadata separately from the audit reason', async () => {
  const view = mount(ProfileEditDialog, { props: { modelValue: true, profile }, global: { plugins: [i18n], stubs: { ZsDialog: DialogStub } } })
  expect(view.get('input').element.value).toBe('张三, 朋友')
  expect((view.findAll('textarea')[0]!.element as HTMLTextAreaElement).value).toBe('每月结算\n周五核对')
  await view.get('input').setValue(' 李四，合作方,李四 ')
  await view.findAll('textarea')[0]!.setValue('新的约定\n按周结算')
  await view.findAll('textarea')[1]!.setValue('本次修改说明')
  await view.findAll('button').at(-1)!.trigger('click')
  expect(view.emitted('submit')?.at(-1)?.[0]).toMatchObject({ tags: ['李四', '合作方'], admin_note: '新的约定\n按周结算', reason: '本次修改说明' })
  await view.get('input').setValue('')
  await view.findAll('textarea')[0]!.setValue('')
  await view.findAll('button').at(-1)!.trigger('click')
  expect(view.emitted('submit')?.at(-1)?.[0]).toMatchObject({ tags: [], admin_note: '' })
  // FE-21: opening another row resets metadata from the previous edit.
  await view.setProps({ modelValue: false })
  await view.setProps({ profile: { ...profile, id: 3, tags: [], admin_note: '' }, modelValue: true })
  expect(view.get('input').element.value).toBe('')
  expect((view.findAll('textarea')[0]!.element as HTMLTextAreaElement).value).toBe('')
  view.unmount()
})
