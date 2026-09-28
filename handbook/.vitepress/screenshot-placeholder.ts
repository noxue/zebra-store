import type MarkdownIt from 'markdown-it'

/** Prefix of the screenshots a later Playwright pass captures (see handbook/SCREENSHOTS.md). */
const SHOT_PREFIX = '/screenshots/'

function escapeAttr(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

/**
 * Renders `![说明](/screenshots/…png)` as the <ZsShot> component: the image when it exists,
 * a labelled placeholder while it has not been captured yet. Using a component (not a
 * plain <img>) also keeps Vite from treating a missing file as a build error.
 */
export function screenshotPlaceholder(md: MarkdownIt): void {
  const fallback = md.renderer.rules.image
  md.renderer.rules.image = (tokens, idx, options, env, self) => {
    const token = tokens[idx]
    const src = token.attrGet('src') ?? ''
    if (!src.startsWith(SHOT_PREFIX)) {
      return fallback ? fallback(tokens, idx, options, env, self) : self.renderToken(tokens, idx, options)
    }
    const alt = token.children ? md.renderer.renderInlineAsText(token.children, options, env) : token.content
    return `<ZsShot src="${escapeAttr(src)}" alt="${escapeAttr(alt)}" />`
  }
}
