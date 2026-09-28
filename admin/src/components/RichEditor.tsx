import { defineComponent, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { EditorContent, useEditor } from '@tiptap/vue-3'
import StarterKit from '@tiptap/starter-kit'
import Image from '@tiptap/extension-image'
import Link from '@tiptap/extension-link'
import Placeholder from '@tiptap/extension-placeholder'
import TextAlign from '@tiptap/extension-text-align'
import TextStyle from '@tiptap/extension-text-style'
import Color from '@tiptap/extension-color'
import Table from '@tiptap/extension-table'
import TableRow from '@tiptap/extension-table-row'
import TableCell from '@tiptap/extension-table-cell'
import TableHeader from '@tiptap/extension-table-header'
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  Bold,
  Code2,
  Heading1,
  Heading2,
  Heading3,
  ImagePlus,
  Italic,
  Link2,
  List,
  ListOrdered,
  Quote,
  Redo2,
  Strikethrough,
  Table2,
  Undo2,
} from 'lucide-vue-next'
import { processHtmlForDisplay, processHtmlForStorage } from '@/utils/content'
import { getImageUrl } from '@/utils/image'
import { MediaPicker } from './MediaPicker'
import { cn, type IconComponent } from './ui'

const COLORS = ['#2d2445', '#ff5fa2', '#8b5cf6', '#38bdf8', '#f43f5e', '#fb923c', '#34d399']

