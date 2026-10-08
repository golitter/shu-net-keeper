# Scheduler

本目录存放定时执行类工具。

## macOS WiFi

[`macos-wifi`](./macos-wifi/) 是独立于 Tauri 的 macOS 后台服务。在用户登录桌面和系统唤醒时检测指定 WiFi，连接后自动进行校园网认证。使用私有 `config.toml`，提供构建、安装和卸载脚本。

- 使用、权限设置及测试：[`macos-wifi/README.md`](./macos-wifi/README.md)
- 示例配置：`macos-wifi/config.example.toml`
- 安装：`bash macos-wifi/scripts/install.sh /absolute/path/config.toml`
- 卸载：`bash macos-wifi/scripts/uninstall.sh`

## Windows Timer

[`windows-timer`](./windows-timer/) 是 Windows 专用、仅使用指定以太网接口的 SHU 校园网单次检测与定时登录工具。

- 实现说明：[`windows-timer/docs/implementation.md`](./windows-timer/docs/implementation.md)
- 测试指南：[`windows-timer/docs/testing.md`](./windows-timer/docs/testing.md)
- 使用说明：[`windows-timer/README.md`](./windows-timer/README.md)
- 立即测试：`windows-timer/scripts/test-now.ps1`
- 注销认证：`windows-timer/scripts/logout-now.ps1`
- 安装任务：`windows-timer/scripts/install-task.ps1`
- 卸载任务：`windows-timer/scripts/uninstall-task.ps1`
