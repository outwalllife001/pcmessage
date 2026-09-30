# 现成项目调研

调研日期：2026-09-30。依据项目官方仓库、README 和官方框架文档；没有安装测试这些应用。下列适用性结论是针对本项目需求的判断。

需求以文字消息为主，支持 Markdown 排版和偶尔发送图片，不要求把文字保存成文件再发送。

| 项目 | 已有能力 | 对本项目的适用性 | 复用建议 |
| --- | --- | --- | --- |
| [LocalSend](https://github.com/localsend/localsend) | Windows/macOS 等平台，局域网直传文字和文件，无外部服务器；Apache-2.0 | 最适合直接尝试的现成工具。其主要定位是传输，是否满足聊天式记录和 Markdown 阅读体验需要试用确认 | 参考设备发现、传输和网络故障处理；不整体 fork |
| [PairDrop](https://github.com/schlagmichdoch/PairDrop) | 浏览器发送文字和文件，WebRTC/WebSocket，Node.js 服务端；GPL-3.0 | 无需安装即可使用，但运行模式依赖网站/信令服务，桌面常驻和聊天记录需要补齐 | 本次不作为客户端底座 |
| [Mesh-Talk](https://github.com/OctopusGarage/mesh-talk) | Tauri 2 + React + Rust，局域网聊天、文件、历史记录、托盘；MIT | 最接近桌面聊天的技术方向，但还包含群组、多设备身份、日志同步和离线中继等能力，超过家庭小工具的范围 | 参考桌面组织方式；不引入整套聊天协议 |
| [LAN Messenger](https://github.com/Felzy613/Lan-Messenger) | macOS SwiftUI、Windows WinUI，两套原生客户端，局域网聊天、文件、历史记录和远程桌面 | 功能接近，但需要维护两套界面和平台实现，且已经包含远程桌面 | 参考交互。README 标注 MIT，但 GitHub API 未识别许可证，复制代码前需核实实际授权文件 |

## 选择

如果只需要立刻发送文字和图片，优先试用 LocalSend，可能已经够用。

如果要开发 PCMessage，建议从小型 Tauri 项目开始，集中实现文字聊天、Markdown 阅读和截图粘贴。调研未发现可以直接确认满足这些需求、又足够简单的项目；裁剪上述完整应用不一定比小范围开发省事。

复用以成熟库为主：

- [Tauri 2](https://v2.tauri.app/)：Windows/macOS 桌面外壳，使用系统 WebView，一套业务代码。
- [markdown-it](https://github.com/markdown-it/markdown-it)：Markdown 解析和渲染，MIT。
- [DOMPurify](https://github.com/cure53/DOMPurify)：清理渲染后的 HTML。
- [LocalSend 协议文档](https://github.com/localsend/protocol)：参考 UDP 组播发现和直接传输的设计。参考设计不等于实现协议兼容，第一版不承诺与 LocalSend 互通。

没有复制这些项目的代码。具体网络库和版本在初始化工程时锁定。安装包大小和运行内存需要实测，不根据框架宣传数字作保证。