/** Tiptap rich text editor (HTML string v-model) with media-library image insertion. */
export const RichEditor = defineComponent({
  name: 'RichEditor',
  props: {
    modelValue: { type: String, default: '' },
    placeholder: String,
    minHeight: { type: String, default: '200px' },
    scene: { type: String, default: 'editor' },
  },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const sourceMode = ref(false)
    const source = ref('')
    const pickerOpen = ref(false)
    let lastEmitted = props.modelValue

    const editor = useEditor({
      content: processHtmlForDisplay(props.modelValue || ''),
      extensions: [
        StarterKit,
        Image,
        Link.configure({ openOnClick: false }),
        Placeholder.configure({ placeholder: () => props.placeholder || t('admin.richEditor.placeholder') }),
        TextAlign.configure({ types: ['heading', 'paragraph'] }),
        TextStyle,
        Color,
        Table.configure({ resizable: false }),
        TableRow,
        TableHeader,
        TableCell,
      ],
      onUpdate: ({ editor: ed }) => {
        const html = ed.isEmpty ? '' : processHtmlForStorage(ed.getHTML())
        lastEmitted = html
        emit('update:modelValue', html)
      },
    })

    watch(
      () => props.modelValue,
      (value) => {
        if (value === lastEmitted) return
        lastEmitted = value
        editor.value?.commands.setContent(processHtmlForDisplay(value || ''), false)
      },
    )

    onBeforeUnmount(() => editor.value?.destroy())

    const toggleSource = () => {
      if (!sourceMode.value) {
        source.value = props.modelValue || ''
      } else {
        lastEmitted = source.value
        emit('update:modelValue', source.value)
        editor.value?.commands.setContent(processHtmlForDisplay(source.value), false)
      }
      sourceMode.value = !sourceMode.value
    }

    const setLink = () => {
      const url = window.prompt(t('admin.richEditor.enterLinkUrl'))
      if (url === null) return
      if (!url) editor.value?.chain().focus().unsetLink().run()
      else editor.value?.chain().focus().setLink({ href: url }).run()
    }

    const insertImages = (urls: string[]) => {
      urls.forEach((u) => editor.value?.chain().focus().setImage({ src: getImageUrl(u) }).run())
    }

    const btn = (icon: IconComponent, title: string, action: () => void, active = false) => {
      const Icon = icon
      return (
        <button
          type="button"
          title={title}
          onMousedown={(e: MouseEvent) => e.preventDefault()}
          onClick={action}
          class={cn(
            'rounded-lg p-1.5 transition-colors',
            active ? 'bg-primary text-on-primary' : 'text-muted hover:bg-primary-soft hover:text-primary',
          )}
        >
          <Icon class="h-4 w-4" />
        </button>
      )
    }

    return () => {
      const ed = editor.value
      const c = () => ed?.chain().focus()
      return (
        <div class="overflow-hidden rounded-zs border border-line-strong bg-surface-strong">
          <div class="flex flex-wrap items-center gap-0.5 border-b border-line bg-primary-soft/40 px-2 py-1.5">
            {btn(Bold, t('admin.richEditor.bold'), () => c()?.toggleBold().run(), !!ed?.isActive('bold'))}
            {btn(Italic, t('admin.richEditor.italic'), () => c()?.toggleItalic().run(), !!ed?.isActive('italic'))}
            {btn(Strikethrough, t('admin.richEditor.strikethrough'), () => c()?.toggleStrike().run(), !!ed?.isActive('strike'))}
            <span class="mx-1 h-4 w-px bg-line-strong" />
            {btn(Heading1, t('admin.richEditor.heading1'), () => c()?.toggleHeading({ level: 1 }).run(), !!ed?.isActive('heading', { level: 1 }))}
            {btn(Heading2, t('admin.richEditor.heading2'), () => c()?.toggleHeading({ level: 2 }).run(), !!ed?.isActive('heading', { level: 2 }))}
            {btn(Heading3, t('admin.richEditor.heading3'), () => c()?.toggleHeading({ level: 3 }).run(), !!ed?.isActive('heading', { level: 3 }))}
            <span class="mx-1 h-4 w-px bg-line-strong" />
            {btn(List, t('admin.richEditor.bulletList'), () => c()?.toggleBulletList().run(), !!ed?.isActive('bulletList'))}
            {btn(ListOrdered, t('admin.richEditor.orderedList'), () => c()?.toggleOrderedList().run(), !!ed?.isActive('orderedList'))}
            {btn(Quote, t('admin.richEditor.blockquote'), () => c()?.toggleBlockquote().run(), !!ed?.isActive('blockquote'))}
            <span class="mx-1 h-4 w-px bg-line-strong" />
            {btn(AlignLeft, t('admin.richEditor.alignLeft'), () => c()?.setTextAlign('left').run(), !!ed?.isActive({ textAlign: 'left' }))}
            {btn(AlignCenter, t('admin.richEditor.alignCenter'), () => c()?.setTextAlign('center').run(), !!ed?.isActive({ textAlign: 'center' }))}
            {btn(AlignRight, t('admin.richEditor.alignRight'), () => c()?.setTextAlign('right').run(), !!ed?.isActive({ textAlign: 'right' }))}
            <span class="mx-1 h-4 w-px bg-line-strong" />
            {btn(Link2, t('admin.richEditor.insertLink'), setLink, !!ed?.isActive('link'))}
            {btn(ImagePlus, t('admin.richEditor.uploadImage'), () => (pickerOpen.value = true))}
            {btn(Table2, t('admin.richEditor.insertTable'), () => c()?.insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run())}
            <span class="mx-1 flex items-center gap-0.5" title={t('admin.richEditor.textColor')}>
              {COLORS.map((color) => (
                <button
                  type="button"
                  key={color}
                  onMousedown={(e: MouseEvent) => e.preventDefault()}
                  onClick={() => c()?.setColor(color).run()}
                  class="h-4 w-4 rounded-full border border-line"
                  style={{ background: color }}
                  aria-label={color}
                />
              ))}
            </span>
            <span class="mx-1 h-4 w-px bg-line-strong" />
            {btn(Undo2, t('admin.richEditor.undo'), () => c()?.undo().run())}
            {btn(Redo2, t('admin.richEditor.redo'), () => c()?.redo().run())}
            {btn(Code2, t('admin.richEditor.sourceMode'), toggleSource, sourceMode.value)}
          </div>
          {ed?.isActive('table') && (
            <div class="flex flex-wrap gap-1 border-b border-line px-2 py-1 text-xs">
              {(
                [
                  ['addColumn', () => c()?.addColumnAfter().run()],
                  ['deleteColumn', () => c()?.deleteColumn().run()],
                  ['addRow', () => c()?.addRowAfter().run()],
                  ['deleteRow', () => c()?.deleteRow().run()],
                  ['mergeCells', () => c()?.mergeOrSplit().run()],
                  ['deleteTable', () => c()?.deleteTable().run()],
                ] as const
              ).map(([key, fn]) => (
                <button type="button" key={key} class="rounded-full px-2 py-0.5 text-muted hover:bg-primary-soft hover:text-primary" onClick={fn}>
                  {t(`admin.richEditor.${key}`)}
                </button>
              ))}
            </div>
          )}
          {sourceMode.value ? (
            <textarea
              value={source.value}
              onInput={(e: Event) => (source.value = (e.target as HTMLTextAreaElement).value)}
              class="block w-full resize-y bg-transparent p-3 font-mono text-xs text-fg focus:outline-none"
              style={{ minHeight: props.minHeight }}
            />
          ) : (
            <div class="zs-prose px-4 py-3 text-sm" style={{ minHeight: props.minHeight }}>
              {ed && <EditorContent editor={ed} />}
            </div>
          )}
          <MediaPicker dialogOnly multiple scene={props.scene} open={pickerOpen.value} onUpdate:open={(v: boolean) => (pickerOpen.value = v)} onPick={insertImages} />
        </div>
      )
    }
  },
})

export default RichEditor
