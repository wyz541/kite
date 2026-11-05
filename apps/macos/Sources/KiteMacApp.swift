import AppKit
import Combine
import CoreServices
import SwiftUI

extension Notification.Name {
    static let kiteOpenFolders = Notification.Name("KiteOpenFolders")
    static let kiteRevealItems = Notification.Name("KiteRevealItems")
}

@MainActor
final class KiteOpenRequestRouter {
    static let shared = KiteOpenRequestRouter()

    private var pendingFolderPaths: [String] = []
    private var pendingRevealPaths: [String] = []

    func enqueue(urls: [URL]) {
        var folderPaths: [String] = []
        var revealPaths: [String] = []

        for url in urls where url.isFileURL {
            let values = try? url.resourceValues(forKeys: [.isDirectoryKey])
            if values?.isDirectory == true {
                folderPaths.append(url.path)
            } else {
                revealPaths.append(url.path)
            }
        }

        if !folderPaths.isEmpty {
            pendingFolderPaths.append(contentsOf: folderPaths)
            NotificationCenter.default.post(name: .kiteOpenFolders, object: nil)
        }
        if !revealPaths.isEmpty {
            pendingRevealPaths.append(contentsOf: revealPaths)
            NotificationCenter.default.post(name: .kiteRevealItems, object: nil)
        }
    }

    func enqueueReveal(urls: [URL]) {
        let paths = urls.filter(\.isFileURL).map(\.path)
        guard !paths.isEmpty else { return }
        pendingRevealPaths.append(contentsOf: paths)
        NotificationCenter.default.post(name: .kiteRevealItems, object: nil)
    }

    func drainFolders() -> [String] {
        defer { pendingFolderPaths.removeAll() }
        return pendingFolderPaths
    }

    func drainRevealItems() -> [String] {
        defer { pendingRevealPaths.removeAll() }
        return pendingRevealPaths
    }
}

@MainActor
final class KiteAppleEventRouter: NSObject {
    static let shared = KiteAppleEventRouter()

    @objc
    func handleShow(
        _ event: NSAppleEventDescriptor,
        reply: NSAppleEventDescriptor
    ) {
        let urls = Self.fileURLs(in: event)
        guard !urls.isEmpty else { return }
        NSApp.activate(ignoringOtherApps: true)
        KiteOpenRequestRouter.shared.enqueueReveal(urls: urls)
    }

    private static func fileURLs(in event: NSAppleEventDescriptor) -> [URL] {
        guard let directObject = event.paramDescriptor(forKeyword: keyDirectObject) else {
            return []
        }
        if directObject.numberOfItems == 0 {
            return [directObject.fileURLValue].compactMap { $0 }
        }
        return (1...directObject.numberOfItems).compactMap {
            directObject.atIndex($0)?.fileURLValue
        }
    }

    static let finderEventClass = fourCharacterCode("FNDR")
    static let finderShowEvent = fourCharacterCode("show")

    private static func fourCharacterCode(_ value: String) -> AEEventClass {
        AEEventClass(value.utf8.reduce(0) { ($0 << 8) | UInt32($1) })
    }
}

@MainActor
final class KiteAppDelegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        let manager = NSAppleEventManager.shared()
        let router = KiteAppleEventRouter.shared
        manager.setEventHandler(
            router,
            andSelector: #selector(KiteAppleEventRouter.handleShow(_:reply:)),
            forEventClass: AEEventClass(kAEMiscStandards),
            andEventID: AEEventID(kAEMakeObjectsVisible)
        )
        manager.setEventHandler(
            router,
            andSelector: #selector(KiteAppleEventRouter.handleShow(_:reply:)),
            forEventClass: KiteAppleEventRouter.finderEventClass,
            andEventID: AEEventID(KiteAppleEventRouter.finderShowEvent)
        )
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        NSApp.activate(ignoringOtherApps: true)
        KiteOpenRequestRouter.shared.enqueue(urls: urls)
    }
}

