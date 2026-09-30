import "./style.css";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";
import { renderMarkdown, escapeHtml as e, safeExternalUrl } from "./markdown";
import type { Attachment, Draft, Message, Peer, Snapshot } from "./types";

const icons = {
  chat: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M5 5h14v11H10l-5 4V5Z"/><path d="M8 9h8M8 12h5"/></svg>',
  plus: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M12 5v14M5 12h14"/></svg>',
  settings:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><circle cx="12" cy="12" r="3"/><path d="m10 3-1 3-3 1-3-1-1 4 2 2-1 3 2 3 3-1 2 3h4l1-3 3-1 3 1 1-4-2-2 1-3-2-3-3 1-2-3h-4Z"/></svg>',
  image:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><rect x="4" y="4" width="16" height="16" rx="3"/><circle cx="9" cy="9" r="1.5"/><path d="m4 17 5-5 4 4 3-3 4 4"/></svg>',
  send: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="m5 12 14-7-5 14-3-6-6-1Z"/><path d="m11 13 8-8"/></svg>',
  computer:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></svg>',
};
const app = document.querySelector<HTMLDivElement>("#app")!;
app.innerHTML = `
<aside class="sidebar">
  <div class="brand"><span class="brand-mark">${icons.chat}</span><div>PCMessage<small>家里的电脑，直接聊。</small></div></div>
  <div class="section-label"><span>电脑</span><button class="icon-button" id="add" aria-label="添加电脑" title="添加电脑">${icons.plus}</button></div>
  <nav id="peers" aria-label="电脑列表"></nav>
  <div class="sidebar-bottom"><div id="local" class="local-device"></div><button class="icon-button" id="settings" aria-label="设置" title="设置">${icons.settings}</button></div>
</aside>
<main>
  <header id="conversation-header"><div><h1>消息</h1><span class="subtle">在你的电脑之间</span></div></header>
  <div id="network-error" class="network-error" hidden></div>
  <section id="messages" aria-label="消息记录"><div class="empty"><span class="empty-icon">${icons.chat}</span><h2>选一台电脑，发条消息。</h2><p>文字、代码，或一张截图。</p><button class="secondary" id="empty-add">添加电脑</button></div></section>
  <section id="composer" hidden aria-label="发送消息">
    <div id="draft-images" class="draft-images"></div>
    <div id="preview" class="markdown preview" hidden></div>
    <textarea id="text" placeholder="写点什么…" aria-label="消息内容" spellcheck="false"></textarea>
    <div class="composer-toolbar"><div class="composer-actions"><button class="icon-button" id="attach" title="添加图片" aria-label="添加图片">${icons.image}</button><button class="text-button" id="toggle-preview" aria-pressed="false">预览</button><span class="format-hint">Markdown</span></div><div class="send-actions"><span id="shortcut"></span><button class="primary" id="send">发送 ${icons.send}</button></div></div>
  </section>
  <div id="pair-bar" hidden></div>
</main>
<div id="toast" role="status" hidden></div>
<dialog id="add-dialog"><form id="add-form"><div class="dialog-heading"><h2>添加电脑</h2><button type="button" class="icon-button close-dialog" aria-label="关闭">×</button></div><label for="address">对方电脑的 IP</label><input id="address" placeholder="192.168.1.20" autocomplete="off" required /><p class="subtle">可在对方 PCMessage 的设置中查看。</p><button class="primary" id="connect">连接</button></form></dialog>
<dialog id="settings-dialog"><form id="settings-form"><div class="dialog-heading"><h2>设置</h2><button type="button" class="icon-button close-dialog" aria-label="关闭">×</button></div><label for="device-name">电脑名</label><input id="device-name" maxlength="40" required /><div class="address-box"><span class="subtle">本机地址</span><div id="addresses"></div></div><p class="subtle">关闭窗口后继续接收。完全退出请使用托盘菜单。</p><button class="primary">保存</button></form></dialog>
<dialog id="pair-dialog"><div class="dialog-heading"><h2>确认配对</h2></div><div id="pair-content"></div></dialog>
<dialog id="image-dialog"><div class="lightbox-toolbar"><button class="secondary" id="save-lightbox">另存图片</button><button class="icon-button close-dialog" aria-label="关闭">×</button></div><img id="lightbox-image" alt="图片预览" /></dialog>`;

