import AppKit
import CoreLocation
import CoreWLAN
import Foundation
import Darwin

func log(_ message: String) {
    let timestamp = ISO8601DateFormatter().string(from: Date())
    print("[\(timestamp)] \(message)")
    fflush(stdout)
}

struct Settings: Decodable {
    let ssid: String
    let wifi_password: String?
    let attempts: Int
    let retry_seconds: Int
    let timeout_seconds: Int

    func validate() throws {
        guard !ssid.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              (1...10).contains(attempts), (1...60).contains(retry_seconds),
              (1...60).contains(timeout_seconds) else {
            throw ServiceError.message("SSID 或重试参数无效")
        }
    }
}

enum ServiceError: Error, CustomStringConvertible {
    case message(String)
    var description: String {
        switch self { case .message(let text): return text }
    }
}

// A generation invalidates queued checks on sleep, wake, or permission revocation.
// Only one serial worker runs; an in-flight curl worker is terminated on sleep.
final class WorkState: @unchecked Sendable {
    private let lock = NSLock()
    private var generation = 0
    private var child: Process?

    func invalidate() -> Int {
        lock.lock()
        generation += 1
        let token = generation
        let oldChild = child
        child = nil
        lock.unlock()
        if let process = oldChild, process.isRunning {
            // The Rust worker creates its own process group before spawning curl.
            if kill(-process.processIdentifier, SIGTERM) != 0 { process.terminate() }
        }
        return token
    }

    func current(_ token: Int) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return token == generation
    }

    func launch(_ process: Process, token: Int) throws -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard token == generation else { return false }
        try process.run()
        child = process
        return true
    }

    func clear(_ process: Process) {
        lock.lock(); defer { lock.unlock() }
        if child === process { child = nil }
    }

    func pause(seconds: Int, token: Int) -> Bool {
        for _ in 0..<(seconds * 4) {
            if !current(token) { return false }
            Thread.sleep(forTimeInterval: 0.25)
        }
        return current(token)
    }
}

func defaultConfigURL() -> URL {
    FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/SHUWiFiKeeper/config.toml")
}

func workerURL() -> URL {
    Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/shu-wifi-login")
}

func acquireInstanceLock() throws -> Int32 {
    let directory = defaultConfigURL().deletingLastPathComponent()
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                           attributes: [.posixPermissions: 0o700])
    let descriptor = open(directory.appendingPathComponent("service.lock").path,
                          O_CREAT | O_RDWR | O_NOFOLLOW, mode_t(0o600))
    guard descriptor >= 0 else { throw ServiceError.message("无法打开服务锁文件") }
    guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
        close(descriptor)
        throw ServiceError.message("服务已经运行；请先停止 LaunchAgent 再手动测试")
    }
    // Don't leak the lock to authentication subprocesses.
    _ = fcntl(descriptor, F_SETFD, FD_CLOEXEC)
    return descriptor
}

final class Service: NSObject, CLLocationManagerDelegate {
    private let location = CLLocationManager()
    private let queue = DispatchQueue(label: "com.shu-net-keeper.macos-wifi.check", qos: .utility)
    private let work = WorkState()
    private let configURL: URL
    private let settings: Settings?
    private let once: Bool
    private let authorizeOnly: Bool
    private var observers: [NSObjectProtocol] = []
    private var authorized = false
    private var sleeping = false
    private var authorizationTimeout: DispatchWorkItem?

    init(configURL: URL, settings: Settings?, once: Bool, authorizeOnly: Bool) {
        self.configURL = configURL
        self.settings = settings
        self.once = once
        self.authorizeOnly = authorizeOnly
        super.init()
    }

    func start() {
        location.delegate = self
        let center = NSWorkspace.shared.notificationCenter
        observers.append(center.addObserver(forName: NSWorkspace.willSleepNotification,
                                            object: nil, queue: .main) { [weak self] _ in
            guard let self = self else { return }
            self.sleeping = true
            _ = self.work.invalidate()
            log("系统进入睡眠，取消本次检测")
        })
        observers.append(center.addObserver(forName: NSWorkspace.didWakeNotification,
                                            object: nil, queue: .main) { [weak self] _ in
            guard let self = self else { return }
            self.sleeping = false
            if self.authorized { self.schedule(reason: "系统唤醒") }
        })
        if location.authorizationStatus == .notDetermined {
            log("首次运行需要定位权限，用于读取附近 WiFi 的名称；等待用户授权")
            // macOS uses Always authorization, with NSLocationUsageDescription.
            location.requestAlwaysAuthorization()
            if once || authorizeOnly {
                let timeout = DispatchWorkItem {
                    log("等待授权超时，请在系统设置 → 隐私与安全性 → 定位服务中检查权限")
                    exit(1)
                }
                authorizationTimeout = timeout
                DispatchQueue.main.asyncAfter(deadline: .now() + 120, execute: timeout)
            }
        }
        handleAuthorization()
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        handleAuthorization()
    }

