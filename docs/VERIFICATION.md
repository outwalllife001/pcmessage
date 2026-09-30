# 验证记录

2026-09-30，第一版交付。

- `npm run check`：TypeScript / Vite 构建通过，4 项 Markdown 测试通过。
- `npm run test:ui`：4 项界面操作测试通过。此处使用模拟桌面 API。
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`：3 项真实 HTTPS / 本地存储集成测试通过，包括双向文字与图片、双端配对、重复发送、断线重试和重启恢复。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`：通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`：通过。
- `npm audit`：未报告漏洞。
- Mac ARM64 release DMG：本地构建成功，约 4.21 MiB。仅 ARM64，不包含 Intel 版本。
- Mac 原生客户端：实际启动并读取设置及本机地址；关闭窗口后，证书验证的 HTTPS 探测仍成功，接收服务继续运行。

首次交付时，Windows 11 x64 的安装包尚未生成。GitHub Actions 在执行任何步骤之前被账户账单锁定阻止，见[构建记录](https://github.com/outwalllife001/pcmessage/actions/runs/36669570558)。当时按用户要求先交付 Mac 与源码，不更改账户账单设置。

## Windows 本机构建与安装

2026-09-30，在 Windows 11 Pro x64（10.0.22631）完成：

- 环境：Node.js 24.19.0、Rust 1.98.1 stable MSVC、Visual Studio 2022 C++ Build Tools、已安装的 WebView2 Runtime。
- `npm ci` / 依赖审计：通过，未报告漏洞。
- `npm run check`：TypeScript / Vite 构建及 4 项 Markdown 测试通过。
- `npm run test:ui`：4 项界面操作测试通过，使用模拟桌面 API。
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`：3 项真实 HTTPS / 存储集成测试通过，包括文字与图片、配对、认证拒绝、消息去重、断线重试及重启恢复。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings`、`cargo fmt --manifest-path src-tauri/Cargo.toml --check`：通过。
- `npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis -- --locked`：成功。
- 安装包：`PCMessage_0.1.0_x64-setup.exe`，3,745,454 字节（约 3.57 MiB）。SHA-256：`69d7b5bf6fa4ee9240ba7da15e43c83076c5e689407d6fc71d4b55d98276cf11`。
- NSIS 静默安装退出码为 0，卸载信息正确登记版本 `0.1.0`。本机使用显式目录 `C:\Users\emt\Applications\PCMessage`，避免安装过程继承桌面宿主的 AppData 目录重定向；桌面创建了 PCMessage 快捷方式。
- 安装后的原生客户端实际启动，显示 Windows 本机名称及消息界面；`/v1/info` 在 TCP `47321` 返回 HTTP 200。探测使用本机设备证书作为信任根、核对 `pcmessage.local` 主机名，TLS 校验通过。

修复了 Windows 并行开发时 Vite 监听 Rust 编译目录、碰到锁定 DLL 后报 `EBUSY` 并退出的问题：前端监听排除 `src-tauri` 和本地 `artifacts`。界面测试截图改用 Playwright 的测试输出目录，取消固定的 `/tmp` 路径。本机与 CI 打包均使用锁定的 Rust 依赖。

验证边界：本轮没有第二台实体电脑，尚未进行 Mac 与 Windows 实机互通验证。首次启动的 Windows 防火墙权限提示需要用户手动处理；设置窗口和关闭到托盘的原生交互验证受该提示阻挡。云端构建状态单独记录，不以本机成功推断 GitHub Actions 成功。

## 0.1.1 单向发送问题排查

- 实际 Mac → Windows 11 的证书固定 HTTPS `/v1/info` 返回 200；配对凭证通过校验。
- 原 Mac 应用能收消息，但显示 Windows 离线，发送失败。用户重新关闭、开启 Mac 的“本地网络”权限后，应用显示在线，并成功从 Mac 发消息到 Windows。
- 结论：本次故障与 Mac 本地网络授权状态有关；Windows 接收服务、端口和配对凭证正常。没有修改 Windows 防火墙或重新配对。
- 发送错误保存到 SQLite，并在消息下显示；Mac 的权限类网络错误提示重新检查本地网络开关。网络超时显示目标 IP/端口。
- 已配对设备的在线状态作为提示，允许手动尝试发送。重复重试失败后按钮仍可使用；成功后清除旧错误。
- 新增旧版消息数据库迁移、错误持久化及重复失败重试回归验证。通信协议保持 v1，兼容现有 Windows 客户端。
- 0.1.1 检查结果：前端构建和 4 项 Markdown 测试通过，5 项界面测试通过，4 项 HTTPS / 存储集成测试通过；Clippy、Rust 格式及 `git diff --check` 通过。

## 0.1.2 回车发送

- Enter 发送，Shift + Enter 换行；中文输入法确认候选词的回车不会发送消息，包括 WebKit 的 `keyCode=229` 情况。
- 界面快捷键提示和 README 已同步更新。
- 前端构建、4 项 Markdown 测试及 5 项界面测试通过；发送操作测试覆盖 Enter、Shift + Enter 和输入法确认。

## 0.2.0 任意文件传输

- 新增任意文件选择、拖入、附件卡片、另存；支持文件单独发送、空文件和图片与文件混发。图片仍可预览和粘贴截图。
- 限制为单个文件 100 MiB、每条消息 8 个附件、合计 200 MiB；普通文件不限制扩展名。超出预览限制的图片作为文件传输。文件夹请先压缩。
- 设备能力标记识别旧版客户端；旧版仍可收发文字和图片，文件发送会提示升级到 0.2.0。更新后可以直接重试，无需重新配对。
- 前端构建及 4 项 Markdown 测试、7 项界面测试、6 项真实 HTTPS / SQLite 集成测试通过；Clippy、Rust 格式检查通过。
- 文件集成测试验证 Markdown、PDF、ZIP、EXE、MP4、未知二进制、SVG、空文件的逐字节传输与另存、文件名保留、错误摘要拒绝、断线重试和重启恢复。
- 界面测试验证文件卡片、文件大小、空文件、图片混发、文件名 HTML 转义、保存操作和拖入；准备附件时禁止切换收件电脑和发送。
- 无新增依赖、服务器或消息数据库迁移。沿用 v1 附件存储字段，已有配对与历史保留。
- 本轮文件传输的双节点集成测试在 Mac 本机执行；Windows 源码已同步，任意文件实机互传需要 Windows 同样更新到 0.2.0。
- Mac ARM64 0.2.0 的 `.app` / `.dmg` 本地构建成功；本机应用已更新。原生文件选择器实际选取 Markdown 文件，显示正确文件名和 75 B 大小；验证附件随后从草稿移除，原聊天记录保留。