function $<T extends HTMLElement = HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}
const input = $<HTMLTextAreaElement>("text");
const messageList = $("messages");
let state: Snapshot | null = null;
let selected: string | null = null;
let messages: Message[] = [];
let draft: Draft = { text: "", images: [] };
let preview = false;
let sending = false;
let limit = 100;
let renderKey = "";
let stateKey = "";
let toastTimer: ReturnType<typeof setTimeout>;
let refreshing = false;
let refreshAgain = false;
let lightboxId = "";
const imageCache = new Map<string, Promise<string>>();
const drafts = new Map<string, Draft>();
$("shortcut").textContent = "Enter 发送 · Shift + Enter 换行";
function toast(message: unknown) {
  $("toast").textContent = String(message);
  $("toast").hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => ($("toast").hidden = true), 4500);
}
async function action<T>(fn: () => Promise<T>): Promise<T | undefined> {
  try {
    return await fn();
  } catch (error) {
    toast(error);
    return undefined;
  }
}
function peer(): Peer | undefined {
  return state?.peers.find((p) => p.id === selected);
}
function saveDraft() {
  if (!selected) return;
  draft.text = input.value;
  const copy = { text: draft.text, images: [...draft.images] };
  drafts.set(selected, copy);
  try {
    localStorage.setItem(`draft:${selected}`, JSON.stringify(copy));
  } catch {
    /* Draft remains in memory when browser storage is full. */
  }
}
function loadDraft(id: string): Draft {
  if (drafts.has(id)) return drafts.get(id)!;
  try {
    const saved = JSON.parse(localStorage.getItem(`draft:${id}`) ?? "null");
    if (saved && typeof saved.text === "string" && Array.isArray(saved.images))
      return saved;
  } catch {
    /* Ignore a damaged draft; message history is stored separately. */
  }
  return { text: "", images: [] };
}
function imageUrl(id: string): Promise<string> {
  if (imageCache.size >= 8 && !imageCache.has(id))
    imageCache.delete(imageCache.keys().next().value!);
  if (!imageCache.has(id))
    imageCache.set(
      id,
      invoke<string>("image_url", { id }).catch((error) => {
        imageCache.delete(id);
        throw error;
      }),
    );
  return imageCache.get(id)!;
}
async function hydrateImages(root: HTMLElement) {
  for (const img of root.querySelectorAll<HTMLImageElement>(
    "img[data-asset]",
  )) {
    const id = img.dataset.asset!;
    try {
      const url = await imageUrl(id);
      if (img.isConnected) img.src = url;
    } catch {
      if (img.isConnected) img.alt = "图片无法读取";
    }
  }
}
function renderPeers() {
  if (!state) return;
  const local = state.local;
  $("local").innerHTML =
    `<span class="presence"></span><div><strong>${e(local.name)}</strong><small>本机 · ${local.platform === "macos" ? "Mac" : "Windows"}</small></div>`;
  $("peers").innerHTML = state.peers.length
    ? state.peers
        .map(
          (p) =>
            `<button class="peer ${p.id === selected ? "selected" : ""}" data-peer="${e(p.id)}"><span class="device-avatar ${p.online ? "" : "offline"}">${icons.computer}</span><span class="peer-info"><strong>${e(p.name)}</strong><small><i class="presence ${p.online ? "" : "offline"}"></i>${p.online ? (p.paired ? "在线" : "可配对") : "离线"}</small></span>${p.unread ? `<span class="unread">${p.unread > 99 ? "99+" : p.unread}</span>` : ""}</button>`,
        )
        .join("")
    : '<p class="no-peers">还没有发现其他电脑</p>';
  $("network-error").hidden = !state.network_error;
  $("network-error").textContent = state.network_error ?? "";
}
function renderHeader() {
  const p = peer();
  if (!p) return;
  $("conversation-header").innerHTML =
    `<div class="header-peer"><span class="header-avatar">${icons.computer}</span><div><h1>${e(p.name)}</h1><span class="subtle">${p.online ? "在线" : "离线"} · ${e(p.address)}</span></div></div>${p.paired ? '<button class="text-button" id="forget">取消配对</button>' : ""}`;
  $("composer").hidden = !p.paired;
  $("pair-bar").hidden = p.paired;
  if (!p.paired)
    $("pair-bar").innerHTML =
      '<p>首次连接，确认一次配对码。</p><button class="primary" id="begin-pair">配对这台电脑</button>';
  updateSend();
}
function updateSend() {
  const p = peer();
  $<HTMLButtonElement>("send").disabled =
    sending || !p?.paired || (!input.value.trim() && !draft.images.length);
  $("send").innerHTML = `${sending ? "发送中" : "发送"} ${icons.send}`;
  $<HTMLButtonElement>("attach").disabled = sending;
  input.readOnly = sending;
}
function renderDraft() {
  $("draft-images").innerHTML = draft.images
    .map(
      (a) =>
        `<div class="draft-image"><img data-asset="${e(a.id)}" alt="${e(a.name)}"/><button type="button" data-remove="${e(a.id)}" aria-label="移除 ${e(a.name)}">×</button></div>`,
    )
    .join("");
  void hydrateImages($("draft-images"));
  $("preview").hidden = !preview;
  input.hidden = preview;
  $("preview").innerHTML =
    renderMarkdown(input.value) || '<span class="subtle">还没有文字</span>';
  $("toggle-preview").textContent = preview ? "编辑" : "预览";
  $("toggle-preview").setAttribute("aria-pressed", String(preview));
  updateSend();
}
function renderMessages() {
  const p = peer();
  if (!p) return;
  const key = `${selected}:${limit}:${JSON.stringify(messages)}`;
  if (key === renderKey) return;
  const oldHeight = messageList.scrollHeight;
  const nearBottom =
    oldHeight - messageList.scrollTop - messageList.clientHeight < 110;
  const previousPeer = renderKey.split(":")[0];
  renderKey = key;
  if (!messages.length) {
    messageList.innerHTML =
      '<div class="empty conversation-empty"><span class="empty-icon">' +
      icons.chat +
      "</span><h2>从第一条消息开始。</h2><p>把文字或截图发到这台电脑。</p></div>";
    return;
  }
  const date = (timestamp: number) =>
    new Date(timestamp).toLocaleTimeString("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
    });
  messageList.innerHTML = `${messages.length >= limit ? '<button class="text-button load-more" id="older">更早的消息</button>' : ""}${messages.map((m) => `<article class="message ${m.direction}" data-message="${e(m.id)}"><div class="message-meta"><strong>${m.direction === "outgoing" ? "我" : e(p.name)}</strong><time datetime="${new Date(m.created_at).toISOString()}">${date(m.created_at)}</time></div><div class="bubble"><div class="markdown">${renderMarkdown(m.text)}</div>${m.images.length ? `<div class="message-images">${m.images.map((a) => `<button class="image-button" data-open-image="${e(a.id)}" aria-label="查看 ${e(a.name)}"><img data-asset="${e(a.id)}" alt="${e(a.name)}" /></button>`).join("")}</div>` : ""}</div><div class="message-tools">${m.text ? `<button class="text-button" data-copy="${e(m.id)}" data-direction="${m.direction}">复制原文</button>` : ""}${m.direction === "outgoing" ? (m.status === "failed" ? `<button class="retry text-button" data-retry="${e(m.id)}">发送失败 · 重试</button>${m.delivery_error ? `<span class="delivery-error">${e(m.delivery_error)}</span>` : ""}` : `<span>${m.status === "sending" ? "发送中…" : "已发送"}</span>`) : ""}</div></article>`).join("")}`;
  void hydrateImages(messageList).then(() => {
    if (nearBottom || previousPeer !== selected)
      messageList.scrollTop = messageList.scrollHeight;
  });
  if (nearBottom || previousPeer !== selected)
    messageList.scrollTop = messageList.scrollHeight;
  else messageList.scrollTop += messageList.scrollHeight - oldHeight;
}
function renderPairings() {
  const dialog = $<HTMLDialogElement>("pair-dialog");
  const pairing = state?.pairings[0];
  if (!pairing) {
    if (dialog.open) dialog.close();
    return;
  }
  $("pair-content").innerHTML =
    `<p>与 <strong>${e(pairing.name)}</strong> 配对</p><div class="pair-code">${pairing.code.slice(0, 3)} ${pairing.code.slice(3)}</div><p class="subtle">两台电脑显示的号码相同，再确认。</p><div class="dialog-actions"><button class="secondary" data-decline="${e(pairing.id)}">取消</button><button class="primary" data-confirm="${e(pairing.id)}" ${pairing.confirmed ? "disabled" : ""}>${pairing.confirmed ? "等待对方确认…" : "号码相同，确认"}</button></div>`;
  if (!dialog.open) dialog.showModal();
}
async function refresh() {
  if (!isTauri()) return;
  if (refreshing) {
    refreshAgain = true;
    return;
  }
  refreshing = true;
  try {
    do {
      refreshAgain = false;
      state = await invoke<Snapshot>("get_state");
      if (selected && !state.peers.some((p) => p.id === selected)) {
        saveDraft();
        selected = null;
        messages = [];
        renderKey = "";
        $("composer").hidden = true;
        $("pair-bar").hidden = true;
        $("conversation-header").innerHTML =
          '<div><h1>消息</h1><span class="subtle">在你的电脑之间</span></div>';
        messageList.innerHTML =
          '<div class="empty"><span class="empty-icon">' +
          icons.chat +
          "</span><h2>选一台电脑，发条消息。</h2></div>";
      }
      if (!selected) {
        const first = state.peers.find((p) => p.paired);
        if (first) await selectPeer(first.id, false);
      }
      const key = JSON.stringify(state);
      if (key !== stateKey) {
        stateKey = key;
        renderPeers();
        renderHeader();
        renderPairings();
      }
      if (selected) {
        const id = selected;
        const history = await invoke<Message[]>("get_messages", {
          peerId: id,
          limit,
        });
        if (selected === id) {
          messages = history;
          renderMessages();
          if (document.hasFocus()) await invoke("mark_read", { peerId: id });
        }
      }
    } while (refreshAgain);
  } catch (error) {
    toast(error);
  } finally {
    refreshing = false;
  }
}
async function selectPeer(id: string, reload = true) {
  if (sending) return;
  saveDraft();
  selected = id;
  limit = 100;
  messages = [];
  renderKey = "";
  stateKey = "";
  draft = loadDraft(id);
  input.value = draft.text;
  preview = false;
  renderPeers();
  renderHeader();
  renderDraft();
  if (reload) await refresh();
  input.focus();
}
async function addImages(images: Attachment[]) {
  if (draft.images.length + images.length > 8) {
    toast("每条消息最多 8 张图片");
    return;
  }
  if (
    [...draft.images, ...images].reduce((sum, a) => sum + a.size, 0) >
    32 * 1024 * 1024
  ) {
    toast("一条消息最多 32 MB");
    return;
  }
  draft.images.push(...images);
  saveDraft();
  renderDraft();
}
async function pasteFiles(files: File[]) {
  if (!selected || sending) return;
  for (const file of files) {
    if (file.size > 15 * 1024 * 1024) {
      toast("单张图片最多 15 MB");
      continue;
    }
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    const asset = await action(() =>
      invoke<Attachment>("stage_image", { bytes, name: file.name }),
    );
    if (asset) await addImages([asset]);
  }
}
async function send() {
  if ($<HTMLButtonElement>("send").disabled || !selected) return;
  const id = selected;
  const text = input.value;
  const imageIds = draft.images.map((a) => a.id);
  sending = true;
  updateSend();
  try {
    const message = await invoke<Message>("send_message", {
      peerId: id,
      text,
      imageIds,
    });
    input.value = "";
    draft = { text: "", images: [] };
    saveDraft();
    preview = false;
    renderDraft();
    if (message.status === "failed")
      toast(message.delivery_error || "发送失败，消息已保留，可点击重试");
  } catch (error) {
    toast(error);
  } finally {
    sending = false;
    updateSend();
    await refresh();
    input.focus();
  }
}
$("add").onclick = () => $<HTMLDialogElement>("add-dialog").showModal();
$("empty-add").onclick = $("add").onclick;
$("settings").onclick = () => {
  if (!state) return;
  $<HTMLInputElement>("device-name").value = state.local.name;
  $("addresses").innerHTML = (
    state.addresses.length ? state.addresses : ["127.0.0.1"]
  )
    .map((address) => `<code>${e(address)}:${state!.local.port}</code>`)
    .join("");
  $<HTMLDialogElement>("settings-dialog").showModal();
};
for (const button of document.querySelectorAll<HTMLButtonElement>(
  ".close-dialog",
))
  button.onclick = () => button.closest("dialog")!.close();
