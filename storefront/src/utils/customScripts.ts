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
  scope: 'global' | 'main' | 'reseller'
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
      scope: value.scope === 'global' || value.scope === 'reseller' ? value.scope : 'main',
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
const resellerFrame = (scripts: NormalizedScript[]): Element => {
  const root = document.createElement('div')
  root.className = 'fixed bottom-5 right-5 z-[100]'
  const button = document.createElement('button')
  button.type = 'button'
  button.className = 'zs-gradient-bg rounded-full px-5 py-3 font-bold text-on-primary shadow-zs'
  const lang = document.documentElement.lang
  button.textContent = lang.startsWith('en') ? 'Support' : lang.startsWith('zh-TW') ? '聯絡客服' : '联系客服'
  button.setAttribute('aria-expanded', 'false')
  const panel = document.createElement('div')
  panel.className = 'mb-3 overflow-hidden rounded-zs-lg border border-line bg-surface shadow-zs-lg'
  panel.style.cssText = 'width:min(400px,calc(100vw - 40px));height:min(600px,calc(100vh - 100px));display:none'
  const frame = document.createElement('iframe')
  frame.title = button.textContent
  // A reseller may own a subdomain of the main shop. Never grant this frame
  // the shop origin: its script must not see parent storage, cookies or DOM.
  frame.setAttribute('sandbox', 'allow-scripts allow-popups')
  frame.setAttribute('credentialless', '')
  frame.referrerPolicy = 'no-referrer'
  frame.style.cssText = 'display:block;width:100%;height:100%;border:0'
  const markup = (script: NormalizedScript) => script.code.trimStart().startsWith('<') ? script.code : `<script>${script.code.replaceAll('</script', '<\\/script')}</script>`
  const head = scripts.filter((script) => script.position === 'head').map(markup).join('\n')
  const body = scripts.filter((script) => script.position === 'body_end').map(markup).join('\n')
  frame.srcdoc = `<!doctype html><html><head><meta name="referrer" content="no-referrer"><style>html,body{min-height:100%;margin:0;background:transparent}</style>${head}</head><body>${body}</body></html>`
  panel.appendChild(frame)
  button.addEventListener('click', () => {
    const open = panel.style.display === 'none'
    panel.style.display = open ? 'block' : 'none'
    button.setAttribute('aria-expanded', String(open))
  })
  root.append(panel, button)
  tag(root, 'reseller-support', 'reseller')
  return root
}

export const applyCustomScripts = (raw: unknown, tenantMode: string = 'main') => {
  if (typeof document === 'undefined') return
  const scripts = normalizeCustomScripts(raw).filter((script) => tenantMode === 'reseller' ? script.scope !== 'main' : script.scope !== 'reseller')
  const signature = JSON.stringify([tenantMode, scripts])
  if (signature === appliedSignature) return
  clearCustomScripts()
  const resellerScripts = scripts.filter((script) => script.scope === 'reseller')
  scripts.filter((script) => script.scope !== 'reseller').forEach((item, index) => {
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
  if (tenantMode === 'reseller' && resellerScripts.length) document.body.appendChild(resellerFrame(resellerScripts))
  appliedSignature = signature
}
