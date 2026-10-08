# 独立后台工具（Scheduler）

本目录包含两个独立于主项目 Tauri GUI 的工具：Windows 使用任务计划程序定时检测以太网；macOS 使用后台服务在登录桌面和系统唤醒时检测 WiFi。两个版本分别构建、安装和配置，私有 `config.toml` 不通用。

## macOS WiFi

[`macos-wifi`](./macos-wifi/) 是独立于 Tauri 的 macOS 后台服务。在用户登录桌面和系统唤醒时检测指定 WiFi，连接后自动进行校园网认证。使用私有 `config.toml`，提供构建、安装和卸载脚本。

- 使用、权限设置及测试：[`macos-wifi/README.md`](./macos-wifi/README.md)
- 示例配置：`macos-wifi/config.example.toml`
- 安装：`bash macos-wifi/scripts/install.sh /absolute/path/config.toml`
- VPN/TUN 专用路由维护器：`sudo bash macos-wifi/scripts/install-direct-route.sh`
- 卸载：`bash macos-wifi/scripts/uninstall.sh`
- 已安装路由维护器时，还需卸载它：`sudo bash macos-wifi/scripts/uninstall-direct-route.sh`

以上命令在 `scheduler/` 目录执行。用户级 WiFi 服务不加 `sudo`；路由维护器需要管理员权限。安装后无需每次手动启动，重启电脑并登录桌面、合盖睡眠后开盖都会触发检测；已连接或已认证时跳过对应操作。

## Windows Timer

[`windows-timer`](./windows-timer/) 是 Windows 专用、仅使用指定以太网接口的 SHU 校园网单次检测与定时登录工具。

- 实现说明：[`windows-timer/docs/implementation.md`](./windows-timer/docs/implementation.md)
- 测试指南：[`windows-timer/docs/testing.md`](./windows-timer/docs/testing.md)
- 使用说明：[`windows-timer/README.md`](./windows-timer/README.md)
- 立即测试：`windows-timer/scripts/test-now.ps1`
- 注销认证：`windows-timer/scripts/logout-now.ps1`
- 安装任务：`windows-timer/scripts/install-task.ps1`
- 卸载任务：`windows-timer/scripts/uninstall-task.ps1`