$("add-form").onsubmit = (event) => {
  event.preventDefault();
  void action(async () => {
    const button = $<HTMLButtonElement>("connect");
    button.disabled = true;
    try {
      const id = await invoke<string>("add_peer", {
        address: $<HTMLInputElement>("address").value,
      });
      $<HTMLDialogElement>("add-dialog").close();
      await refresh();
      await selectPeer(id);
    } finally {
      button.disabled = false;
    }
  });
};
$("settings-form").onsubmit = (event) => {
  event.preventDefault();
  void action(async () => {
    await invoke("rename_device", {
      name: $<HTMLInputElement>("device-name").value,
    });
    $<HTMLDialogElement>("settings-dialog").close();
    await refresh();
  });
};
$("peers").onclick = (event) => {
  const button = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-peer]",
  );
  if (button) void selectPeer(button.dataset.peer!);
};
$("conversation-header").onclick = (event) => {
  if (
    (event.target as HTMLElement).closest("#forget") &&
    selected &&
    confirm("取消与这台电脑的配对？消息记录会保留。")
  )
    void action(async () => {
      await invoke("forget_peer", { peerId: selected });
      await refresh();
    });
};
$("pair-bar").onclick = (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>(
    "#begin-pair",
  );
  if (button && selected)
    void action(async () => {
      button.disabled = true;
      try {
        await invoke("start_pair", { peerId: selected });
        await refresh();
      } finally {
        button.disabled = false;
      }
    });
};
$("pair-content").onclick = (event) => {
  const button = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-confirm],[data-decline]",
  );
  if (button)
    void action(async () => {
      await invoke("confirm_pair", {
        id: button.dataset.confirm ?? button.dataset.decline,
        accept: !!button.dataset.confirm,
      });
      await refresh();
    });
};
$<HTMLDialogElement>("pair-dialog").addEventListener("cancel", (event) => {
  event.preventDefault();
  const p = state?.pairings[0];
  if (p)
    void action(async () => {
      await invoke("confirm_pair", { id: p.id, accept: false });
      await refresh();
    });
});
input.oninput = () => {
  saveDraft();
  updateSend();
};
input.onkeydown = (event) => {
  if (
    event.key === "Enter" &&
    !event.shiftKey &&
    !event.isComposing &&
    event.keyCode !== 229
  ) {
    event.preventDefault();
    void send();
  }
};
input.onpaste = (event) => {
  const files = Array.from(event.clipboardData?.files ?? []).filter((f) =>
    f.type.startsWith("image/"),
  );
  if (files.length) {
    event.preventDefault();
    void pasteFiles(files);
  } else if (!event.clipboardData?.getData("text/plain"))
    void action(async () => {
      const image = await invoke<Attachment | null>("clipboard_image");
      if (image) await addImages([image]);
    });
};
$("toggle-preview").onclick = () => {
  preview = !preview;
  renderDraft();
  if (!preview) input.focus();
};
$("attach").onclick = () =>
  void action(async () => {
    const images = await invoke<Attachment[]>("choose_images");
    await addImages(images);
  });
