import AppKit
import Foundation

// MARK: - Config
struct AppConfig: Codable {
    var apiBase: String
    var username: String
    var localPath: String
    var apiToken: String
    
    var isConfigured: Bool { !username.isEmpty && !localPath.isEmpty && !apiToken.isEmpty }
    static let empty = AppConfig(apiBase: "https://mdflare.com", username: "", localPath: "", apiToken: "")
}

class ConfigManager {
    static let shared = ConfigManager()
    private let configURL: URL = {
        let dir = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".mdflare", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("config.json")
    }()
    
    func load() -> AppConfig {
        guard let data = try? Data(contentsOf: configURL),
              let config = try? JSONDecoder().decode(AppConfig.self, from: data) else { return .empty }
        return config
    }
    
    func save(_ config: AppConfig) {
        guard let data = try? JSONEncoder().encode(config) else { return }
        try? data.write(to: configURL, options: .atomic)
    }
}

// MARK: - API Client
class APIClient {
    let baseURL: String
    let username: String
    let apiToken: String
    
    init(baseURL: String, username: String, apiToken: String = "") {
        self.baseURL = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        self.username = username
        self.apiToken = apiToken
    }
    
    struct FileItem: Codable {
        let name: String
        let path: String
        let type: String
        let size: Int?
        let modified: String?
        let children: [FileItem]?
    }
    
    struct FilesResponse: Codable {
        let user: String
        let files: [FileItem]
    }
    
    struct FileContent: Codable {
        let path: String
        let content: String
        let size: Int
        let modified: String
    }
    
    func listFiles() async throws -> [FileItem] {
        let url = URL(string: "\(baseURL)/api/\(username)/files")!
        let (data, _) = try await URLSession.shared.data(from: url)
        return try JSONDecoder().decode(FilesResponse.self, from: data).files
    }
    
    func getFile(_ path: String) async throws -> FileContent {
        let encoded = path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? path
        let url = URL(string: "\(baseURL)/api/\(username)/file/\(encoded)")!
        let (data, _) = try await URLSession.shared.data(from: url)
        return try JSONDecoder().decode(FileContent.self, from: data)
    }
    
    func putFile(_ path: String, content: String) async throws {
        let encoded = path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? path
        let url = URL(string: "\(baseURL)/api/\(username)/file/\(encoded)")!
        var req = URLRequest(url: url)
        req.httpMethod = "PUT"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if !apiToken.isEmpty { req.setValue("Bearer \(apiToken)", forHTTPHeaderField: "Authorization") }
        req.httpBody = try JSONSerialization.data(withJSONObject: ["content": content])
        let _ = try await URLSession.shared.data(for: req)
    }
    
    func deleteFile(_ path: String) async throws {
        let encoded = path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? path
        let url = URL(string: "\(baseURL)/api/\(username)/file/\(encoded)")!
        var req = URLRequest(url: url)
        req.httpMethod = "DELETE"
        if !apiToken.isEmpty { req.setValue("Bearer \(apiToken)", forHTTPHeaderField: "Authorization") }
        let _ = try await URLSession.shared.data(for: req)
    }
}

// MARK: - File Watcher (FSEvents)
class FileWatcher {
    private var stream: FSEventStreamRef?
    private let path: String
    private let onChange: ([String]) -> Void
    
    init(path: String, onChange: @escaping ([String]) -> Void) {
        self.path = path
        self.onChange = onChange
    }
    
    func start() {
        let pathsToWatch = [path as CFString] as CFArray
        var context = FSEventStreamContext()
        context.info = Unmanaged.passUnretained(self).toOpaque()
        
        stream = FSEventStreamCreate(
            nil,
            { (_, info, _, eventPaths, _, _) in
                guard let info = info else { return }
                let watcher = Unmanaged<FileWatcher>.fromOpaque(info).takeUnretainedValue()
                guard let paths = unsafeBitCast(eventPaths, to: NSArray.self) as? [String] else { return }
                let mdPaths = paths.filter { $0.hasSuffix(".md") }
                if !mdPaths.isEmpty { watcher.onChange(mdPaths) }
            },
            &context,
            pathsToWatch,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            1.0,
            UInt32(kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagNoDefer)
        )
        
        guard let stream = stream else { return }
        FSEventStreamScheduleWithRunLoop(stream, CFRunLoopGetMain(), CFRunLoopMode.defaultMode.rawValue)
        FSEventStreamStart(stream)
    }
    
