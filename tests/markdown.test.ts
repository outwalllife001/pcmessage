// @vitest-environment jsdom
import { describe, it, expect } from "vitest";
import { renderMarkdown, safeExternalUrl } from "../src/markdown";
describe("received Markdown", () => {
  it("renders Chinese, code blocks and tables while preserving code literally", () => {
    const html = renderMarkdown(
      "# 中文\n\n**你好**\n\n```html\n<script>alert(1)</script>\n```\n\n| 列 | 值 |\n| --- | --- |\n| A | B |",
    );
    expect(html).toContain("<h1>中文</h1>");
    expect(html).toContain("<strong>你好</strong>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).toContain("<table>");
    expect(html).not.toContain("<script>");
  });
  it("does not run embedded HTML, script links or inline event handlers", () => {
    const html = renderMarkdown(
      '<img src=x onerror=alert(1)>\n\n[bad](javascript:alert(1))\n\n<iframe src="https://example.com"></iframe>',
    );
    const root = document.createElement("div");
    root.innerHTML = html;
    expect(root.querySelector("img,iframe,script,[onerror]")).toBeNull();
    expect(root.querySelector('a[href^="javascript:"]')).toBeNull();
  });
  it("does not request remote or local images automatically", () => {
    const html = renderMarkdown(
      "![远程](https://example.com/a.png)\n\n![本地](./a.png)\n\n![secret](file:///etc/passwd)",
    );
    const root = document.createElement("div");
    root.innerHTML = html;
    expect(root.querySelector("img")).toBeNull();
    expect(root.querySelector("button")?.dataset.imageUrl).toBe(
      "https://example.com/a.png",
    );
    expect(root.querySelectorAll(".missing-image")).toHaveLength(1);
    expect(root.textContent).toContain("file:///etc/passwd");
  });
  it("allows web links but rejects desktop and executable protocols", () => {
    expect(safeExternalUrl("https://example.com")).toBe("https://example.com/");
    expect(safeExternalUrl("mailto:a@example.com")).toBe(
      "mailto:a@example.com",
    );
    for (const url of [
      "javascript:alert(1)",
      "file:///etc/passwd",
      "data:text/html,x",
      "tauri://localhost",
    ])
      expect(safeExternalUrl(url)).toBeNull();
  });
});
