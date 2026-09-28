import { expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { Checkbox } from './Checkbox'

it('FE-18 exposes mixed selection and permits selecting all by keyboard', async () => {
  const wrapper = mount(Checkbox, { props: { modelValue: false, indeterminate: true } })
  expect(wrapper.get('[role="checkbox"]').attributes('aria-checked')).toBe('mixed')
  await wrapper.get('[role="checkbox"]').trigger('keydown', { key: ' ' })
  expect(wrapper.emitted('update:modelValue')).toEqual([[true]])
  await wrapper.setProps({ modelValue: true, indeterminate: false })
  expect(wrapper.get('[role="checkbox"]').attributes('aria-checked')).toBe('true')
  await wrapper.setProps({ disabled: true })
  await wrapper.get('[role="checkbox"]').trigger('keydown', { key: 'Enter' })
  expect(wrapper.emitted('update:modelValue')).toHaveLength(1)
  wrapper.unmount()
})
