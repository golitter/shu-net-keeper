# macOS WiFi 启动／唤醒登录服务

独立于 Tauri 的 macOS 后台服务。登录系统后启动，开盖唤醒时再次检测；附近存在指定 SSID 时连接 WiFi，再使用学号和校园网密码完成 ePortal 认证。平时等待系统事件，不定时轮询。

## 运行方式

- Swift 使用 AppKit 监听系统睡眠、唤醒，使用 CoreWLAN 扫描和连接 WiFi，使用 CoreLocation 请求读取 SSID 所需的定位权限。
- Rust worker 读取私有 `config.toml`，检查校园网在线状态并认证，复用仓库 `src/rsa.rs` 的 RSA 算法和 `src/constants.rs` 的门户地址。
- 无窗口、无托盘、无 Tauri 依赖。`.app` 只是承载 macOS 权限说明和应用身份的原生包。
- 用户级 LaunchAgent 在**用户登录桌面后**运行，不在登录前运行。合盖后开盖引发系统唤醒时触发；仅打开盖子而系统未睡眠不会触发。
- 开机或唤醒后先等 3 秒，最多检测 `attempts` 次。目标 WiFi 不存在时也只进行这几次短重试，然后等待下次事件。
- 目标网络已连接则直接检查；附近找到目标网络但当前连接其他 WiFi 时，会切换到目标网络。WiFi 被手动关闭时跳过，不自动打开开关。
- 等待 DHCP 最多 15 秒／次；所有门户请求有超时。已在线则不重复登录；门户明确拒绝账号认证则停止本次重试。
- 重复事件合并，所有连接／认证操作串行；睡眠时取消排队任务并终止正在执行的认证进程组。文件锁防止手动运行和 LaunchAgent 同时执行。

## 配置

需要 macOS 13 或更高版本、Rust/Cargo 和 Xcode Command Line Tools（`xcode-select --install`）。构建和用户级服务安装不需要 Tauri、Node 或管理员权限；VPN/TUN 下的专用路由维护器需要一次管理员安装。

在本目录执行：

```bash
cp config.example.toml config.toml
chmod 600 config.toml
```

编辑 `config.toml`：

```toml
ssid = "Shu(ForAll)"
username = "你的学号"
password = "你的校园网密码"

# 仅当 WiFi 本身需要 WPA 密码时填写；开放校园 WiFi 不填写此项。
# wifi_password = "WiFi 密码"

attempts = 4
retry_seconds = 5
timeout_seconds = 10
```

**SSID 必须从 WiFi 菜单核对，大小写、空格、全角／半角括号都必须一致。** 示例为 `Shu(ForAll)`。`attempts` 范围为 1–10，两个秒数范围为 1–60。校园网密码和 WiFi 密码是两个独立字段。暂不支持 WPA Enterprise／802.1X WiFi 连接。

私有 `config.toml` 和备份已加入本目录 `.gitignore`；请勿把真实凭据填入 `config.example.toml`。Swift 与 Rust 之间使用管道传输设置，不生成 JSON 配置文件，不将设置输出到日志。

## 构建和安装

```bash
bash scripts/build.sh
bash scripts/install.sh "$PWD/config.toml"
```

安装脚本会先构建并验证配置，然后安装到：

```text
~/Applications/SHU WiFi Keeper.app
~/Library/Application Support/SHUWiFiKeeper/config.toml
~/Library/LaunchAgents/com.shu-net-keeper.macos-wifi.plist
~/Library/Logs/SHUWiFiKeeper/service.log
```

随后启动用户级 LaunchAgent。配置目录权限为 `700`，配置文件为 `600`。再次安装会更新程序；覆盖已有配置前保留 `config.toml.backup`。不传配置路径时使用已安装的配置。不要使用 `sudo`。

首次运行请允许 **SHU WiFi Keeper 的定位权限**，以读取附近 WiFi 名称；系统要求本地网络权限时也请允许，以访问校园网关。如果没有看到定位提示，可先停止服务，然后从应用包请求授权：

```bash
launchctl bootout "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
open "$HOME/Applications/SHU WiFi Keeper.app" --args --authorize-only
# 授权流程结束后重新启动：
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/com.shu-net-keeper.macos-wifi.plist"
```

`--authorize-only` 不连接 WiFi、不提交校园网凭据。也可在「系统设置 → 隐私与安全性 → 定位服务」检查授权，在「通用 → 登录项」检查是否允许后台运行。

本地构建默认使用 ad-hoc 签名。不同 macOS 版本的 SSID 隐私控制可能影响这种签名下的授权／扫描，必须在目标 Mac 上验证。分发时可使用正式 Developer ID 签名：