@MainActor
@main
struct KiteMacApp: App {
    @StateObject private var model = KiteModel()
    @NSApplicationDelegateAdaptor(KiteAppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
                .onAppear {
                    model.openExternalFolders(KiteOpenRequestRouter.shared.drainFolders())
                    model.revealExternalItems(KiteOpenRequestRouter.shared.drainRevealItems())
                }
                .onReceive(NotificationCenter.default.publisher(for: .kiteOpenFolders)) { _ in
                    model.openExternalFolders(KiteOpenRequestRouter.shared.drainFolders())
                }
                .onReceive(NotificationCenter.default.publisher(for: .kiteRevealItems)) { _ in
                    model.revealExternalItems(KiteOpenRequestRouter.shared.drainRevealItems())
                }
                .onReceive(NotificationCenter.default.publisher(for: NSWindow.willCloseNotification)) { _ in
                    model.saveSession()
                }
                .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
                    model.saveSession()
                }
                .onDisappear {
                    model.saveSession()
                }
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentMinSize)
        .defaultSize(width: 1240, height: 800)
        .commands {
            CommandGroup(replacing: .saveItem) {
                Button("保存") {
                    TextEditorWindowController.saveKeyWindow()
                }
                .keyboardShortcut("s", modifiers: [.command])
            }
            CommandMenu("编辑器") {
                Button("查找…") {
                    TextEditorWindowController.showFindInKeyWindow(replace: false)
                }
                .keyboardShortcut("f", modifiers: [.command])

                Button("查找并替换…") {
                    TextEditorWindowController.showFindInKeyWindow(replace: true)
                }
                .keyboardShortcut("f", modifiers: [.command, .option])
            }
            CommandGroup(after: .appInfo) {
                Button("检查更新…") {
                    model.checkForUpdates()
                }
                .disabled(model.updateInProgress)
            }
            CommandMenu("导航") {
                Button("聚焦路径栏") {
                    model.focusAddress()
                }
                .keyboardShortcut("l", modifiers: [.command])

                Button("全局搜索…") {
                    KiteGlobalSearchWindowController.shared.show(model: model)
                }
                .keyboardShortcut("f", modifiers: [.command, .shift])

                Divider()

                Button("刷新") {
                    model.refresh()
                }
                .keyboardShortcut("r", modifiers: [.command])
            }
            CommandMenu("页签") {
                Button("新建页签") {
                    model.newTab()
                }
                .keyboardShortcut("t", modifiers: [.command])

                Button("关闭页签") {
                    if !model.closeActiveTab() {
                        NSApp.keyWindow?.performClose(nil)
                    }
                }
                .keyboardShortcut("w", modifiers: [.command])

                Divider()

                Button("下一个页签") {
                    model.nextTab()
                }
                .keyboardShortcut(.tab, modifiers: [.control])

                Button("上一个页签") {
                    model.previousTab()
                }
                .keyboardShortcut(.tab, modifiers: [.control, .shift])
            }
            CommandMenu("选择") {
                Button("全选") {
                    model.selectAll()
                }
                .keyboardShortcut("a", modifiers: [.command, .shift])
            }
            CommandMenu("文件夹") {
                Button("新建文件夹…") {
                    model.createFolder(in: model.currentPath)
                }
                .keyboardShortcut("n", modifiers: [.command, .shift])

                Button("新建文件…") {
                    model.createFile(in: model.currentPath)
                }
                .keyboardShortcut("n", modifiers: [.command])

                Divider()

                Button(
                    KitePlatform.canSetDefaultFolderHandler
                        ? "设为默认文件夹打开方式…"
                        : "设为默认文件查看器…"
                ) {
                    model.setAsDefaultFileViewer()
                }

                Button("恢复 Finder 为默认…") {
                    model.restoreFinderAsDefaultFileViewer()
                }
            }
        }
    }
}
