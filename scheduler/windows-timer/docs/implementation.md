# Windows 以太网定时登录器实现说明

## 1. 实现范围

`shu-net-timer` 是一个独立的 Windows Rust 程序，用于通过指定以太网接口检测并恢复上海大学校园网连接。

它采用“运行一次后退出”的方式，不包含 GUI、托盘、邮件通知和进程内常驻轮询。安装后每 30 分钟持续执行、开机执行、错过任务补跑以及失败重试均由 Windows 任务计划程序负责。

## 2. 源码结构

```text
scheduler/
└── windows-timer/
    ├── Cargo.toml
    ├── Cargo.lock
    ├── config.example.toml
    ├── README.md
    ├── docs/
    │   ├── implementation.md
    │   └── testing.md
    ├── scripts/
    │   ├── test-now.ps1
    │   ├── logout-now.ps1
    │   ├── install-task.ps1
    │   └── uninstall-task.ps1
    └── src/
        ├── config.rs
        ├── curl.rs
        ├── ethernet.rs
        ├── login.rs
        ├── main.rs
        └── rsa.rs
```

该目录通过自身的 `[workspace]` 声明成为独立 Rust crate，不加入项目根目录中包含 GUI 的 Cargo workspace。

## 3. Rust 模块

### `main.rs`

负责一次完整的执行流程：

1. 从命令行 `--config` 指定的位置读取配置；未指定时依次查找当前目录和可执行文件目录中的 `config.toml`；
2. 获取指定适配器的 IPv4；
3. 绑定该 IPv4 检测公网；
4. 公网正常时记录日志并退出；
5. 公网不可用时调用 ePortal 登录；
6. 登录请求成功后分别等待 2、3、5 秒，最多进行三次公网验证；
7. 成功返回退出码 `0`，失败返回非零退出码。

日志写入可执行文件所在目录的 `logs/YYYY-MM-DD.log`。每次写日志时会按文件名日期清理超过 30 天的旧 `.log` 文件；其他名称的文件不会被删除。

### `config.rs`

使用 TOML 和 Serde 加载以下配置：

```toml
username = "12345678"
password = "校园网密码"
adapter_name = "以太网"
timeout_seconds = 15
```

模块会检查用户名、密码和适配器名称非空，并将请求超时限制在 1 到 120 秒。

### `ethernet.rs`

通过 `windows-sys` 直接调用 Windows IP Helper API `GetAdaptersAddresses`：

- 仅请求 IPv4 适配器信息；
- 使用微软建议的 15 KB 初始缓冲区，并处理 `ERROR_BUFFER_OVERFLOW`；
- 按 Windows 友好名称匹配配置中的适配器；
- 要求接口类型为 Ethernet，运行状态为 `Up`；
- 遍历单播地址并排除 `169.254.*` 链路本地地址和未指定地址；
- 将选中的 IPv4 作为所有 curl 请求的本地源地址。

该流程不启动 `powershell.exe`，也不依赖 CIM 查询权限。

公网检测地址为：

```text
http://www.msftconnecttest.com/connecttest.txt
```

只有响应正文为 `Microsoft Connect Test` 才判定为联网。

curl 调用包含：

```text
--disable
--noproxy "*"
--interface <以太网 IPv4>
```

其中 `--disable` 忽略用户 `_curlrc`，`--noproxy "*"` 跳过 Clash、v2rayN 等系统或环境代理，`--interface` 将请求绑定到指定以太网 IPv4。

### `curl.rs`

集中创建所有 curl 子进程。程序不会通过 `PATH` 搜索同名命令，而是根据 `SystemRoot` 组装：

```text
<SystemRoot>\System32\curl.exe
```

这可以减少普通 `PATH` 同名程序截获请求参数的风险。HTTP 仍由独立的系统 `curl.exe` 子进程执行，并非 Rust 进程内 HTTP 客户端。

### `login.rs`

登录流程与学校 ePortal 页面一致：

1. 绑定以太网 IPv4 访问 `http://10.10.9.9`；
2. 从页面 JavaScript 跳转中提取动态登录 URL；
3. 提取完整 query string 及其中的 `mac`；
4. 生成 `password>mac` 并调用 RSA 加密；
5. 保持 GET 阶段产生的 Cookie；
6. 向 `/eportal/InterFace.do?method=login` 提交表单；
7. 固定提交 `service=shu` 和 `passwordEncrypt=true`；
8. 登录表单正文通过子进程 stdin 传入，用户名和加密密码不会出现在 curl 命令行；
9. 使用独立写入线程和 `wait_with_output` 同时处理 stdin、stdout 和 stderr，避免管道互相等待；
10. 解析 JSON 响应中的 `result` 和 `message`。

程序不会写死成功页中的 `userIndex`，也不打开浏览器。

### `rsa.rs`

实现 ePortal 页面所需的 JavaScript 兼容 RSA 算法：

- 使用与门户一致的公钥模数和指数；
- 反转 `password>mac` 字符串；
- 按门户算法进行零填充、分块和模幂计算；
- 用单元测试验证输出与原项目的已知结果完全一致。

## 4. 测试与注销脚本

`scripts/test-now.ps1` 不创建或修改计划任务，可以在安装前立即执行一次完整流程。

