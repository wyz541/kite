import SwiftUI

@MainActor
@main
struct KiteMacApp: App {
    @StateObject private var model = KiteModel()

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
        }
        .windowStyle(.titleBar)
        .commands {
            CommandMenu("前往") {
                Button("上层文件夹") {
                    model.navigateUp()
                }
                .keyboardShortcut(.upArrow, modifiers: [.command])

                Button("前往文件夹…") {
                    model.focusAddress()
                }
                .keyboardShortcut("g", modifiers: [.command, .shift])
            }
        }
    }
}
