# Windows 以太网定时登录器测试指南

## 1. 测试目标

测试分为四类：

1. 已联网时应直接退出；
2. 校园网会话注销后应自动重新登录；
3. 物理网线断开时应明确报错；
4. 已安装的计划任务失败后应按设置重试。

真实登录测试必须保持网线连接。拔掉网线只能测试物理断线处理，无法测试校园网认证。

## 2. 测试前准备

进入项目目录：

从仓库根目录进入：

```powershell
cd scheduler\windows-timer
```

确认 `config.toml` 已填写：

```toml
username = "你的学号"
password = "你的校园网密码"
adapter_name = "以太网"
timeout_seconds = 15
```

建议在测试期间断开 Wi-Fi，但程序本身仍会将 HTTP 请求绑定到配置的以太网 IPv4。

## 3. 已联网快速测试

执行：

```powershell
.\scripts\test-now.ps1
```

已联网时预期输出：

```text
开始单次检测，适配器：以太网
以太网 IPv4：<当前地址>
网络正常，无需登录
Test passed: Ethernet is online.
```

该脚本不会创建或修改计划任务。

## 4. 注销脚本安全检查

先使用只读模式：

```powershell
.\scripts\logout-now.ps1 -CheckOnly
```

预期输出：

```text
Adapter: 以太网
Ethernet IPv4: <当前地址>
An active SHU portal session was found.
Check-only mode: no logout request was sent.
```

`-CheckOnly` 不会注销网络。

## 5. 真实注销与自动登录测试

如果计划任务已经安装，为避免它在测试期间抢先自动登录，可以先用管理员 PowerShell 暂时禁用：

```powershell
Disable-ScheduledTask -TaskName 'SHU Net Keeper (Ethernet)'
```

执行真实注销：

```powershell
.\scripts\logout-now.ps1
```

根据提示输入：

```text
YES
```

成功输出应为：

```text
Logout succeeded. The Ethernet link remains connected, but portal authentication is now offline.
```

随后立即执行自动登录：

```powershell
.\scripts\test-now.ps1
```

预期关键日志：

```text
以太网无法访问公网，开始登录 Shu(ForAll)
登录请求成功，等待校园网认证状态生效
第 1 次联网验证成功，公网连接已经恢复
```

认证状态生效较慢时，也可能在第 2 次或第 3 次验证恢复，这仍然属于成功。

测试结束后重新启用任务：

```powershell
Enable-ScheduledTask -TaskName 'SHU Net Keeper (Ethernet)'
```

不要复制或分享 `getOnlineUserInfo` 的完整响应，其中的 `userIndex` 是当前临时会话标识。

## 6. 物理断线测试

拔掉网线后执行：

```powershell
.\scripts\test-now.ps1
```

预期程序报告指定以太网适配器未连接，并返回非零退出码。程序不会自动修复未插网线、交换机断开或被管理员禁用的网卡。

重新插入网线，等待 Windows 获得 IPv4 后再次运行测试脚本。

## 7. 计划任务安装与重试测试

该测试要求已经使用管理员 PowerShell 安装任务：

```powershell
.\scripts\install-task.ps1
```

安装成功后会立即在后台执行一次检测。连续运行两次安装命令，第二次也应正常覆盖文件、更新任务并启动检测，不应出现 `Copy-Item` 权限错误或任务 XML 时长错误：

```powershell
.\scripts\install-task.ps1
.\scripts\install-task.ps1
```

两次即时检测都会写入当天日志。同一时刻如果已有检测仍在运行，任务计划程序使用 `IgnoreNew` 避免创建重叠实例。

测试步骤：

1. 拔掉网线；
2. 手动启动任务；
3. 等待第一次执行失败；
4. 在两分钟内重新插入网线；
5. 等待任务计划程序自动重试；
6. 查看固定部署目录中的日志。

手动启动：

```powershell
Start-ScheduledTask -TaskName 'SHU Net Keeper (Ethernet)'
```

查看任务状态：

```powershell
Get-ScheduledTaskInfo -TaskName 'SHU Net Keeper (Ethernet)' |
    Format-List LastRunTime, LastTaskResult, NextRunTime
```

查看日志：

```powershell
Get-Content C:\ProgramData\SHUNetTimer\logs\*.log -Tail 50
```

任务失败时每 2 分钟重试一次，最多 5 次。每 30 分钟的常规触发与失败重试是两套独立机制。

## 8. 常见错误

### `ConvertFrom-Json` 报错

当前注销脚本已经让 curl 写入原始响应文件并按 UTF-8 读取。如果仍然出现此错误，应确认执行的是仓库中最新的 `scripts/logout-now.ps1`。

### curl 尝试连接 `127.0.0.1:7890`

当前 Rust 程序和注销脚本均传入 `--noproxy "*"`，不会使用 Clash、v2rayN 等代理。如果仍然出现，应确认运行的是最新 release，并重新执行：

```powershell
cargo build --release
```

### `Get-NetAdapter` 拒绝访问

定时运行的 Rust 程序直接使用 Windows API，不调用 `Get-NetAdapter`。只有一次性的注销 PowerShell 脚本使用该命令；如果桌面 PowerShell受到策略限制，可以使用管理员 PowerShell运行注销测试。

## 9. 测试后的日志

源码目录立即测试日志：

```text
scheduler\windows-timer\target\release\logs\YYYY-MM-DD.log
```

安装后的计划任务日志：

```text
C:\ProgramData\SHUNetTimer\logs\YYYY-MM-DD.log
```

日志按日期命名，并自动清理超过 30 天的旧日志。