    func stop() {
        _ = work.invalidate()
        log("服务停止")
        exit(0)
    }

    private func handleAuthorization() {
        let status = location.authorizationStatus
        let allowed = status == .authorizedAlways
        if allowed {
            authorizationTimeout?.cancel()
            guard !authorized else { return }
            authorized = true
            log("定位权限已授权")
            if authorizeOnly { exit(0) }
            if !sleeping { schedule(reason: "程序启动／权限已就绪") }
        } else if status == .denied || status == .restricted {
            authorizationTimeout?.cancel()
            authorized = false
            _ = work.invalidate()
            log("定位权限未获准，无法判断 WiFi 名称；请在系统设置中授权 SHU WiFi Keeper")
            if once || authorizeOnly { exit(1) }
        }
    }

    private func schedule(reason: String) {
        guard let settings = settings else { return }
        let token = work.invalidate()
        log("\(reason)：3 秒后检测目标 WiFi")
        queue.asyncAfter(deadline: .now() + 3) { [self] in
            guard work.current(token) else { return }
            let code = check(settings: settings, token: token)
            if once && work.current(token) { exit(code) }
        }
    }

    private func check(settings: Settings, token: Int) -> Int32 {
        for attempt in 1...settings.attempts {
            guard work.current(token) else { return 1 }
            do {
                guard let interface = CWWiFiClient.shared().interface() else {
                    throw ServiceError.message("未找到 WiFi 接口")
                }
                guard interface.powerOn() else {
                    log("WiFi 已关闭，本次跳过")
                    return 0
                }
                if interface.ssid() != settings.ssid {
                    let networks = try interface.scanForNetworks(withSSID: Data(settings.ssid.utf8))
                    guard work.current(token) else { return 1 }
                    let matching = networks.filter { $0.ssid == settings.ssid }
                    guard let target = matching.max(by: { $0.rssiValue < $1.rssiValue }) else {
                        if networks.contains(where: { $0.ssid == nil }) {
                            throw ServiceError.message("扫描结果隐藏了 SSID，请检查定位权限和应用签名")
                        }
                        // Retry briefly: the WiFi radio may still be recovering after wake.
                        throw ServiceError.message("附近未发现目标 WiFi")
                    }
                    log("发现目标 WiFi，尝试连接（第 \(attempt) 次）")
                    try interface.associate(to: target, password: settings.wifi_password)
                }
                guard work.current(token) else { return 1 }
                guard let name = interface.interfaceName else {
                    throw ServiceError.message("无法读取 WiFi 接口名称")
                }
                var ip: String?
                for _ in 0..<15 {
                    guard work.current(token) else { return 1 }
                    if interface.ssid() == settings.ssid { ip = ipv4(interface: name) }
                    if ip != nil { break }
                    guard work.pause(seconds: 1, token: token) else { return 1 }
                }
                guard let ip = ip, interface.ssid() == settings.ssid else {
                    throw ServiceError.message("目标 WiFi 尚未连接或未取得 IPv4")
                }
                let process = Process()
                process.executableURL = workerURL()
                process.arguments = ["--config", configURL.path, "--ip", ip, "--interface", name]
                process.standardOutput = FileHandle.standardOutput
                process.standardError = FileHandle.standardError
                guard try work.launch(process, token: token) else { return 1 }
                process.waitUntilExit()
                work.clear(process)
                guard work.current(token) else { return 1 }
                if process.terminationStatus == 0 { return 0 }
                if process.terminationStatus == 2 { return 2 }
                throw ServiceError.message("认证或网络检查暂未成功")
            } catch {
                guard work.current(token) else { return 1 }
                log("第 \(attempt)/\(settings.attempts) 次检测：\(error)")
            }
            if attempt < settings.attempts && !work.pause(seconds: settings.retry_seconds, token: token) {
                return 1
            }
        }
        log("本次检测结束，等待下一次启动或唤醒")
        return 1
    }
}