脚本会：

- 检查 `config.toml` 是否存在；
- release 程序不存在时调用 `cargo build --release`；
- 使用 `--config` 运行 Rust 程序；
- 原样返回 Rust 程序的退出码；
- 输出日志目录。

所有 PowerShell 脚本均使用纯 ASCII 内容，以兼容 Windows PowerShell 5.1 对无 BOM UTF-8 脚本的处理。

`scripts/logout-now.ps1` 用于真实登录测试。它通过门户的 `getOnlineUserInfo` 动态取得当前 `userIndex`，确认后向 `InterFace.do?method=logout` 提交注销请求。脚本不读取校园网密码，不断开以太网，并在注销后检查在线会话是否已经消失。

门户返回 UTF-8 JSON，而 Windows PowerShell 5.1 会按当前系统代码页解释原生命令标准输出。为防止中文字段变成乱码并破坏 JSON，脚本让 curl 将原始响应写入随机临时文件，再使用 `[Text.Encoding]::UTF8` 显式读取，最后在 `finally` 中删除临时文件。

注销脚本提供两种安全控制：

- `-CheckOnly`：只确认以太网和在线会话，不发送注销请求；
- 默认模式：要求手工输入大写 `YES` 后才执行注销；`-Force` 可供明确需要的自动化调用跳过确认。

完整测试操作见 [`testing.md`](./testing.md)。

## 5. 固定部署与计划任务

`scripts/install-task.ps1` 需要管理员权限。它将构建产物和配置复制到：

```text
C:\ProgramData\SHUNetTimer\
├── shu-net-timer.exe
├── config.toml
└── logs\
```

随后创建或更新任务计划程序库根目录中的任务：

```text
SHU Net Keeper (Ethernet)
```

任务设置如下：

| 项目 | 实现值 |
|---|---|
| 运行身份 | `SYSTEM` |
| 连续触发 | 安装后 1 分钟首次执行，之后每 30 分钟重复且没有结束日期 |
| 启动触发 | Windows 启动后延迟 60 秒 |
| 错过任务 | `StartWhenAvailable = true` |
| 失败重试 | 每 2 分钟一次，最多 5 次 |
| 最大运行时间 | 5 分钟 |
| 并发策略 | `IgnoreNew` |

任务动作固定为：

```text
Executable: C:\ProgramData\SHUNetTimer\shu-net-timer.exe
Arguments:  --config "C:\ProgramData\SHUNetTimer\config.toml"
Working directory: C:\ProgramData\SHUNetTimer
```

因此安装完成后，移动源码目录或执行 `cargo clean` 不会破坏计划任务。

## 6. 文件权限

安装脚本关闭 `C:\ProgramData\SHUNetTimer` 的继承权限：

- 安装用户可以读取程序和日志，并修改 `config.toml`；
- `SYSTEM` 拥有完全控制权限，以便运行程序和创建日志；
- Administrators 拥有完全控制权限；
- 其他普通用户不获得访问权限。

`config.toml` 中的密码仍是明文，只通过 NTFS ACL 限制读取，不属于加密存储。

## 7. 卸载行为

以下命令只删除计划任务，保留程序、配置和日志：

```powershell
.\scripts\uninstall-task.ps1
```

显式增加 `-RemoveFiles` 才会删除固定部署目录：

```powershell
.\scripts\uninstall-task.ps1 -RemoveFiles
```

删除前会验证目标路径严格等于 `C:\ProgramData\SHUNetTimer`，避免递归删除意外目录。

## 8. 当前测试

项目当前包含并通过以下 Rust 单元测试：

- ePortal JavaScript 跳转地址和 MAC 参数解析；
- RSA 加密结果与已知门户结果一致。

开发期间还执行了：

- `cargo test`；
- `cargo clippy --all-targets -- -D warnings`；
- release 构建；
- Windows PowerShell 5.1 脚本解析；
- 注销脚本 `-CheckOnly` 实机验证；
- 门户 UTF-8 JSON 原始字节读取与解析；
- 指定以太网 IPv4 且绕过代理的公网直连检测；
- 在真实 Windows 环境中通过 `GetAdaptersAddresses` 找到“以太网”和正确 IPv4；
- 计划任务动作、触发器和失败重试参数的内存构造检查。

原 PowerShell 网卡查询版本的单次进程树峰值约为 174.5 MiB、运行约 3.24 秒；改用 Windows API 后实测峰值约为 23.5 MiB、运行约 0.68 秒。Rust 主进程峰值约为 6.3 MiB，其余主要来自短暂运行的 `curl.exe` 和控制台宿主。

## 9. 已知边界

- 不自动启用被管理员禁用的网卡；
- 不断开或禁用 Wi-Fi；
- 不处理验证码、短信或 MFA；
- ePortal 地址、RSA 公钥或页面结构变化时需要更新代码；
- 公网状态目前只使用一个微软检测地址；
- 凭据尚未使用 Windows DPAPI 加密。
- HTTP 仍依赖 Windows 自带的 `curl.exe` 子进程，尚未达到完全进程内 HTTP；
- `curl.rs` 的系统目录基于进程环境中的 `SystemRoot`，不是通过 `GetSystemDirectoryW` 获取。
