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

Windows 11 x64 的源码与 NSIS 构建工作流已经提交，但安装包尚未生成，也未在 Windows 11 实机验证。GitHub Actions 在执行任何步骤之前被账户账单锁定阻止，见[构建记录](https://github.com/outwalllife001/pcmessage/actions/runs/36669570558)。本次按用户要求先交付 Mac 与源码，不更改账户账单设置。
