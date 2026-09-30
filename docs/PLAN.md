# PCMessage 实现说明

目标已收窄为 Apple Silicon Mac（M4 Pro，macOS 15+）和 Windows 11 x64。主用途是文字消息，Markdown 只是排版方式。第一版已实现桌面客户端，Mac 安装包本地生成，Windows 已配置 GitHub Actions 构建；本次按用户要求先交付 Mac 与源码。

## 已实现

- 局域网自动发现，手动 IP:端口连接。
- 六位码双端确认配对，后续固定证书和设备凭证。
- 一对一文字、Markdown 预览、复制原文、复制代码。
- 截图粘贴、图片放大及另存；任意文件选择、拖入和另存。
- SQLite 本地历史，按电脑保存草稿，未读数量。
- 发送状态、失败重试、消息去重。
- 系统通知、托盘常驻、单实例运行。

不包含账号、群聊、公网连接、云同步、远程桌面或离线中继。

## 结构

```text
src/                       TypeScript 聊天界面与 Markdown 渲染
src-tauri/src/model.rs       设备、消息、附件与大小限制
src-tauri/src/storage.rs     SQLite 历史与配对
src-tauri/src/core.rs        设备身份、附件落盘和接收事务
src-tauri/src/network.rs     UDP 发现、HTTPS、配对和传输
src-tauri/src/lib.rs         桌面命令、剪贴板、通知与托盘
src-tauri/tests/transport.rs  两个隔离节点的真实 HTTPS 集成测试
tests/markdown.test.ts       Markdown 与危险内容处理测试
e2e/chat.spec.ts             浏览器界面操作测试（桌面 API 被模拟）
.github/workflows/build.yml  Mac ARM64 / Windows x64 构建
```

界面使用普通 TypeScript + Vite，Tauri 使用系统 WebView。应用各自启动内置接收端，没有额外服务器。

## 收发

设备通过 UDP 组播宣布名称、端口和公开证书。对已配对的电脑同时做证书验证的探测，作为自动发现的备用；手动 IP 添加仅接受私有/本地 IPv4 地址。

消息传输走 HTTPS。设备身份为证书 SHA-256 指纹。首次发现的信息尚未信任，用户在两台电脑核对绑定双方证书和本次请求的六位号码。只有确认后才保存配对凭证，后续固定信任对端证书；发送端也必须提供该配对凭证。

文字传 UTF-8 原文；图片与文件传 multipart 二进制。接收端检查大小、元数据和 SHA-256，先完整写入附件，再保存消息，最后回执。失败重试使用同一个消息 ID；重复内容不再插入，冲突内容拒绝。原始 HTML 不执行，渲染结果经过 DOMPurify，远程图片需要点击才加载。

附件存放在应用数据目录，命名使用规范 UUID，不使用收到的文件路径。私钥和 SQLite 数据在本机保存；传输加密不代表本地数据库加密。

## 验证范围

本地自动化覆盖：中文 Markdown、代码、表格、图片附件、HTTPS 配对与固定证书、未配对请求拒绝、损坏图片拒绝、重复回执、离线失败、恢复重试、重启记录和配对恢复。

界面测试覆盖：预览、复制、发送快捷键、图片粘贴数据、不同设备草稿、离线状态、配对确认、远程图片默认不加载。界面测试模拟桌面 API，因此不等于已经验证 Windows 的系统剪贴板、托盘或通知。

CI 在 Windows 构建和运行传输测试，再生成 NSIS 安装包；Mac 生成 ARM64 DMG。Windows CI runner 是 Windows Server，Windows 11 实机安装和系统集成仍需在真实电脑验证。

## 0.2.0 文件附件

- 不限制扩展名，文件最大 100 MiB，一条消息最多 8 个附件、合计 200 MiB。空文件也可传输。
- PNG/JPEG/GIF/WebP 符合预览限制时保留图片体验；其他内容按普通文件显示文件名与大小，只能由用户选择路径另存。
- 复用已有 HTTPS、UUID 缓存、SHA-256 校验、SQLite 历史和重试机制，没有额外服务器或依赖。
- `images` 保留为 v1 存储和传输字段名，现在包含全部附件，避免迁移旧聊天数据。设备信息新增 `file_transfer` 能力标记；普通文件发往旧版时给出升级提示，文字和图片继续兼容。
- 原生选择和拖入从 Rust 读取路径，单次读取限制 100 MiB；附件准备过程中暂停发送和切换会话，避免发往错误电脑。
