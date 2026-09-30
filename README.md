# PCMessage

家里的 Windows 和 Mac 之间发文字、图片和文件。输入或粘贴文字即可发送，支持 Markdown，不需要把文字保存成文件。

## 支持的平台

- **Mac：Apple Silicon ARM64，macOS 15+，面向 M4 Pro。** 不构建 Intel 版。
- **Windows 11 x64。** 安装包会在缺少 WebView2 时引导安装。
- 两台电脑在同一家庭局域网，并且都运行 PCMessage。

## 使用

1. Mac 打开 `.dmg`，把 PCMessage 拖到“应用程序”；Windows 运行 `-setup.exe`。
2. 两边打开应用，在左侧选择另一台电脑。没有自动发现时，点“添加电脑”，填写对方“设置”中的 `IP:端口`。
3. 在一台电脑上发起配对。两边核对同一个六位号码，并各自确认。
4. 输入文字，按 Enter 或点击“发送”。Shift + Enter 换行。

点击“添加文件”或拖入文件即可发送，支持任意文件类型；收到后点击文件卡片另存。图片仍可粘贴截图、放大及另存。文字消息可以复制 Markdown 原文，代码块可以单独复制。关闭窗口会留在菜单栏/托盘继续接收；要完全退出，使用托盘菜单的“退出”。

对方离线时保留草稿；发送过程中断时保存失败消息，点击“重试”即可，不会重复生成消息。历史记录、配对和草稿都保存在本机。

文件传输需要两边都更新到 **0.2.0 或更高版本**。已有配对、聊天记录和草稿会保留；与旧版电脑仍可收发文字和图片。

## 获取安装包

推送代码后，[Desktop builds](https://github.com/outwalllife001/pcmessage/actions/workflows/build.yml) 自动构建两个平台。进入一次成功的运行，在 Artifacts 中下载：

- `PCMessage-aarch64-apple-darwin`：Mac `.dmg`
- `PCMessage-x86_64-pc-windows-msvc`：Windows `-setup.exe`

下载的 ZIP 先解压。测试安装包未购买代码签名证书；如果系统拦截，请从系统设置允许打开你自己构建/本仓库下载的应用。

Windows 安装包也可以在本机生成，构建及实机安装结果见[验证记录](docs/VERIFICATION.md)。GitHub Actions 曾被账户账单锁定阻止；若云端运行失败，可按下面的命令在 Windows 本机构建。

## 开发

需要 Node.js 22+、Rust stable；Mac 需要 Xcode Command Line Tools，Windows 需要 Visual Studio C++ Build Tools。

```sh
npm ci
npm run tauri -- dev
```

检查与测试：

```sh
npm run check
npx playwright install chromium
npm run test:ui
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Mac 构建：

```sh
npm run tauri -- build --target aarch64-apple-darwin --bundles dmg
```

Windows 构建（在 Windows 上执行）：

```powershell
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis -- --locked
```

安装包输出到 `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/PCMessage_0.2.0_x64-setup.exe`。默认仅为当前用户安装，无需管理员权限。安装后可从开始菜单打开 PCMessage。

## 功能范围

- 一对一文字、图片和任意类型文件，支持标题、列表、引用、表格、代码块。
- 每条文字最多 256 KiB；最多 8 个附件；单个文件最多 100 MiB；文字与附件合计最多 200 MiB。文件夹请先压缩。
- PNG、JPEG、GIF 和 WebP 在不超过 15 MiB / 2500 万像素时显示图片预览，其余图片作为文件发送。Markdown 中的 HTTPS 图片默认不加载，点击后才加载；粘贴文字里的本地图片路径需要改为发送图片附件。
- IPv4 家庭局域网直连；没有账号、云服务、群聊和跨公网连接。
- HTTPS 传输，首次核对配对码，之后固定信任设备证书。配对码比较时，两台电脑都应由你操作。消息数据库没有做额外的静态加密。

## 网络与数据

默认 TCP `47321` 接收消息，UDP `47322` 发现设备，组播地址 `239.255.47.32`。若 TCP 端口被占用，自动选择空闲端口，实际地址可在“设置”中查看。

Windows 将家庭网络设为“专用”，允许 PCMessage 通过防火墙；Mac 允许本地网络访问。路由器的访客网络/客户端隔离会阻止电脑互通。

应用数据：

- Mac：`~/Library/Application Support/com.pcmessage.desktop/`
- Windows：`%APPDATA%\com.pcmessage.desktop\`

这里包含 SQLite 记录、图片、设备证书和配对凭证。更换电脑/迁移数据时关闭应用后整体备份；不要只删除证书。取消配对保留原有消息。

实现使用 Tauri、Rust、TypeScript、markdown-it、DOMPurify 和 SQLite。见[实现说明](docs/PLAN.md)与[项目调研](docs/RESEARCH.md)。

### Mac 能接收，但发不出去

在 Mac 的 **系统设置 → 隐私与安全 → 本地网络** 中，检查 PCMessage。
如果开关已开启但仍无法发送，关闭后重新开启，再完全退出并重启 PCMessage。
本次实际排查中，Windows 的端口和配对凭证正常，重新开关 Mac 的本地网络权限后恢复双向通信。
0.1.1 会保存并显示发送失败的具体原因；已配对的电脑显示离线时，也可以尝试发送。