func ipv4(interface: String) -> String? {
    let process = Process()
    let pipe = Pipe()
    process.executableURL = URL(fileURLWithPath: "/usr/sbin/ipconfig")
    process.arguments = ["getifaddr", interface]
    process.standardOutput = pipe
    process.standardError = FileHandle.nullDevice
    do { try process.run() } catch { return nil }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard process.terminationStatus == 0,
          let text = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) else { return nil }
    var address = in_addr()
    guard inet_pton(AF_INET, text, &address) == 1,
          !text.hasPrefix("169.254."), !text.hasPrefix("127."), text != "0.0.0.0" else { return nil }
    return text
}

let arguments = Array(CommandLine.arguments.dropFirst())
if arguments == ["--help"] {
    print("SHU WiFi Keeper [--once | --authorize-only | --diagnose | --self-test] [--config PATH]")
    exit(0)
}
if arguments == ["--self-test"] {
    let state = WorkState()
    let first = state.invalidate()
    precondition(state.current(first))
    let second = state.invalidate()
    precondition(!state.current(first) && state.current(second))
    let cancelled = Process()
    cancelled.executableURL = URL(fileURLWithPath: "/usr/bin/true")
    let rejected = try state.launch(cancelled, token: first)
    precondition(!rejected)
    let child = Process()
    child.executableURL = URL(fileURLWithPath: "/bin/sleep")
    child.arguments = ["30"]
    let launched = try state.launch(child, token: second)
    precondition(launched)
    _ = state.invalidate()
    child.waitUntilExit()
    precondition(child.terminationReason == .uncaughtSignal)
    let valid = Settings(ssid: "Shu(forall)", wifi_password: nil, attempts: 4, retry_seconds: 5, timeout_seconds: 10)
    try valid.validate()
    do {
        try Settings(ssid: "", wifi_password: nil, attempts: 0, retry_seconds: 5, timeout_seconds: 10).validate()
        fatalError("invalid configuration accepted")
    } catch { }
    print("Swift self-test passed: event invalidation, child cancellation and configuration validation")
    exit(0)
}

do {
    var configURL = defaultConfigURL()
    var mode: String?
    var index = 0
    while index < arguments.count {
        let argument = arguments[index]
        if argument == "--config", index + 1 < arguments.count {
            index += 1
            configURL = URL(fileURLWithPath: arguments[index])
        } else if ["--once", "--authorize-only", "--diagnose"].contains(argument), mode == nil {
            mode = argument
        } else { throw ServiceError.message("未知或重复参数：\(argument)；使用 --help 查看用法") }
        index += 1
    }
    if mode == "--diagnose" {
        // Read-only: no prompt, association, credentials, or portal requests.
        let manager = CLLocationManager()
        print("定位服务启用：\(CLLocationManager.locationServicesEnabled())，授权状态：\(manager.authorizationStatus.rawValue)")
        if let interface = CWWiFiClient.shared().interface() {
            print("WiFi 接口：\(interface.interfaceName ?? "未知")，开关：\(interface.powerOn())，当前 SSID：\(interface.ssid() ?? "不可读取／未连接")")
        } else {
            print("无法读取 WiFi 接口；请在实际登录桌面的 Mac 上检查 WiFi 和应用权限")
        }
        exit(0)
    }
    let descriptor = try acquireInstanceLock()
    // Kept open for the lifetime of the process; launchd releases it on exit.
    _ = descriptor
    var settings: Settings?
    if mode != "--authorize-only" {
        let validator = Process()
        let pipe = Pipe()
        validator.executableURL = workerURL()
        validator.arguments = ["--settings", configURL.path]
        validator.standardOutput = pipe
        validator.standardError = FileHandle.standardError
        try validator.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        validator.waitUntilExit()
        guard validator.terminationStatus == 0 else { throw ServiceError.message("配置验证未通过") }
        settings = try JSONDecoder().decode(Settings.self, from: data)
        try settings?.validate()
    }
    let application = NSApplication.shared
    application.setActivationPolicy(.accessory)
    let service = Service(configURL: configURL, settings: settings,
                          once: mode == "--once", authorizeOnly: mode == "--authorize-only")
    let signals = [SIGTERM, SIGINT].map { number -> DispatchSourceSignal in
        signal(number, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: number, queue: .main)
        source.setEventHandler { service.stop() }
        source.resume()
        return source
    }
    service.start()
    withExtendedLifetime((service, signals)) { application.run() }
} catch {
    log("服务无法启动：\(error)")
    exit(1)
}
