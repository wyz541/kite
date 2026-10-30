import AppKit
import Foundation

struct BrowserEntry: Identifiable, Hashable {
    let path: String
    let name: String
    let isDirectory: Bool
    let kindLabel: String
    let size: UInt64
    let modifiedUnixMs: Int64?
    let gitStatus: String?
    let svnStatus: String?

    var id: String { path }

    init(dto: DirectoryEntryDto) {
        path = dto.path
        name = dto.name
        isDirectory = dto.isDirectory
        kindLabel = dto.kindLabel
        size = dto.size
        modifiedUnixMs = dto.modifiedUnixMs
        gitStatus = dto.gitStatus
        svnStatus = dto.svnStatus
    }
}

struct BrowserColumn: Identifiable {
    let path: String
    let entries: [BrowserEntry]
    var selectedPath: String?

    var id: String { path }
    var title: String {
        let name = URL(fileURLWithPath: path).lastPathComponent
        return name.isEmpty ? path : name
    }
}

struct BrowserTab: Identifiable {
    let id: UUID
    var columns: [BrowserColumn]
    var address: String

    var currentPath: String { columns.last?.path ?? address }
    var title: String {
        let name = URL(fileURLWithPath: currentPath).lastPathComponent
        return name.isEmpty ? currentPath : name
    }
}

@MainActor
final class KiteModel: ObservableObject {
    private static let sessionSchemaVersion: UInt32 = 1

    private let core: KiteCore

    @Published private(set) var tabs: [BrowserTab] = []
    @Published private(set) var favorites: [FavoriteEntryDto] = []
    @Published private(set) var activeTabID: UUID?
    @Published var errorMessage: String?
    @Published var focusedAddressRequest = 0
    @Published private(set) var updateInProgress = false
    @Published private(set) var updateStatusMessage: String?

    init(core: KiteCore = KiteCore()) {
        self.core = core
        self.favorites = core.favorites()
        restoreOrCreateTabs()
    }

    var activeTab: BrowserTab? {
        guard let activeTabID else { return nil }
        return tabs.first { $0.id == activeTabID }
    }

    var activeTabIndex: Int? {
        guard let activeTabID else { return nil }
        return tabs.firstIndex { $0.id == activeTabID }
    }

    var columns: [BrowserColumn] { activeTab?.columns ?? [] }

    var address: String {
        get { activeTab?.address ?? "" }
        set {
            guard let index = activeTabIndex else { return }
            tabs[index].address = newValue
        }
    }

    var selectedColumnEntry: BrowserEntry? {
        guard let column = columns.last,
              let selectedPath = column.selectedPath else {
            return nil
        }
        return column.entries.first { $0.path == selectedPath }
    }

    var selectedColumnFile: BrowserEntry? {
        guard let entry = selectedColumnEntry, !entry.isDirectory else {
            return nil
        }
        return entry
    }

    var currentPath: String {
        activeTab?.currentPath ?? core.defaultDirectory()
    }

    var canCloseTab: Bool { tabs.count > 1 }

