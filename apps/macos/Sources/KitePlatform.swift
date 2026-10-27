import AppKit
import Foundation
import QuickLookUI
import UniformTypeIdentifiers

enum KitePlatform {
    private static let quickLookController = QuickLookController()

    static func open(_ path: String) {
        NSWorkspace.shared.open(URL(fileURLWithPath: path))
    }

    static func revealInFinder(_ path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }

    static func quickLook(_ path: String) {
        quickLookController.show(path)
    }

    static func copyPath(_ path: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(path, forType: .string)
    }

    static func share(_ path: String) {
        guard let view = NSApp.keyWindow?.contentView else { return }
        let picker = NSSharingServicePicker(items: [URL(fileURLWithPath: path)])
        picker.show(relativeTo: NSRect(x: 0, y: 0, width: 1, height: 1), of: view, preferredEdge: .minY)
    }

    static func moveToTrash(_ path: String) throws {
        try FileManager.default.trashItem(at: URL(fileURLWithPath: path), resultingItemURL: nil)
    }

    static func promptForName(title: String, message: String, defaultValue: String) -> String? {
        let alert = NSAlert()
        alert.alertStyle = .informational
        alert.messageText = title
        alert.informativeText = message
        alert.addButton(withTitle: "确定")
        alert.addButton(withTitle: "取消")

        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 320, height: 24))
        field.stringValue = defaultValue
        field.placeholderString = "名称"
        alert.accessoryView = field
        alert.window.initialFirstResponder = field

        guard alert.runModal() == .alertFirstButtonReturn else {
            return nil
        }
        return field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func createFolder(in directory: String, name: String) throws {
        try validateName(name)
        let url = URL(fileURLWithPath: directory).appendingPathComponent(name, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
    }

    static func rename(_ path: String, to name: String) throws {
        try validateName(name)
        let source = URL(fileURLWithPath: path)
        let destination = source.deletingLastPathComponent().appendingPathComponent(name)
        try FileManager.default.moveItem(at: source, to: destination)
    }

    static func canModify(_ path: String) -> Bool {
        URL(fileURLWithPath: path).standardizedFileURL.path != "/"
    }

    static func setKiteAsDefaultFolderOpener() async throws {
        try await NSWorkspace.shared.setDefaultApplication(
            at: Bundle.main.bundleURL,
            toOpen: .folder
        )
    }

    static func restoreFinderAsDefaultFolderOpener() async throws {
        let finder = URL(
            fileURLWithPath: "/System/Library/CoreServices/Finder.app",
            isDirectory: true
        )
        try await NSWorkspace.shared.setDefaultApplication(at: finder, toOpen: .folder)
    }

    private static func validateName(_ name: String) throws {
        guard !name.isEmpty, name != ".", name != "..", !name.contains("/") else {
            throw NSError(
                domain: "KitePlatform",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "请输入有效名称。名称不能包含斜杠。"]
            )
        }
    }
}

private final class QuickLookController: NSObject, QLPreviewPanelDataSource, QLPreviewPanelDelegate {
    private var currentURL: URL?

    func show(_ path: String) {
        currentURL = URL(fileURLWithPath: path)
        guard let panel = QLPreviewPanel.shared() else { return }
        panel.dataSource = self
        panel.delegate = self
        panel.makeKeyAndOrderFront(nil)
    }

    func numberOfPreviewItems(in panel: QLPreviewPanel!) -> Int {
        currentURL == nil ? 0 : 1
    }

    func previewPanel(_ panel: QLPreviewPanel!, previewItemAt index: Int) -> QLPreviewItem! {
        currentURL as NSURL?
    }
}