$("draft-images").onclick = (event) => {
  const button = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-remove]",
  );
  if (button && !sending) {
    draft.images = draft.images.filter((a) => a.id !== button.dataset.remove);
    saveDraft();
    renderDraft();
  }
};
$("send").onclick = () => void send();
messageList.onclick = (event) => {
  void action(async () => {
    const element = (event.target as HTMLElement).closest<HTMLElement>(
      "button,a",
    );
    if (!element) return;
    if (element.dataset.copy) {
      const message = messages.find(
        (m) =>
          m.id === element.dataset.copy &&
          m.direction === element.dataset.direction,
      );
      if (message) {
        await writeText(message.text);
        toast("已复制原文");
      }
    } else if (element.dataset.code !== undefined) {
      await writeText(element.dataset.code);
      toast("代码已复制");
    } else if (element.dataset.retry && selected) {
      (element as HTMLButtonElement).disabled = true;
      try {
        const retried = await invoke<Message>("retry_message", {
          peerId: selected,
          id: element.dataset.retry,
        });
        if (retried.status === "failed")
          toast(retried.delivery_error || "发送失败");
        await refresh();
      } finally {
        (element as HTMLButtonElement).disabled = false;
      }
    } else if (element.dataset.openImage) {
      lightboxId = element.dataset.openImage;
      $<HTMLImageElement>("lightbox-image").src = await imageUrl(lightboxId);
      $<HTMLDialogElement>("image-dialog").showModal();
    } else if (element.dataset.imageUrl) {
      const url = element.dataset.imageUrl;
      const parsed = new URL(url);
      if (parsed.protocol === "https:") {
        const img = document.createElement("img");
        img.referrerPolicy = "no-referrer";
        img.className = "loaded-remote-image";
        img.alt = "远程图片";
        img.src = url;
        element.replaceWith(img);
      }
    } else if (element.id === "older") {
      limit += 100;
      await refresh();
    } else if (element.tagName === "A") {
      event.preventDefault();
      const url = safeExternalUrl(element.getAttribute("href") ?? "");
      if (url) await openUrl(url);
    }
  });
};
// Prevent navigation from links in both messages and preview.
app.addEventListener("click", (event) => {
  const link = (event.target as HTMLElement).closest<HTMLAnchorElement>(
    ".markdown a",
  );
  if (link) {
    event.preventDefault();
    if (!messageList.contains(link)) {
      const url = safeExternalUrl(link.href);
      if (url) void action(() => openUrl(url));
    }
  }
});
$("save-lightbox").onclick = () =>
  void action(async () => {
    if (await invoke<boolean>("save_image", { id: lightboxId }))
      toast("图片已保存");
  });
