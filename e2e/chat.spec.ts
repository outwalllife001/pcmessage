import { test, expect, type Page } from "@playwright/test";
async function desktop(page: Page) {
  await page.addInitScript(() => {
    const w = window as any;
    w.isTauri = true;
    const local = {
      id: "local",
      name: "书房 Mac",
      port: 47321,
      platform: "macos",
      version: 1,
      certificate: "",
    };
    const state = {
      local,
      addresses: ["192.168.1.12"],
      network_error: null,
      pairings: [] as any[],
      peers: [
        {
          ...local,
          id: "windows",
          name: "Windows 11",
          address: "192.168.1.20",
          platform: "windows",
          online: true,
          paired: true,
          unread: 1,
        },
        {
          ...local,
          id: "mini",
          name: "客厅 Mac mini",
          address: "192.168.1.21",
          online: true,
          paired: false,
          unread: 0,
        },
        {
          ...local,
          id: "offline",
          name: "卧室电脑",
          address: "192.168.1.22",
          online: false,
          paired: true,
          unread: 0,
        },
      ],
    };
    const history: any[] = [
      {
        id: "received",
        peer_id: "windows",
        direction: "incoming",
        status: "sent",
        unread: true,
        created_at: 1780000000000,
        text: '# 明天的安排\n\n- **10:00** 开始\n- 带上这段代码\n\n```ts\nconst message = "你好，Mac";\n```',
        images: [],
      },
      {
        id: "outgoing",
        peer_id: "windows",
        direction: "outgoing",
        status: "sent",
        unread: false,
        created_at: 1780000060000,
        text: "收到，我把截图也发过来。",
        images: [],
      },
    ];
    const calls: any[] = [];
    w.__mock = { state, history, calls, copied: "", failSend: false };
    const callbacks = new Map();
    let n = 0;
    const asset = {
      id: "asset1",
      name: "截图.png",
      mime: "image/png",
      size: 68,
      hash: "hash",
    };
    w.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { label: "main" },
      },
      transformCallback: (callback: any) => {
        callbacks.set(++n, callback);
        return n;
      },
      unregisterCallback: () => {},
      invoke: async (cmd: string, args: any = {}) => {
        calls.push({ cmd, args });
        if (cmd === "plugin:event|listen") return ++n;
        if (cmd.startsWith("plugin:notification")) return true;
        if (cmd === "get_state") return structuredClone(state);
        if (cmd === "get_messages")
          return structuredClone(
            history.filter((m) => m.peer_id === args.peerId).slice(-args.limit),
          );
        if (cmd === "mark_read") {
          state.peers.find((p) => p.id === args.peerId)!.unread = 0;
          return;
        }
        if (cmd === "send_message") {
          const m = {
            id: crypto.randomUUID(),
            peer_id: args.peerId,
            text: args.text,
            images: args.imageIds.map(() => asset),
            created_at: Date.now(),
            direction: "outgoing",
            status: w.__mock.failSend ? "failed" : "sent",
            delivery_error: w.__mock.failSend
              ? "无法连接 192.168.1.20:47321：连接超时"
              : null,
            unread: false,
          };
          history.push(m);
          return structuredClone(m);
        }
        if (cmd === "retry_message") {
          const m = history.find((m) => m.id === args.id);
          m.status = w.__mock.failRetry ? "failed" : "sent";
          m.delivery_error = w.__mock.failRetry
            ? "无法连接 192.168.1.20:47321：连接超时"
            : null;
          return structuredClone(m);
        }
        if (cmd === "plugin:clipboard-manager|write_text") {
          w.__mock.copied = args.text;
          return;
        }
        if (cmd === "choose_images") return [asset];
        if (cmd === "stage_image") {
          if (args.bytes.length === 0) throw "图片为空";
          return asset;
        }
        if (cmd === "image_url")
          return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==";
        if (cmd === "clipboard_image") return null;
        if (cmd === "start_pair") {
          state.pairings = [
            {
              id: "pair1",
              peer_id: args.peerId,
              name: "客厅 Mac mini",
              code: "123456",
              incoming: false,
              confirmed: false,
            },
          ];
          return "pair1";
        }
        if (cmd === "confirm_pair") {
          if (args.accept)
            state.peers.find((p) => p.id === "mini")!.paired = true;
          state.pairings = [];
          return;
        }
        if (cmd === "add_peer") return "mini";
        if (cmd === "rename_device") {
          state.local.name = args.name;
          return;
        }
        if (cmd === "save_image") return true;
        return null;
      },
    };
  });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Windows 11" })).toBeVisible();
}
test("Markdown chat, original copy, preview, image attachment and send shortcut", async ({
  page,
}, testInfo) => {
  await desktop(page);
  await expect(page.locator(".markdown h1")).toHaveText("明天的安排");
  await page.getByRole("button", { name: "复制原文" }).first().click();
  expect(await page.evaluate(() => (window as any).__mock.copied)).toContain(
    "**10:00**",
  );
  await page
    .getByRole("textbox", { name: "消息内容" })
    .fill("## 来自 Mac\n\n**你好 Windows**");
  await page.getByRole("button", { name: "预览", exact: true }).click();
  await expect(page.locator("#preview h2")).toHaveText("来自 Mac");
  await page.getByRole("button", { name: "编辑", exact: true }).click();
  await page.getByRole("button", { name: "添加图片", exact: true }).click();
  await expect(page.locator(".draft-image")).toHaveCount(1);
  await page.locator("#text").press("Shift+Enter");
  await expect(page.locator("#text")).toHaveValue(
    "## 来自 Mac\n\n**你好 Windows**\n",
  );
  await page.locator("#text").evaluate((input) => {
    input.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Enter",
        isComposing: true,
        bubbles: true,
      }),
    );
    // WebKit can report the IME confirmation key without isComposing.
    input.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Enter",
        keyCode: 229,
        bubbles: true,
      }),
    );
  });
  expect(
    await page.evaluate(
      () =>
        (window as any).__mock.calls.filter(
          (c: any) => c.cmd === "send_message",
        ).length,
    ),
  ).toBe(0);
  await page.locator("#text").press("Enter");
  await expect(page.locator(".outgoing .markdown h2")).toHaveText("来自 Mac");
  await expect(page.getByRole("textbox", { name: "消息内容" })).toHaveValue("");
  await expect(page.locator(".message-images img")).toHaveCount(1);
  await page.screenshot({ path: testInfo.outputPath("pcmessage-ui.png") });
});
test("per-device drafts, offline state and failed-message retry", async ({
  page,
}) => {
  await desktop(page);
  await page.locator("#text").fill("还没发送的草稿");
  await page.getByRole("button", { name: /卧室电脑/ }).click();
  await expect(page.locator("#conversation-header")).toContainText("离线");
  await page.locator("#text").fill("离线草稿");
  await expect(
    page.getByRole("button", { name: "发送", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: /Windows 11 在线/ }).click();
  await expect(page.locator("#text")).toHaveValue("还没发送的草稿");
  await page.evaluate(() => ((window as any).__mock.failSend = true));
  await page.getByRole("button", { name: "发送", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "发送失败 · 重试" }),
  ).toBeVisible();
  await expect(page.locator(".delivery-error")).toContainText("连接超时");
  await page.getByRole("button", { name: "发送失败 · 重试" }).click();
  await expect(
    page.getByRole("button", { name: "发送失败 · 重试" }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        (window as any).__mock.history.filter(
          (m: any) => m.text === "还没发送的草稿",
        ).length,
    ),
  ).toBe(1);
});
test("new peer requires visible pairing confirmation", async ({ page }) => {
  await desktop(page);
  await page.getByRole("button", { name: /客厅 Mac mini/ }).click();
  await page.getByRole("button", { name: "配对这台电脑" }).click();
  await expect(page.getByText("123 456")).toBeVisible();
  await page.getByRole("button", { name: "号码相同，确认" }).click();
  await expect(page.locator("#pair-dialog")).not.toBeVisible();
  await expect(page.getByRole("textbox", { name: "消息内容" })).toBeVisible();
});
test("remote images remain unloaded until clicked and pasted image bytes are sent", async ({
  page,
}) => {
  await desktop(page);
  await page.locator("#text").fill("![远程](https://example.com/photo.png)");
  await page.getByRole("button", { name: "发送", exact: true }).click();
  await expect(page.locator(".remote-image")).toBeVisible();
  await expect(
    page.locator('img[src="https://example.com/photo.png"]'),
  ).toHaveCount(0);
  await page.locator("#text").evaluate((element) => {
    const data = new DataTransfer();
    data.items.add(
      new File([new Uint8Array([137, 80, 78, 71])], "paste.png", {
        type: "image/png",
      }),
    );
    element.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true }),
    );
  });
  await expect(page.locator(".draft-image")).toHaveCount(1);
  expect(
    await page.evaluate(
      () =>
        (window as any).__mock.calls.find((c: any) => c.cmd === "stage_image")
          .args.bytes,
    ),
  ).toEqual([137, 80, 78, 71]);
});

test("a repeated failed retry remains available and keeps its error", async ({
  page,
}) => {
  await desktop(page);
  await page.evaluate(() => {
    (window as any).__mock.failSend = true;
    (window as any).__mock.failRetry = true;
  });
  await page.locator("#text").fill("重试失败也可以再次重试");
  await page.getByRole("button", { name: "发送", exact: true }).click();
  const retry = page.getByRole("button", { name: "发送失败 · 重试" });
  await retry.click();
  await expect(retry).toBeEnabled();
  await expect(page.locator(".delivery-error")).toContainText("连接超时");
  await page.evaluate(() => ((window as any).__mock.failRetry = false));
  await retry.click();
  await expect(retry).toHaveCount(0);
});
