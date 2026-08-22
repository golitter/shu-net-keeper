# SHU Windows Ethernet Timer

Windows 专用、无 GUI、非驻留的上海大学校园网定时登录器。每次执行只检查一次，并且所有 HTTP 请求都绑定到指定以太网 IPv4。

完整的模块、登录流程、固定部署、任务设置和权限设计见 [`docs/implementation.md`](./docs/implementation.md)。

## 使用

需要 Windows 10/11、Rust 工具链和系统自带的 PowerShell、`curl.exe`。

```powershell
cd scheduler\windows-timer
Copy-Item config.example.toml config.toml
notepad config.toml
cargo build --release
```

先通过测试脚本立即执行一次。它不会创建或修改计划任务，也不需要管理员权限：

```powershell
.\scripts\test-now.ps1
```

如果 release 程序还不存在，测试脚本会先执行 `cargo build --release`。测试退出码为 `0` 表示以太网原本正常，或者校园网登录后已经恢复联网；非零表示测试失败。

确认日志结果正常后，以管理员身份运行：

```powershell
.\scripts\install-task.ps1
```

安装脚本会复制文件到固定目录：

```text
C:\ProgramData\SHUNetTimer\
├── shu-net-timer.exe
├── config.toml
└── logs\
```

计划任务引用固定目录，不依赖源码目录或 Cargo 的 `target`。安装后会在每天 12:00 和 Windows 启动后 60 秒各触发一次。执行失败时，每 2 分钟重试一次，最多重试 5 次。

安装后如需修改配置，请编辑：

```powershell
notepad C:\ProgramData\SHUNetTimer\config.toml
```

日志位于：

```text
C:\ProgramData\SHUNetTimer\logs\YYYY-MM-DD.log
```

卸载任务：

```powershell
.\scripts\uninstall-task.ps1
```

上面的命令只删除计划任务，保留程序、配置和日志。确定需要全部清除时运行：

```powershell
.\scripts\uninstall-task.ps1 -RemoveFiles
```

如果有线网卡不叫“以太网”，先运行 `Get-NetAdapter`，然后修改 `config.toml` 中的 `adapter_name`。

## 注意

- `config.toml` 包含校园网密码，已被 `.gitignore` 排除；不要提交或分享它。
- 安装脚本会收紧配置文件权限，并以 `SYSTEM` 运行计划任务，因此需要管理员权限。
- 程序不会禁用 Wi-Fi；它通过绑定以太网 IPv4，确保检测和登录请求不走 Wi-Fi。
- 程序会忽略 `_curlrc` 和 `HTTP_PROXY`/`HTTPS_PROXY` 等代理设置，避免校园网请求被发送至 Clash、v2rayN 等本机代理。
- 门户返回登录成功后，程序会按 2、3、5 秒的间隔验证公网，避免认证状态延迟造成误报。
- 门户如新增验证码或 MFA，需要另行适配。
