import { defineComponent, h, nextTick, ref } from 'vue'
import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import CardConverterExtraEditor from './CardConverterExtraEditor'
import { defaultExtraRule } from './cardConverterExtra'

const fields = [{ key: 'provider', label: '上游来源', required: true, kind: 'select', options: ['p1', 'p2'] }]

describe('converter extra form and JSON modes', () => {
  it('synchronizes advanced JSON back into the visual form and keeps placeholders valid JSON', async () => {
    const extra = ref<Record<string, unknown>>({ provider: 'p1' })
    const Host = defineComponent({
      setup: () => () => h(CardConverterExtraEditor, {
        modelValue: extra.value,
        fields,
        'onUpdate:modelValue': (value: Record<string, unknown>) => { extra.value = value },
      }),
    })
    const wrapper = mount(Host)
    await wrapper.findAll('button').find((button) => button.text() === '高级 JSON')!.trigger('click')
    const json = wrapper.get('textarea[aria-label="extra JSON"]')
    await json.setValue('{"provider":"p2","note":"{{order.order_no}}","nested":{"site":"{{site.domain}}"}}')
    expect(extra.value).toEqual({ provider: 'p2', note: '{{order.order_no}}', nested: { site: '{{site.domain}}' } })
    await wrapper.findAll('button').find((button) => button.text() === '普通模式')!.trigger('click')
    expect(wrapper.find('textarea[aria-label="extra JSON"]').exists()).toBe(false)
    expect(wrapper.text()).toContain('note')
    expect(wrapper.text()).toContain('nested')
    await wrapper.findAll('button').find((button) => button.text() === '高级 JSON')!.trigger('click')
    expect((wrapper.get('textarea[aria-label="extra JSON"]').element as HTMLTextAreaElement).value).toContain('"provider": "p2"')
  })

  it('keeps malformed advanced JSON in the editor instead of replacing the form data', async () => {
    const extra = ref<Record<string, unknown>>({ provider: 'p1' })
    const wrapper = mount(defineComponent({ setup: () => () => h(CardConverterExtraEditor, {
      modelValue: extra.value,
      fields,
      'onUpdate:modelValue': (value: Record<string, unknown>) => { extra.value = value },
    }) }))
    await wrapper.findAll('button').find((button) => button.text() === '高级 JSON')!.trigger('click')
    await wrapper.get('textarea[aria-label="extra JSON"]').setValue('{"provider":')
    await wrapper.findAll('button').find((button) => button.text() === '普通模式')!.trigger('click')
    expect(wrapper.find('textarea[aria-label="extra JSON"]').exists()).toBe(true)
    expect(wrapper.find('[role="alert"]').exists()).toBe(true)
    expect(extra.value).toEqual({ provider: 'p1' })
  })

  it('inserts a selected order variable as a JSON-safe string placeholder', async () => {
    const extra = ref<Record<string, unknown>>({ provider: 'p1', note: 'Order: ' })
    const wrapper = mount(defineComponent({ setup: () => () => h(CardConverterExtraEditor, {
      modelValue: extra.value,
      fields,
      'onUpdate:modelValue': (value: Record<string, unknown>) => { extra.value = value },
    }) }))
    const variables = wrapper.findAll('select').find((select) => select.text().includes('订单号'))!
    await variables.setValue('0')
    expect(extra.value.note).toBe('Order: {{order.order_no}}')
    await wrapper.findAll('button').find((button) => button.text() === '高级 JSON')!.trigger('click')
    const json = (wrapper.get('textarea[aria-label="extra JSON"]').element as HTMLTextAreaElement).value
    expect(JSON.parse(json)).toEqual({ provider: 'p1', note: 'Order: {{order.order_no}}' })
  })

  it('shows an advanced conditional rule as editable form controls', async () => {
    const extra = ref<Record<string, unknown>>({})
    const wrapper = mount(defineComponent({ setup: () => () => h(CardConverterExtraEditor, {
      modelValue: extra.value,
      'onUpdate:modelValue': (value: Record<string, unknown>) => { extra.value = value },
    }) }))
    await wrapper.findAll('button').find((button) => button.text() === '高级 JSON')!.trigger('click')
    await wrapper.get('textarea[aria-label="extra JSON"]').setValue(JSON.stringify({ route: { $zs_rule: {
      cases: [{ when: { variable: 'source.name', operator: 'eq', value: 'A' }, then: { literal: 'p1' } }],
      else: { variable: 'source.id' },
    } } }))
    await wrapper.findAll('button').find((button) => button.text() === '普通模式')!.trigger('click')
    expect(wrapper.text()).toContain('如果')
    expect(wrapper.text()).toContain('否则输出')
    expect(wrapper.find('textarea[aria-label="route JSON"]').exists()).toBe(false)
    expect(extra.value.route).toEqual({ $zs_rule: {
      cases: [{ when: { variable: 'source.name', operator: 'eq', value: 'A' }, then: { literal: 'p1' } }],
      else: { variable: 'source.id' },
    } })
  })

  it('shows upstream names while storing their stable ID and a separate converter alias', async () => {
    const extra = ref<Record<string, unknown>>({ channel: defaultExtraRule() })
    const wrapper = mount(defineComponent({ setup: () => () => h(CardConverterExtraEditor, {
      modelValue: extra.value,
      sources: [{ id: 7, name: '渠道1' }, { id: 8, name: '渠道2' }],
      'onUpdate:modelValue': (value: Record<string, unknown>) => { extra.value = value },
    }) }))
    const sourceSelect = wrapper.findAll('select').find((select) => select.text().includes('渠道1'))!
    expect(sourceSelect.text()).toContain('渠道1（ID 7）')
    await sourceSelect.setValue('0')
    const aliasInput = wrapper.findAll('input').find((input) => input.attributes('placeholder') === '传给转换器的值，如 p1')!
    await aliasInput.setValue('p1')
    expect((extra.value.channel as ReturnType<typeof defaultExtraRule>).$zs_rule.cases[0]).toEqual({
      when: { variable: 'source.id', operator: 'eq', value: '7' },
      then: { literal: 'p1' },
    })
    expect(wrapper.text()).toContain('发送给转换器')
  })

  it('keeps the number control when cleared and requires a valid numeric value', async () => {
    const extra = ref<Record<string, unknown>>({ quantity: 2 })
    const wrapper = mount(defineComponent({ setup: () => () => h(CardConverterExtraEditor, {
      modelValue: extra.value,
      'onUpdate:modelValue': (value: Record<string, unknown>) => { extra.value = value },
    }) }))
    const kind = wrapper.findAll('select').find((select) => select.text().includes('条件规则'))!
    await kind.setValue('1')
    const number = wrapper.get('input[type="number"]')
    await number.setValue('')
    expect(wrapper.find('input[type="number"]').exists()).toBe(true)
    expect((wrapper.findComponent(CardConverterExtraEditor).vm as unknown as { validate: () => Record<string, unknown> | null }).validate()).toBeNull()
    await nextTick()
    expect(wrapper.text()).toContain('请填写「quantity」的数字值')
    await wrapper.get('input[type="number"]').setValue('42')
    expect(extra.value.quantity).toBe(42)
  })
})
