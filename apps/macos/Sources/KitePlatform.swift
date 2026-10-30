import AppKit
import Foundation
import QuickLookUI
import UniformTypeIdentifiers

enum KitePlatform {
    private static let quickLookController = QuickLookController()

    static func open(_ path: String) {
        NSWorkspace.shared.open(URL(fileURLWithPath: path))
    }

    static func openTextEditor(_ path: String) {
        do {
            try TextEditorWindowController.open(path: path)
        } catch {
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = "无法打开文本编辑器"
            alert.informativeText = error.localizedDescription
            alert.runModal()
        }
    }

    static func isEditableTextFile(_ path: String) -> Bool {
        let extensionName = URL(fileURLWithPath: path).pathExtension.lowercased()
        return [
            "", "txt", "md", "markdown", "log", "json", "jsonc", "toml", "yaml", "yml",
            "xml", "ini", "conf", "cfg", "properties", "env", "csv", "tsv", "svg",
            "rs", "py", "pyw", "js", "mjs", "cjs", "jsx", "ts", "tsx", "html", "htm",
            "css", "scss", "less", "c", "h", "cc", "cpp", "cxx", "hpp", "hxx", "cs",
            "java", "go", "kt", "kts", "sh", "bash", "zsh", "bat", "cmd", "ps1", "psm1",
            "psd1", "sql", "swift", "lua", "rb", "php", "proto", "cmake", "diff", "patch"
        ].contains(extensionName)
    }

    static func revealInFinder(_ path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }

    static func quickLook(_ path: String) {
        quickLookController.show(path)
    }

    static func copyPath(_ path: String) {
        copyText(path)
    }

    static func copyText(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
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

    static func createFile(in directory: String, name: String) throws {
        try validateName(name)
        let url = URL(fileURLWithPath: directory).appendingPathComponent(name, isDirectory: false)
        try Data().write(to: url, options: .withoutOverwriting)
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

    static func installUpdate(archivePath: String, version: String) throws {
        let fileManager = FileManager.default
        let appURL = Bundle.main.bundleURL.resolvingSymlinksInPath()
        guard appURL.pathExtension == "app" else {
            throw updateError("Kite 当前不是应用包，无法自动更新。")
        }
        let parentURL = appURL.deletingLastPathComponent()
        guard fileManager.isWritableFile(atPath: parentURL.path) else {
            throw updateError("应用目录不可写。请将 Kite 移到可写目录后重试。")
        }

        let supportURL = try fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        .appendingPathComponent("Kite", isDirectory: true)
        .appendingPathComponent("updates", isDirectory: true)
        try fileManager.createDirectory(at: supportURL, withIntermediateDirectories: true)

        let processID = ProcessInfo.processInfo.processIdentifier
        let scriptURL = supportURL.appendingPathComponent("apply-\(version)-\(processID).sh")
        let logURL = supportURL.appendingPathComponent("update.log")
        let script = """
        #!/bin/bash
        set -u

        PID="$1"
        ARCHIVE="$2"
        APP="$3"
        LOG="$4"
        SCRIPT="$5"

        exec >>"$LOG" 2>&1
        echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] Installing Kite update"

        WORK="${APP}.update-${PID}"
        BACKUP="${APP}.old-${PID}"
        NEW_APP="$WORK/Kite.app"

        cleanup() {
            rm -rf "$WORK"
        }

        restore_and_open() {
            if [ -d "$BACKUP" ]; then
                rm -rf "$APP"
                mv "$BACKUP" "$APP"
                open "$APP"
            fi
        }

        while kill -0 "$PID" 2>/dev/null; do
            sleep 0.2
        done

        rm -rf "$WORK" "$BACKUP"
        mkdir -p "$WORK" || exit 1
        if ! /usr/bin/ditto -x -k "$ARCHIVE" "$WORK"; then
            echo "Cannot extract update archive"
            cleanup
            open "$APP"
            exit 1
        fi
        if [ ! -d "$NEW_APP" ]; then
            echo "Updated Kite.app is missing"
            cleanup
            open "$APP"
            exit 1
        fi
        if ! mv "$APP" "$BACKUP"; then
            echo "Cannot move current Kite.app"
            cleanup
            open "$APP"
            exit 1
        fi
        if ! mv "$NEW_APP" "$APP"; then
            echo "Cannot install updated Kite.app"
            restore_and_open
            cleanup
            exit 1
        fi
        if ! open "$APP"; then
            echo "Updated Kite.app cannot start"
            restore_and_open
            cleanup
            exit 1
        fi

        sleep 2
        rm -rf "$WORK" "$BACKUP" "$ARCHIVE"
        rm -f "$SCRIPT"
        """
        try script.write(to: scriptURL, atomically: true, encoding: .utf8)

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [
            scriptURL.path,
            String(processID),
            archivePath,
            appURL.path,
            logURL.path,
            scriptURL.path
        ]
        try process.run()
    }

    private static func updateError(_ message: String) -> NSError {
        NSError(
            domain: "KitePlatform",
            code: 2,
            userInfo: [NSLocalizedDescriptionKey: message]
        )
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
