# macOS WiFi 启动／唤醒登录服务

独立于 Tauri 的 macOS 后台服务。登录系统后启动，开盖唤醒时再次检测；附近存在指定 SSID 时连接 WiFi，再使用学号和校园网密码完成 ePortal 认证。WiFi 检测服务平时等待系统事件，不定时轮询；可选的特权路由维护器接受即时刷新通知，并每 15 秒检查一次路由兜底。

## 运行方式

- Swift 使用 AppKit 监听系统睡眠、唤醒，使用 CoreWLAN 扫描和连接 WiFi，使用 CoreLocation 请求读取 SSID 所需的定位权限。
- Rust worker 读取私有 `config.toml`，检查校园网在线状态并认证，复用仓库 `src/rsa.rs` 的 RSA 算法和 `src/constants.rs` 的门户地址。
- 无窗口、无托盘、无 Tauri 依赖。`.app` 只是承载 macOS 权限说明和应用身份的原生包。
- 用户级 LaunchAgent 在**用户登录桌面后**运行，不在登录前运行。关机后开机和重新启动都适用；从系统启动到用户会话建立的时间不属于连接耗时。合盖后开盖引发系统唤醒时触发；仅打开盖子而系统未睡眠不会触发。
- 开机或唤醒后立即检测，首先执行 `attempts` 次基础尝试。第一次优先使用系统缓存的目标网络，缓存缺失或后续重试才主动扫描。基础尝试间按 0.25、0.5、1 秒的短间隔重试；已关联目标 WiFi 时允许在恢复窗口内继续重试，避免网关尚未恢复就提前结束。一直找不到目标 WiFi 时只执行基础尝试。
- 目标网络已连接则直接检查；附近找到目标网络但当前连接其他 WiFi 时，会切换到目标网络。WiFi 被手动关闭时跳过，不自动打开开关。
- 等待 DHCP 最多 15 秒／次，每 0.1 秒重新检查。门户连接超时最多 2 秒，单次 HTTP 请求总超时由 `timeout_seconds` 控制。认证成功后立即验证，未生效时每 0.25 秒重查，最多 3 次。已在线则不重复登录；门户明确拒绝账号认证则停止本次重试。
- 重复事件合并，所有连接／认证操作串行；睡眠时取消排队任务并终止正在执行的认证进程组。文件锁防止手动运行和 LaunchAgent 同时执行。

## 配置

需要 macOS 13 或更高版本、Rust/Cargo 和 Xcode Command Line Tools（`xcode-select --install`）。构建和用户级服务安装不需要 Tauri、Node 或管理员权限；VPN/TUN 下的专用路由维护器需要一次管理员安装。

以下命令除特别说明外，均从仓库根目录进入本目录后执行：

```bash
cd scheduler/macos-wifi
```

首次创建私有配置：

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
retry_seconds = 1
recovery_seconds = 30
timeout_seconds = 10
```

**SSID 必须从 WiFi 菜单核对，大小写、空格、全角／半角括号都必须一致。** 示例为 `Shu(ForAll)`。`attempts` 范围为 1–10，`retry_seconds` 和 `timeout_seconds` 范围为 1–60，`recovery_seconds` 范围为 0–120，省略时默认 30 秒，设为 0 可关闭恢复阶段。校园网密码和 WiFi 密码是两个独立字段。暂不支持 WPA Enterprise／802.1X WiFi 连接。

私有 `config.toml` 和备份已加入本目录 `.gitignore`；请勿把真实凭据填入 `config.example.toml`。Swift 与 Rust 之间使用管道传输设置，不生成 JSON 配置文件，不将设置输出到日志。

基础尝试间的实际重试间隔不超过 1 秒；超过基础次数后，恢复阶段的间隔为 `min(retry_seconds, 2)` 秒。恢复窗口从首次观察到目标 WiFi 已关联时开始计时，只允许窗口内启动额外尝试，正在执行的尝试仍使用自身的 DHCP／HTTP 超时。目标网络已就绪时尽量在 5 秒内完成检测／连接／认证；系统扫描、关联、DHCP 和门户响应时间不受本程序控制，因此不保证所有环境都在 5 秒内成功。旧配置无需新增字段即可使用默认 30 秒恢复窗口。

## 构建和安装

```bash
# VPN/TUN 可能接管校园网关路由时，先安装专用路由维护器（一次管理员授权）
sudo bash scripts/install-direct-route.sh