window.addEventListener("focus", () => void refresh());
window.addEventListener("beforeunload", saveDraft);
app.addEventListener("dragover", (event) => {
  event.preventDefault();
  if (peer()?.paired) $("composer").classList.add("dragging");
});
app.addEventListener("dragleave", () =>
  $("composer").classList.remove("dragging"),
);
app.addEventListener("drop", (event) => {
  event.preventDefault();
  $("composer").classList.remove("dragging");
  if (peer()?.paired)
    void pasteFiles(
      Array.from(event.dataTransfer?.files ?? []).filter((f) =>
        f.type.startsWith("image/"),
      ),
    );
});
async function start() {
  if (!isTauri()) {
    messageList.innerHTML =
      '<div class="empty"><span class="empty-icon">' +
      icons.chat +
      "</span><h2>请打开 PCMessage 桌面客户端。</h2><p>浏览器页面仅用于开发界面。</p></div>";
    $<HTMLButtonElement>("add").disabled = true;
    return;
  }
  await listen("changed", () => void refresh());
  await listen<string>("notice", (event) => toast(event.payload));
  await getCurrentWindow().onFocusChanged(({ payload }) => {
    if (payload) void refresh();
  });
  await getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "drop" && peer()?.paired && !sending)
      void action(async () => {
        const images = await invoke<Attachment[]>("stage_paths", {
          paths: event.payload.type === "drop" ? event.payload.paths : [],
        });
        await addImages(images);
      });
    $("composer").classList.toggle("dragging", event.payload.type === "over");
  });
  await refresh();
  if (!(await isPermissionGranted())) await requestPermission();
}
void start().catch(toast);