    func stop() {
        guard let stream = stream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        self.stream = nil
    }
}

// MARK: - Sync Engine
class SyncEngine {
    var api: APIClient?
    var watcher: FileWatcher?
    var localPath = ""
    var isSyncing = false
    var localHashes: [String: String] = [:]
    var onStatusChange: ((String) -> Void)?
    var syncTimer: Timer?
    
    func start(config: AppConfig) {
        localPath = config.localPath
        api = APIClient(baseURL: config.apiBase, username: config.username, apiToken: config.apiToken)
        
        let fm = FileManager.default
        if !fm.fileExists(atPath: localPath) {
            try? fm.createDirectory(atPath: localPath, withIntermediateDirectories: true)
        }
        
        watcher = FileWatcher(path: localPath) { [weak self] paths in
            self?.handleLocalChanges(paths)
        }
        watcher?.start()
        
        syncTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            Task { await self?.fullSync() }
        }
        
        log("✅ 동기화 시작: \(localPath)")
        onStatusChange?("동기화 중")
        Task { await fullSync() }
    }
    
    func stop() {
        watcher?.stop()
        watcher = nil
        syncTimer?.invalidate()
        syncTimer = nil
        onStatusChange?("중지됨")
    }
    
    private func handleLocalChanges(_ paths: [String]) {
        guard !isSyncing else { return }
        Task {
            for fullPath in paths {
                let rel = fullPath.replacingOccurrences(of: localPath + "/", with: "")
                guard rel.hasSuffix(".md") else { continue }
                
                if FileManager.default.fileExists(atPath: fullPath) {
                    guard let content = try? String(contentsOfFile: fullPath, encoding: .utf8) else { continue }
                    let hash = simpleHash(content)
                    if localHashes[rel] == hash { continue }
                    localHashes[rel] = hash
                    do {
                        try await api?.putFile(rel, content: content)
                        log("⬆️ \(rel)")
                    } catch { log("❌ 업로드 실패: \(rel)") }
                } else {
                    do {
                        try await api?.deleteFile(rel)
                        localHashes.removeValue(forKey: rel)
                        log("🗑️ \(rel)")
                    } catch { log("❌ 삭제 실패: \(rel)") }
                }
            }
        }
    }
    
    func fullSync() async {
        guard !isSyncing, let api = api else { return }
        isSyncing = true
        onStatusChange?("동기화 중...")
        
        do {
            let remoteFiles = try await api.listFiles()
            let remotePaths = flatten(remoteFiles)
            let localFiles = getLocalMdFiles()
            var count = 0
            
            // remote → local
            for r in remotePaths {
                let localFile = (localPath as NSString).appendingPathComponent(r)
                if !FileManager.default.fileExists(atPath: localFile) {
                    let file = try await api.getFile(r)
                    let dir = (localFile as NSString).deletingLastPathComponent
                    try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
                    try file.content.write(toFile: localFile, atomically: true, encoding: .utf8)
                    localHashes[r] = simpleHash(file.content)
                    count += 1
                    log("⬇️ \(r)")
                }
            }
            
            // local → remote
            for l in localFiles {
                if !remotePaths.contains(l) {
                    let fullPath = (localPath as NSString).appendingPathComponent(l)
                    guard let content = try? String(contentsOfFile: fullPath, encoding: .utf8) else { continue }
                    try await api.putFile(l, content: content)
                    localHashes[l] = simpleHash(content)
                    count += 1
                    log("⬆️ \(l)")
                }
            }
            
            if count > 0 { log("🔄 \(count)개 동기화 완료") }
            onStatusChange?("대기 중 · \(remotePaths.count + localFiles.count)개 파일")
        } catch {
            log("❌ \(error.localizedDescription)")
            onStatusChange?("오류")
        }
        isSyncing = false
    }
    
    private func flatten(_ items: [APIClient.FileItem], prefix: String = "") -> [String] {
        var result: [String] = []
        for item in items {
            if item.type == "folder", let children = item.children {
                result += flatten(children)
            } else if item.type == "file" {
                result.append(item.path)
            }
        }
        return result
    }
    
    private func getLocalMdFiles() -> [String] {
        guard let e = FileManager.default.enumerator(atPath: localPath) else { return [] }
        var files: [String] = []
        while let f = e.nextObject() as? String {
            if f.hasSuffix(".md") && !f.hasPrefix(".") { files.append(f) }
        }
        return files
    }
    
    private func simpleHash(_ s: String) -> String {
        var h: Int = 0
        for c in s.unicodeScalars { h = ((h << 5) &- h) &+ Int(c.value) }
        return String(h, radix: 36)
    }
    
    func log(_ msg: String) {
        let ts = ISO8601DateFormatter().string(from: Date())
        print("[\(ts)] \(msg)")
    }
}