```bash
SHU_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" bash scripts/build.sh
```

应用更新后若权限失效，请重新检查定位授权。程序不会将无法读取 SSID 解释为已连接目标网络。

## 检查和测试

### VPN／代理隔离

所有校园网状态检测、网关 GET、认证 POST 和登录验证都使用系统 curl，并删除大小写代理环境变量、忽略 `.curlrc`、显式设置空代理和 `--noproxy '*'`。请求绑定 `if!en0`（实际 WiFi 接口由系统查询），每次请求前检查 WiFi IP 与接口专用路由。不会退回 VPN 默认路由。

VPN/TUN 若删除了 WiFi 默认路由，单独跳过 HTTP 代理不足以完成连接。安装固定目标的路由维护器：

```bash
sudo bash scripts/install-direct-route.sh
launchctl kickstart -k "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
```

维护器是 root 拥有的 LaunchDaemon，每 15 秒检查路由，无 WiFi 扫描或校园网 HTTP 请求。只维护 `10.10.9.9/32` 的 **WiFi 接口专用路由**，网关从 WiFi DHCP 自动读取；不修改 VPN 默认路由或系统代理。接口、IP 或网关变化时自动重新检查。没有可用的 WiFi 专用路由时，主服务报告明确原因并跳过认证。

root 维护器不读取用户配置或凭据，不接受来自用户文件的目标地址、网关或可执行命令。安装路径：`/Library/Application Support/SHUWiFiKeeper/direct-route.sh`、`/Library/LaunchDaemons/com.shu-net-keeper.macos-wifi.direct-route.plist`。路由变化日志：`/Library/Logs/SHUWiFiKeeper/direct-route.log`。

只读预览（不改变路由）：`bash scripts/direct-route.sh --dry-run`。停止路由维护并移除专用路由：`sudo bash scripts/uninstall-direct-route.sh`。如果曾安装维护器，卸载整个服务时也执行该命令。

### 服务测试

```bash
# 不需要账号、不会联网的自动检查
cargo test --manifest-path Cargo.toml
"dist/SHU WiFi Keeper.app/Contents/MacOS/shu-wifi-service" --self-test

# 配置检查，不发送登录请求
"dist/SHU WiFi Keeper.app/Contents/MacOS/shu-wifi-login" --check-config config.toml

# 只读诊断：不弹授权、不连接 WiFi、不认证
"$HOME/Applications/SHU WiFi Keeper.app/Contents/MacOS/shu-wifi-service" --diagnose

# 服务状态和实时日志
launchctl print "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
tail -f "$HOME/Library/Logs/SHUWiFiKeeper/service.log"
```

一次完整的手动连接／登录测试，需要先停止后台服务，避免实例锁冲突：

```bash
launchctl bootout "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
open -W "$HOME/Applications/SHU WiFi Keeper.app" --args --once
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/com.shu-net-keeper.macos-wifi.plist"
```

`--once` 使用已安装的配置；可追加 `--config /absolute/path/config.toml`。直接运行包内可执行文件可以看到终端日志／退出码，但授权测试优先通过 `open` 使用应用身份。退出码 `0` 表示已在线、登录成功或 WiFi 被关闭而跳过；`1` 表示检测／网络／配置失败；`2` 表示门户拒绝认证。

真实验收需要在校园环境中核对：启动后连接并认证、已经在线不重复认证、合盖后开盖恢复、目标 SSID 不存在时有限重试、定位被拒绝时提示、WiFi 关闭时跳过。单元测试和编译不代表这些硬件／校园网场景已通过。

日志不记录密码或 HTTP 响应正文。日志保存在本机 `service.log`，当前版本不自动轮转。

## 修改配置和卸载

修改配置后重启，让 WiFi 设置和认证凭据同时重新载入：

```bash
launchctl kickstart -k "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
```

卸载自动启动：

```bash
bash scripts/uninstall.sh
```

卸载会停止服务，把 LaunchAgent 文件改名为 `.plist.disabled`，保留应用、私有配置和日志，可重新安装恢复。

## 系统接口参考

- [NSWorkspace：系统睡眠／唤醒通知](https://developer.apple.com/documentation/appkit/nsworkspace)
- [CoreWLAN：扫描附近网络](https://developer.apple.com/documentation/corewlan/cwinterface/scanfornetworks(withssid:))
- [CoreWLAN：连接 WiFi](https://developer.apple.com/documentation/corewlan/cwinterface/associate(to:password:))
- [Apple：SSID 访问需要定位授权](https://developer.apple.com/forums/thread/732431)
