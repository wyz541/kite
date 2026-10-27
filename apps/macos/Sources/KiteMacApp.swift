import AppKit
import Combine
import SwiftUI

@MainActor
@main
struct KiteMacApp: App {
    @StateObject private var model = KiteModel()

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
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
        }
    }
}