# 安装用户级 WiFi 服务（不要加 sudo；脚本会自动构建）
bash scripts/install.sh "$PWD/config.toml"
```

没有 VPN/TUN 且 WiFi 已有可用直连路由时，可以跳过第一条命令。只需要构建、不安装或启动服务时执行 `bash scripts/build.sh`，产物位于 `dist/SHU WiFi Keeper.app`。

安装脚本会先构建并验证配置，然后安装到：

```text
~/Applications/SHU WiFi Keeper.app
~/Library/Application Support/SHUWiFiKeeper/config.toml
~/Library/LaunchAgents/com.shu-net-keeper.macos-wifi.plist
~/Library/Logs/SHUWiFiKeeper/service.log
```

随后启动用户级 LaunchAgent。配置目录权限为 `700`，配置文件为 `600`。再次安装会更新程序；覆盖已有配置前保留 `config.toml.backup`。不传配置路径时使用已安装的配置。`install.sh` 不使用 `sudo`，特权路由维护器的安装脚本使用 `sudo`。

安装后无需手动运行：重启并登录 macOS 后自动启动，合盖睡眠后开盖自动检测。安装脚本将源码目录中的配置**复制**到上述安装位置；安装后只修改源码目录的 `config.toml` 不会改变当前服务配置，需要重新安装或直接编辑安装后的文件。

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

所有校园网状态检测、网关 GET、认证 POST 和登录验证都使用系统 curl，并删除大小写代理环境变量、忽略 `.curlrc`、显式设置空代理和 `--noproxy '*'`。请求绑定 `if!en0`（实际 WiFi 接口由系统查询），在线状态查询和认证提交前检查 WiFi IP、当前 DHCP 网关与接口专用路由的一致性；检查过程中租约变化会重新确认。旧网关对应的路由不能仅凭接口是 `en0` 就被判定为可用。不会退回 VPN 默认路由。

VPN/TUN 若删除了 WiFi 默认路由，单独跳过 HTTP 代理不足以完成连接。安装固定目标的路由维护器：

```bash
sudo bash scripts/install-direct-route.sh
launchctl kickstart -k "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
```

维护器是 root 拥有的 LaunchDaemon，无 WiFi 扫描或校园网 HTTP 请求。主服务发现路由与当前 DHCP 网关不一致时，修改固定的 `route-request` 通知文件；LaunchDaemon 的 `WatchPaths` 会触发即时检查，无需等待定时轮询。任务的 `ThrottleInterval` 显式设为 1 秒，避免默认启动节流叠加等待，同时每 15 秒定时检查兜底。只维护 `10.10.9.9/32` 的 **WiFi 接口专用路由**，网关从 WiFi DHCP 自动读取；不修改 VPN 默认路由或系统代理。主服务每次最多等待 3 秒让路由匹配当前网关，仍不匹配时不提交凭据，再按恢复窗口有限重试。

从旧版升级时重新运行 `sudo bash scripts/install-direct-route.sh`，安装通知文件并更新 LaunchDaemon 的即时触发和启动节流设置，再运行 `bash scripts/install.sh` 更新用户级服务；源码中的任务设置不会自动更新已安装的任务。

root 维护器不读取用户配置或凭据，不接受来自用户文件的目标地址、网关或可执行命令。安装路径：`/Library/Application Support/SHUWiFiKeeper/direct-route.sh`、`/Library/LaunchDaemons/com.shu-net-keeper.macos-wifi.direct-route.plist`。路由变化日志：`/Library/Logs/SHUWiFiKeeper/direct-route.log`。

通知文件由 root 创建在不可由普通用户替换的目录中，只允许写入固定刷新信号，维护器不读取其内容。主服务拒绝符号链接和非 root 拥有的通知文件。手动发送通知（不认证）：`"dist/SHU WiFi Keeper.app/Contents/MacOS/shu-wifi-login" --refresh-route`。

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

立即触发一次检测（不注销校园网、不关闭 WiFi）：

```bash
launchctl kickstart -k "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
```

### 分别验证连接、唤醒和认证

- **开机／重启**：登录桌面后查看新的“程序启动／权限已就绪”记录，以及在线状态或登录结果。
- **合盖／开盖**：应出现“系统进入睡眠”和“系统唤醒”记录。合盖不保证校园网认证失效；“已登录校园网，无需重复认证”同样是成功结果。
- **热点切换**：先手动连接手机热点，再触发检测。若附近存在目标 SSID，服务会切换回校园 WiFi；目标不存在时有限重试后保留现有连接。这项测试不保证产生新的认证请求。
- **真实认证**：保持校园 WiFi 连接，在校园网门户 `http://10.10.9.9` 主动注销会话，再触发检测；预期出现“校园网登录成功，在线状态已验证”。浏览器没有服务的专用接口绑定，VPN/TUN 开启时可能无法直接访问门户。Windows 的 `logout-now.ps1` 不适用于 macOS，本版本尚未提供独立注销脚本。