// MARK: - App Delegate (Menu Bar)
class AppDelegate: NSObject, NSApplicationDelegate {
    var statusItem: NSStatusItem!
    var syncEngine = SyncEngine()
    
    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        
        let config = ConfigManager.shared.load()
        
        syncEngine.onStatusChange = { [weak self] status in
            DispatchQueue.main.async {
                self?.statusItem.button?.title = " \(status)"
            }
        }
        
        updateMenu(configured: config.isConfigured)
        
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "flame.fill", accessibilityDescription: "MDFlare")
            button.imagePosition = .imageLeading
        }
        
        if config.isConfigured {
            syncEngine.start(config: config)
        } else {
            statusItem.button?.title = " 설정 필요"
        }
    }
    
    func updateMenu(configured: Bool) {
        let menu = NSMenu()
        
        if configured {
            let config = ConfigManager.shared.load()
            menu.addItem(NSMenuItem(title: "👤 \(config.username)", action: nil, keyEquivalent: ""))
            menu.addItem(NSMenuItem(title: "📁 \(shortenPath(config.localPath))", action: nil, keyEquivalent: ""))
            menu.addItem(NSMenuItem.separator())
            menu.addItem(NSMenuItem(title: "🔄 지금 동기화", action: #selector(syncNow), keyEquivalent: "s"))
            menu.addItem(NSMenuItem(title: "📂 폴더 열기", action: #selector(openFolder), keyEquivalent: "o"))
            menu.addItem(NSMenuItem(title: "🌐 웹에서 열기", action: #selector(openWeb), keyEquivalent: "w"))
            menu.addItem(NSMenuItem.separator())
            menu.addItem(NSMenuItem(title: "⚙️ 설정 초기화", action: #selector(resetConfig), keyEquivalent: ""))
        } else {
            menu.addItem(NSMenuItem(title: "⚙️ 초기 설정", action: #selector(showSetup), keyEquivalent: ""))
        }
        
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "종료", action: #selector(quit), keyEquivalent: "q"))
        
        // 모든 메뉴 아이템의 target 설정
        for item in menu.items {
            if item.action != nil { item.target = self }
        }
        
        statusItem.menu = menu
    }
    
    @objc func syncNow() {
        Task { await syncEngine.fullSync() }
    }
    
    @objc func openFolder() {
        let config = ConfigManager.shared.load()
        NSWorkspace.shared.open(URL(fileURLWithPath: config.localPath))
    }
    
    @objc func openWeb() {
        let config = ConfigManager.shared.load()
        if let url = URL(string: "\(config.apiBase)/\(config.username)") {
            NSWorkspace.shared.open(url)
        }
    }
    
    @objc func showSetup() {
        showSetupDialog(savedUsername: "", savedToken: "", savedFolder: "")
    }
    
    private func showSetupDialog(savedUsername: String, savedToken: String, savedFolder: String) {
        let alert = NSAlert()
        alert.messageText = "MDFlare Agent 설정"
        alert.informativeText = "1. 아래 '웹에서 토큰 발급' 클릭\n2. Google 로그인 → 🔑 API 토큰 버튼\n3. 토큰 복사 후 아래에 붙여넣기"
        
        let stack = NSStackView(frame: NSRect(x: 0, y: 0, width: 300, height: 160))
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        
        let usernameLabel = NSTextField(labelWithString: "사용자 이름:")
        let usernameField = NSTextField(frame: NSRect(x: 0, y: 0, width: 300, height: 24))
        usernameField.placeholderString = "your-username"
        usernameField.stringValue = savedUsername
        
        let tokenLabel = NSTextField(labelWithString: "API 토큰:")
        let tokenField = NSTextField(frame: NSRect(x: 0, y: 0, width: 300, height: 24))
        tokenField.placeholderString = "웹에서 발급받은 토큰 붙여넣기"
        tokenField.stringValue = savedToken
        
        let folderLabel = NSTextField(labelWithString: "동기화 폴더:")
        let folderField = NSTextField(frame: NSRect(x: 0, y: 0, width: 300, height: 24))
        folderField.placeholderString = "아래 '폴더 선택' 클릭"
        folderField.stringValue = savedFolder
        folderField.isEditable = false
        
        stack.addArrangedSubview(usernameLabel)
        stack.addArrangedSubview(usernameField)
        stack.addArrangedSubview(tokenLabel)
        stack.addArrangedSubview(tokenField)
        stack.addArrangedSubview(folderLabel)
        stack.addArrangedSubview(folderField)
        
        for field in [usernameField, tokenField, folderField] {
            field.translatesAutoresizingMaskIntoConstraints = false
            field.widthAnchor.constraint(equalToConstant: 300).isActive = true
        }
        
        alert.accessoryView = stack
        alert.addButton(withTitle: "시작")              // 1st
        alert.addButton(withTitle: "폴더 선택")          // 2nd
        alert.addButton(withTitle: "웹에서 토큰 발급")    // 3rd
        alert.addButton(withTitle: "취소")               // 4th
        
        let response = alert.runModal()
        
        if response == .alertFirstButtonReturn {
            // 시작
            let username = usernameField.stringValue.lowercased().trimmingCharacters(in: .whitespaces)
            let token = tokenField.stringValue.trimmingCharacters(in: .whitespaces)
            let folderPath = folderField.stringValue
            
            if !username.isEmpty && !token.isEmpty && !folderPath.isEmpty {
                saveConfig(username: username, token: token, folderPath: folderPath)
            } else {
                let err = NSAlert()
                err.messageText = "모든 필드를 입력해주세요"
                err.informativeText = "사용자 이름, API 토큰, 폴더 경로가 모두 필요합니다."
                err.runModal()
                // 다시 다이얼로그 표시 (입력값 유지)
                showSetupDialog(savedUsername: usernameField.stringValue,
                               savedToken: tokenField.stringValue,
                               savedFolder: folderField.stringValue)
            }
        } else if response == .alertSecondButtonReturn {
            // 폴더 선택 → 선택 후 다시 다이얼로그 표시
            let panel = NSOpenPanel()
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.canCreateDirectories = true
            panel.message = "동기화할 마크다운 폴더를 선택하세요"
            
            var folder = savedFolder
            if panel.runModal() == .OK, let url = panel.url {
                folder = url.path
            }
            // 입력값 유지하면서 다시 표시
            showSetupDialog(savedUsername: usernameField.stringValue,
                           savedToken: tokenField.stringValue,
                           savedFolder: folder)
        } else if response == .alertThirdButtonReturn {
            // 웹 브라우저에서 토큰 발급 페이지 열기
            if let url = URL(string: "https://mdflare.com") {
                NSWorkspace.shared.open(url)
            }
            // 입력값 유지하면서 다시 표시
            showSetupDialog(savedUsername: usernameField.stringValue,
                           savedToken: tokenField.stringValue,
                           savedFolder: folderField.stringValue)
        }
        // 취소는 그냥 닫힘
    }
    
    private func saveConfig(username: String, token: String, folderPath: String) {
        let config = AppConfig(apiBase: "https://mdflare.com", username: username, localPath: folderPath, apiToken: token)
        ConfigManager.shared.save(config)
        updateMenu(configured: true)
        syncEngine.start(config: config)
    }
    
    @objc func resetConfig() {
        syncEngine.stop()
        ConfigManager.shared.save(.empty)
        updateMenu(configured: false)
        statusItem.button?.title = " 설정 필요"
    }
    
    @objc func quit() {
        syncEngine.stop()
        NSApplication.shared.terminate(nil)
    }
    
    private func shortenPath(_ path: String) -> String {
        path.replacingOccurrences(of: FileManager.default.homeDirectoryForCurrentUser.path, with: "~")
    }
}

// MARK: - Launch
let app = NSApplication.shared
app.setActivationPolicy(.accessory) // 독에 안 뜸
let delegate = AppDelegate()
app.delegate = delegate
app.run()
