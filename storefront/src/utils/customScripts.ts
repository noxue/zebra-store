/**
 * Admin-configured custom scripts (`site_config.scripts`), port of the original
 * `utils/customScripts.ts`.
 *
 * - Plain JavaScript (no `<`) is wrapped in a new `<script>` element so it runs
 *   (QA-A08: it used to be appended as a text node and never executed).
 * - HTML snippets are parsed; `<script>` tags are re-created as live elements
 *   (parsed/innerHTML scripts never execute), other elements are cloned.
 * - Every node is tagged as managed; a changed script list clears the old
 *   nodes before applying the new ones.
 */

type ScriptPosition = 'head' | 'body_end'

interface NormalizedScript {
  name: string
  position: ScriptPosition
  code: string
}

const MANAGED_ATTR = 'data-site-script-managed'
const NAME_ATTR = 'data-site-script-name'
const GROUP_ATTR = 'data-site-script-group'

let appliedSignature: string | null = null

const normalizePosition = (raw: unknown): ScriptPosition => (raw === 'body_end' ? 'body_end' : 'head')

const normalizeEnabled = (raw: unknown): boolean => {
  if (typeof raw === 'boolean') return raw
  if (typeof raw === 'number') return raw !== 0
  if (typeof raw === 'string') return ['1', 'true', 'yes', 'on'].includes(raw.trim().toLowerCase())
  return false
}

export const normalizeCustomScripts = (raw: unknown): NormalizedScript[] => {
  if (!Array.isArray(raw)) return []
  const out: NormalizedScript[] = []
  for (const item of raw as unknown[]) {
    if (!item || typeof item !== 'object') continue
    const value = item as Record<string, unknown>
    const code = typeof value.code === 'string' ? value.code.trim() : ''
    if (!code || !normalizeEnabled(value.enabled)) continue
    out.push({
      name: typeof value.name === 'string' ? value.name.trim() : '',
      position: normalizePosition(value.position),
      code,
    })
  }
  return out
}

const tag = (el: Element, name: string, group: string) => {
  el.setAttribute(MANAGED_ATTR, '1')
  el.setAttribute(NAME_ATTR, name)
  el.setAttribute(GROUP_ATTR, group)
}

const scriptNode = (code: string, name: string, group: string): HTMLScriptElement => {
  const node = document.createElement('script')
  node.type = 'text/javascript'
  node.text = code
  tag(node, name, group)
  return node
}

const nodesFromHTML = (snippet: string, name: string, group: string): Element[] => {
  const template = document.createElement('template')
  template.innerHTML = snippet
  const nodes: Element[] = []
  template.content.childNodes.forEach((child) => {
    if (child.nodeType !== Node.ELEMENT_NODE) return
    const element = child as Element
    if (element.tagName.toLowerCase() === 'script') {
      const live = document.createElement('script')
      Array.from(element.attributes).forEach((attr) => live.setAttribute(attr.name, attr.value))
      live.text = element.textContent || ''
      tag(live, name, group)
      nodes.push(live)
      return
    }
    const clone = element.cloneNode(true) as Element
    tag(clone, name, group)
    nodes.push(clone)
  })
  return nodes
}

/** Removes every managed script/element. */
export const clearCustomScripts = () => {
  if (typeof document === 'undefined') return
  document.querySelectorAll(`[${MANAGED_ATTR}="1"]`).forEach((node) => node.parentElement?.removeChild(node))
  appliedSignature = null
}

/** Applies the configured scripts; an unchanged list is not re-executed. */
export const applyCustomScripts = (raw: unknown) => {
  if (typeof document === 'undefined') return
  const scripts = normalizeCustomScripts(raw)
  const signature = JSON.stringify(scripts)
  if (signature === appliedSignature) return
  clearCustomScripts()
  scripts.forEach((item, index) => {
    const name = item.name || `script-${index + 1}`
    const group = `group-${index + 1}`
    const target = item.position === 'body_end' && document.body ? document.body : document.head
    if (!item.code.includes('<')) {
      target.appendChild(scriptNode(item.code, name, group))
      return
    }
    const nodes = nodesFromHTML(item.code, name, group)
    if (nodes.length === 0) {
      target.appendChild(scriptNode(item.code, name, group))
      return
    }
    nodes.forEach((node) => target.appendChild(node))
  })
  appliedSignature = signature
}
