import AppKit
import Combine
import SwiftUI

extension Notification.Name {
    static let kiteOpenFolders = Notification.Name("KiteOpenFolders")
}

@MainActor
final class KiteOpenRequestRouter {
    static let shared = KiteOpenRequestRouter()

    private var pendingPaths: [String] = []

    func enqueue(urls: [URL]) {
        let paths = urls.compactMap { url -> String? in
            let values = try? url.resourceValues(forKeys: [.isDirectoryKey])
            return values?.isDirectory == true ? url.path : nil
        }
        guard !paths.isEmpty else { return }
        pendingPaths.append(contentsOf: paths)
        NotificationCenter.default.post(name: .kiteOpenFolders, object: nil)
    }

    func drain() -> [String] {
        defer { pendingPaths.removeAll() }
        return pendingPaths
    }
}

@MainActor
final class KiteAppDelegate: NSObject, NSApplicationDelegate {
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
                    model.openExternalFolders(KiteOpenRequestRouter.shared.drain())
                }
                .onReceive(NotificationCenter.default.publisher(for: .kiteOpenFolders)) { _ in
                    model.openExternalFolders(KiteOpenRequestRouter.shared.drain())
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
        .windowStyle(.titleBar)
        .commands {
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
            CommandMenu("文件夹") {
                Button("设为默认文件夹打开方式…") {
                    model.setAsDefaultFolderOpener()
                }

                Button("恢复 Finder 为默认…") {
                    model.restoreFinderAsDefaultFolderOpener()
                }
            }
        }
    }
}