日志中的 `[...Z]` 时间戳为 UTC，精确到毫秒，北京时间需加 8 小时。阶段日志中的“耗时”从本次启动／唤醒检测被触发时开始累计，可分别看到扫描、WiFi 关联、IPv4 就绪和检测完成的时间。Rust worker 的结果行没有时间戳，但紧接着的“本次检测成功完成”会记录总耗时。确认“发现目标 WiFi”或“请求已隔离代理”还不足以认定认证成功，应检查后续在线状态或成功记录。

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

编辑安装后的配置文件，而不是只编辑源码目录中的副本：

```bash
open -e "$HOME/Library/Application Support/SHUWiFiKeeper/config.toml"
```

修改后重启，让 WiFi 设置和认证凭据同时重新载入，也会立即触发一次检测：

```bash
launchctl kickstart -k "gui/$(id -u)/com.shu-net-keeper.macos-wifi"
```

卸载用户级自动启动；如果安装过专用路由维护器，也执行第二条命令：

```bash
bash scripts/uninstall.sh
sudo bash scripts/uninstall-direct-route.sh
```

用户级卸载会停止服务，把 LaunchAgent 文件改名为 `.plist.disabled`，保留应用、私有配置和日志，可重新安装恢复。路由维护器的卸载会停止 LaunchDaemon、移除当前 WiFi 接口的校园网关专用路由，并保留辅助文件和日志。

## 常见问题

- **“附近未发现目标 WiFi”**：核对 SSID 的大小写、空格和括号，例如 `Shu(ForAll)` 与 `Shu(ForALL)` 不同；检查定位权限和实际信号范围。
- **“WiFi DHCP 网关与专用路由尚未就绪或不一致”**：切换热点／校园网后可能还保留旧网关路由；服务会等待并有限重试。持续出现时按上面的 VPN／代理隔离步骤安装维护器，检查 `sudo launchctl print system/com.shu-net-keeper.macos-wifi.direct-route`。它既是每 15 秒的周期性任务，也会被 `route-request` 通知文件即时触发，两次运行之间显示 `state = not running` 正常，应同时检查 `last exit code`。
- **“校园网请求失败（curl 退出码 Some(7)）”**：连接网关失败，尚不能归因于账号密码。检查 WiFi 是否获取 IP、专用路由是否可用及本地网络权限，不要仅重复修改密码。
- **开盖后仍显示已登录**：无需重连或重新认证。合盖多久会失去无线关联或校园网会话没有固定保证，开盖时也可能已由系统恢复连接。
- **私有配置或日志管理**：密码仍以明文保存在受权限保护的 TOML 中，不属于加密存储；请勿分享配置。当前日志不自动轮转。

## 系统接口参考

- [NSWorkspace：系统睡眠／唤醒通知](https://developer.apple.com/documentation/appkit/nsworkspace)
- [CoreWLAN：扫描附近网络](https://developer.apple.com/documentation/corewlan/cwinterface/scanfornetworks(withssid:))
- [CoreWLAN：连接 WiFi](https://developer.apple.com/documentation/corewlan/cwinterface/associate(to:password:))
- [Apple：SSID 访问需要定位授权](https://developer.apple.com/forums/thread/732431)
