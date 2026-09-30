import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";
const md = new MarkdownIt({ html: false, linkify: true, breaks: true });
const fence = md.renderer.rules.fence!;
md.renderer.rules.fence = (tokens, index, options, env, renderer) =>
  `<div class="code-block"><button type="button" class="code-copy" data-code="${escapeHtml(tokens[index].content)}">复制代码</button>${fence(tokens, index, options, env, renderer)}</div>`;
export function escapeHtml(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
}
// Images never cause a network request until the recipient explicitly clicks.
md.renderer.rules.image = (tokens, index) => {
  const token = tokens[index];
  const src = token.attrGet("src") ?? "";
  const alt = token.content || "图片";
  try {
    const url = new URL(src);
    if (url.protocol === "https:" && !url.username && !url.password) {
      return `<button type="button" class="remote-image" data-image-url="${escapeHtml(url.href)}">加载图片 · ${escapeHtml(alt)}</button>`;
    }
  } catch {
    /* Local paths have no source directory in a pasted message. */
  }
  return `<span class="missing-image">${escapeHtml(alt)} · 请作为图片附件发送</span>`;
};
export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(md.render(text), {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["img", "style", "form", "input", "iframe", "svg", "math"],
    FORBID_ATTR: ["style", "srcset"],
    ALLOW_DATA_ATTR: true,
  });
}
export function safeExternalUrl(value: string): string | null {
  try {
    const url = new URL(value);
    return ["https:", "http:", "mailto:"].includes(url.protocol)
      ? url.href
      : null;
  } catch {
    return null;
  }
}