    func newTab(path: String? = nil) {
        do {
            let tab = try makeTab(path: path ?? currentPath)
            tabs.append(tab)
            activeTabID = tab.id
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    @discardableResult
    func closeActiveTab() -> Bool {
        guard canCloseTab, let index = activeTabIndex else { return false }
        let closingID = tabs[index].id
        tabs.remove(at: index)
        if activeTabID == closingID {
            let nextIndex = min(index, tabs.count - 1)
            activeTabID = tabs[nextIndex].id
        }
        return true
    }

    func closeTab(id: UUID) {
        guard canCloseTab, let index = tabs.firstIndex(where: { $0.id == id }) else { return }
        let wasActive = activeTabID == id
        tabs.remove(at: index)
        if wasActive {
            activeTabID = tabs[min(index, tabs.count - 1)].id
        }
    }

    func activateTab(id: UUID) {
        guard tabs.contains(where: { $0.id == id }) else { return }
        activeTabID = id
    }

    func nextTab() {
        guard tabs.count > 1, let index = activeTabIndex else { return }
        activeTabID = tabs[(index + 1) % tabs.count].id
    }

    func previousTab() {
        guard tabs.count > 1, let index = activeTabIndex else { return }
        activeTabID = tabs[(index + tabs.count - 1) % tabs.count].id
    }

    func loadRoot(_ path: String) {
        guard let index = activeTabIndex else {
            newTab(path: path)
            return
        }
        do {
            let tab = try makeTab(path: path, id: tabs[index].id)
            tabs[index] = tab
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func select(column index: Int, entry: BrowserEntry) {
        guard let tabIndex = activeTabIndex, tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].selectedPath = entry.path
        if tabs[tabIndex].columns.count > index + 1 {
            tabs[tabIndex].columns.removeSubrange((index + 1)..<tabs[tabIndex].columns.count)
        }
        guard entry.isDirectory else { return }
        do {
            let listing = try core.listDirectory(path: entry.path)
            tabs[tabIndex].columns.append(column(from: listing))
            tabs[tabIndex].address = listing.path
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func navigateUp() {
        let parent = URL(fileURLWithPath: currentPath).deletingLastPathComponent().path
        if parent != currentPath {
            loadRoot(parent)
        }
    }

    func focusAddress() {
        focusedAddressRequest += 1
    }

    func refresh() {
        loadRoot(currentPath)
    }

    func copyFile(_ path: String) {
        KitePlatform.writeFilePasteboard([path], cut: false)
        errorMessage = nil
    }

    func cutFile(_ path: String) {
        KitePlatform.writeFilePasteboard([path], cut: true)
        errorMessage = nil
    }

    func pasteFiles(into directory: String) {
        let paths = KitePlatform.filePasteboardPaths()
        guard !paths.isEmpty else {
            errorMessage = "剪贴板中没有文件。"
            return
        }
        let moveFiles = KitePlatform.filePasteboardIsCut()
        let core = self.core

        Task {
            do {
                for path in paths {
                    if moveFiles {
                        _ = try await Task.detached(priority: .userInitiated) {
                            try core.movePathTo(
                                source: path,
                                destinationDirectory: directory
                            )
                        }.value
                    } else {
                        _ = try await Task.detached(priority: .userInitiated) {
                            try core.copyPathTo(
                                source: path,
                                destinationDirectory: directory
                            )
                        }.value
                    }
                }
                if moveFiles {
                    KitePlatform.clearFilePasteboard()
                }
                errorMessage = nil
                refreshIfShowing(directory)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func copyTo(path: String) {
        guard let destination = KitePlatform.chooseDirectory(prompt: "拷贝到这里") else {
            return
        }
        transferPaths([path], to: destination, moveFiles: false)
    }

    private func transferPaths(_ paths: [String], to directory: String, moveFiles: Bool) {
        let core = self.core
        Task {
            do {
                for path in paths {
                    if moveFiles {
                        _ = try await Task.detached(priority: .userInitiated) {
                            try core.movePathTo(
                                source: path,
                                destinationDirectory: directory
                            )
                        }.value
                    } else {
                        _ = try await Task.detached(priority: .userInitiated) {
                            try core.copyPathTo(
                                source: path,
                                destinationDirectory: directory
                            )
                        }.value
                    }
                }
                errorMessage = nil
                refreshIfShowing(directory)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func refreshIfShowing(_ directory: String) {
        let current = URL(fileURLWithPath: currentPath).standardizedFileURL
        let destination = URL(fileURLWithPath: directory).standardizedFileURL
        if current.path == destination.path {
            refresh()
        }
    }

    func moveToTrash(_ entry: BrowserEntry) {
        moveToTrash(path: entry.path)
    }

    func moveToTrash(path: String) {
        do {
            try KitePlatform.moveToTrash(path)
            loadRoot(pathAfterRecycle(currentPath, removed: path))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createFolder(in directory: String) {
        guard let name = KitePlatform.promptForName(
            title: "新建文件夹",
            message: "在“\(columnTitle(for: directory))”中创建文件夹。",
            defaultValue: ""
        ) else {
            return
        }
        do {
            try KitePlatform.createFolder(in: directory, name: name)
            refresh()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createFile(in directory: String) {
        guard let name = KitePlatform.promptForName(
            title: "新建文件",
            message: "在“\(columnTitle(for: directory))”中创建空文件。",
            defaultValue: ""
        ) else {
            return
        }
        do {
            try KitePlatform.createFile(in: directory, name: name)
            refresh()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func addFavorite(path: String, name: String, isDirectory: Bool) {
        do {
            _ = try core.addFavorite(path: path, name: name, isDirectory: isDirectory)
            favorites = core.favorites()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func removeFavorite(_ favorite: FavoriteEntryDto) {
        do {
            _ = try core.removeFavorite(path: favorite.path)
            favorites = core.favorites()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func openFavorite(_ favorite: FavoriteEntryDto) {
        if favorite.isDirectory {
            loadRoot(favorite.path)
        } else {
            KitePlatform.open(favorite.path)
        }
    }

    func rename(_ path: String) {
        let source = URL(fileURLWithPath: path).standardizedFileURL
        guard KitePlatform.canModify(path) else {
            errorMessage = "无法重命名此位置。"
            return
        }
        guard let name = KitePlatform.promptForName(
            title: "重命名",
            message: "输入新名称。",
            defaultValue: source.lastPathComponent
        ) else {
            return
        }
        do {
            try KitePlatform.rename(path, to: name)
            let renamed = source.deletingLastPathComponent().appendingPathComponent(name)
            loadRoot(pathAfterRename(currentPath, original: source.path, renamed: renamed.path))
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func setAsDefaultFolderOpener() {
        guard confirmFolderOpenerChange(
            title: "设为默认文件夹打开方式？",
            message: "Kite 将成为文件夹的默认应用。",
            detail: "macOS 仍将 Finder 作为系统外壳。",
            action: "继续"
        ) else {
            return
        }
        Task {
            do {
                try await KitePlatform.setKiteAsDefaultFolderOpener()
                errorMessage = nil
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func restoreFinderAsDefaultFolderOpener() {
        guard confirmFolderOpenerChange(
            title: "恢复 Finder？",
            message: "Finder 将重新成为文件夹的默认应用。",
            detail: "Kite 的其他文件和目录保持不变。",
            action: "恢复"
        ) else {
            return
        }
        Task {
            do {
                try await KitePlatform.restoreFinderAsDefaultFolderOpener()
                errorMessage = nil
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func checkForUpdates() {
        guard !updateInProgress else { return }
        updateInProgress = true
        updateStatusMessage = "正在检查 Kite 更新…"
        errorMessage = nil

        Task {
            defer { updateInProgress = false }
            do {
                let update = try await Self.fetchUpdateInfo()
                guard update.updateAvailable else {
                    updateStatusMessage = nil
                    showUpdateAlert(
                        title: "Kite 已是最新版本",
                        message: "当前版本：\(update.currentVersion)\n最新版本：\(update.latestVersion)",
                        style: .informational
                    )
                    return
                }
                guard confirmUpdate(update) else {
                    updateStatusMessage = nil
                    return
                }

                updateStatusMessage = "正在下载 Kite \(update.latestVersion)…"
                let archive = try await Self.downloadUpdate(update)
                updateStatusMessage = "正在安装 Kite \(update.latestVersion)…"
                try KitePlatform.installUpdate(
                    archivePath: archive,
                    version: update.latestVersion
                )
                NSApp.terminate(nil)
            } catch {
                updateStatusMessage = nil
                errorMessage = error.localizedDescription
                showUpdateAlert(
                    title: "Kite 更新失败",
                    message: error.localizedDescription,
                    style: .warning
                )
            }
        }
    }

    nonisolated private static func fetchUpdateInfo() async throws -> UpdateInfoDto {
        try KiteCore().checkForUpdate()
    }

    nonisolated private static func downloadUpdate(_ update: UpdateInfoDto) async throws -> String {
        try KiteCore().downloadUpdate(update: update)
    }

    private func confirmUpdate(_ update: UpdateInfoDto) -> Bool {
        let alert = NSAlert()
        alert.alertStyle = .informational
        alert.messageText = "发现 Kite \(update.latestVersion)"
        alert.informativeText = "当前版本：\(update.currentVersion)\n\n\(summarizedReleaseNotes(update.releaseNotes))"
        alert.addButton(withTitle: "下载并更新")
        alert.addButton(withTitle: "稍后")
        return alert.runModal() == .alertFirstButtonReturn
    }

    private func showUpdateAlert(
        title: String,
        message: String,
        style: NSAlert.Style
    ) {
        let alert = NSAlert()
        alert.alertStyle = style
        alert.messageText = title
        alert.informativeText = message
        alert.addButton(withTitle: "确定")
        alert.runModal()
    }

    private func summarizedReleaseNotes(_ notes: String) -> String {
        let normalized = notes
            .split(whereSeparator: \.isNewline)
            .map { String($0).trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
        if normalized.isEmpty {
            return "GitHub Release 未提供更新说明。"
        }
        let limit = 1600
        guard normalized.count > limit else { return normalized }
        return String(normalized.prefix(limit)) + "\n…"
    }

    func openExternalFolders(_ paths: [String]) {
        for path in paths {
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
                  isDirectory.boolValue else {
                continue
            }
            newTab(path: path)
        }
    }

    func saveSession() {
        guard !tabs.isEmpty else { return }
        let savedTabs = tabs.map { tab in
            SessionTabDto(
                currentDirectory: tab.currentPath,
                viewMode: "columns",
                showPathBar: true
            )
        }
        let activeTab = UInt32(activeTabIndex ?? 0)
        let state = SessionStateDto(
            schemaVersion: Self.sessionSchemaVersion,
            activeTab: activeTab,
            tabs: savedTabs
        )
        do {
            try core.saveSession(state: state)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func restoreOrCreateTabs() {
        let fallbackPath = core.defaultDirectory()
        if let state = core.loadSession() {
            let requestedIndex = min(Int(state.activeTab), max(state.tabs.count - 1, 0))
            var restoredTabs: [BrowserTab] = []
            var restoredActiveID: UUID?
            for (index, savedTab) in state.tabs.enumerated() {
                guard let tab = try? makeTab(path: savedTab.currentDirectory) else {
                    continue
                }
                if index == requestedIndex {
                    restoredActiveID = tab.id
                }
                restoredTabs.append(tab)
            }
            if !restoredTabs.isEmpty {
                tabs = restoredTabs
                activeTabID = restoredActiveID ?? restoredTabs[0].id
                return
            }
        }

        do {
            let tab = try makeTab(path: fallbackPath)
            tabs = [tab]
            activeTabID = tab.id
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func makeTab(path: String, id: UUID = UUID()) throws -> BrowserTab {
        let currentListing = try core.listDirectory(path: path)
        var restoredColumns: [BrowserColumn] = []

        for ancestor in ancestorPaths(for: currentListing.path).dropLast() {
            if let listing = try? core.listDirectory(path: ancestor) {
                restoredColumns.append(column(from: listing))
            }
        }
        restoredColumns.append(column(from: currentListing))

        if restoredColumns.count > 1 {
            for index in 0..<(restoredColumns.count - 1) {
                restoredColumns[index].selectedPath = restoredColumns[index + 1].path
            }
        }

        return BrowserTab(
            id: id,
            columns: restoredColumns,
            address: currentListing.path
        )
    }

    private func ancestorPaths(for path: String) -> [String] {
        var current = URL(fileURLWithPath: path).standardizedFileURL
        var paths: [URL] = []
        while true {
            paths.append(current)
            let parent = current.deletingLastPathComponent()
            if parent.path == current.path {
                break
            }
            current = parent
        }
        return paths.reversed().map(\.path)
    }

    private func column(from listing: DirectoryListingDto) -> BrowserColumn {
        BrowserColumn(
            path: listing.path,
            entries: listing.entries.map(BrowserEntry.init(dto:)),
            selectedPath: nil
        )
    }

    private func columnTitle(for path: String) -> String {
        let name = URL(fileURLWithPath: path).lastPathComponent
        return name.isEmpty ? path : name
    }

    private func pathAfterRename(_ current: String, original: String, renamed: String) -> String {
        let currentURL = URL(fileURLWithPath: current).standardizedFileURL
        let originalURL = URL(fileURLWithPath: original).standardizedFileURL
        let renamedURL = URL(fileURLWithPath: renamed).standardizedFileURL
        let currentParts = currentURL.pathComponents
        let originalParts = originalURL.pathComponents
        guard currentParts.starts(with: originalParts) else {
            return current
        }
        let suffix = currentParts.dropFirst(originalParts.count)
        return suffix.reduce(renamedURL) { $0.appendingPathComponent($1) }.path
    }

    private func pathAfterRecycle(_ current: String, removed: String) -> String {
        let currentURL = URL(fileURLWithPath: current).standardizedFileURL
        let removedURL = URL(fileURLWithPath: removed).standardizedFileURL
        guard currentURL.pathComponents.starts(with: removedURL.pathComponents) else {
            return current
        }
        return removedURL.deletingLastPathComponent().path
    }

    private func confirmFolderOpenerChange(
        title: String,
        message: String,
        detail: String,
        action: String
    ) -> Bool {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = title
        alert.informativeText = "\(message)\n\(detail)"
        alert.addButton(withTitle: action)
        alert.addButton(withTitle: "取消")
        return alert.runModal() == .alertFirstButtonReturn
    }
}
