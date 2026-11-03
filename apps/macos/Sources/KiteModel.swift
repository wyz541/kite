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

enum BrowserColumnSortKey: String, CaseIterable, Identifiable {
    case name
    case modified
    case kind
    case size

    var id: String { rawValue }

    var title: String {
        switch self {
        case .name: return "名称"
        case .modified: return "修改日期"
        case .kind: return "类型"
        case .size: return "大小"
        }
    }
}

enum BrowserColumnFilter: String, CaseIterable, Identifiable {
    case all
    case folders
    case files
    case media
    case documents

    var id: String { rawValue }

    var title: String {
        switch self {
        case .all: return "全部项目"
        case .folders: return "仅文件夹"
        case .files: return "仅文件"
        case .media: return "媒体文件"
        case .documents: return "文档文件"
        }
    }

    func matches(_ entry: BrowserEntry) -> Bool {
        switch self {
        case .all:
            return true
        case .folders:
            return entry.isDirectory
        case .files:
            return !entry.isDirectory
        case .media:
            return !entry.isDirectory && BrowserFileKind.mediaExtensions.contains(entry.fileExtension)
        case .documents:
            return !entry.isDirectory && BrowserFileKind.documentExtensions.contains(entry.fileExtension)
        }
    }
}

enum BrowserColumnGroup: String, CaseIterable, Identifiable {
    case none
    case kind
    case extensionName
    case modified

    var id: String { rawValue }

    var title: String {
        switch self {
        case .none: return "不分组"
        case .kind: return "按类型"
        case .extensionName: return "按扩展名"
        case .modified: return "按修改日期"
        }
    }
}

enum BrowserColumnRow: Identifiable {
    case group(String)
    case entry(BrowserEntry)

    var id: String {
        switch self {
        case .group(let title): return "group:\(title)"
        case .entry(let entry): return "entry:\(entry.path)"
        }
    }
}

private enum BrowserFileKind {
    static let mediaExtensions: Set<String> = [
        "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico",
        "mp3", "wav", "flac", "aac", "ogg",
        "mp4", "mkv", "mov", "avi", "webm"
    ]

    static let documentExtensions: Set<String> = [
        "txt", "md", "log", "ini", "toml", "yaml", "yml", "json",
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx"
    ]
}

extension BrowserEntry {
    fileprivate var fileExtension: String {
        URL(fileURLWithPath: path).pathExtension.lowercased()
    }
}

struct BrowserColumn: Identifiable {
    let path: String
    var directoryEntries: [BrowserEntry]
    var searchEntries: [BrowserEntry]?
    var entries: [BrowserEntry]
    let skipped: UInt64
    let gitAvailable: Bool
    let svnAvailable: Bool
    var selectedPath: String?
    var sortKey: BrowserColumnSortKey
    var sortAscending: Bool
    var filter: BrowserColumnFilter
    var group: BrowserColumnGroup
    var searchQuery: String?
    var searchTruncated: Bool

    init(
        path: String,
        entries: [BrowserEntry],
        skipped: UInt64,
        gitAvailable: Bool,
        svnAvailable: Bool,
        selectedPath: String? = nil
    ) {
        self.path = path
        self.directoryEntries = entries
        self.searchEntries = nil
        self.entries = []
        self.skipped = skipped
        self.gitAvailable = gitAvailable
        self.svnAvailable = svnAvailable
        self.selectedPath = selectedPath
        self.sortKey = .name
        self.sortAscending = true
        self.filter = .all
        self.group = .none
        self.searchQuery = nil
        self.searchTruncated = false
        refreshView()
    }

    var id: String { path }
    var title: String {
        let name = FileManager.default.displayName(atPath: path)
        return name.isEmpty ? path : name
    }

    var isDefaultView: Bool {
        sortKey == .name && sortAscending && filter == .all && group == .none
    }

    var emptyLabel: String {
        if searchQuery != nil {
            return "无搜索结果"
        }
        if filter != .all {
            return "筛选后无项目"
        }
        return "文件夹为空"
    }

    var displayRows: [BrowserColumnRow] {
        guard group != .none else {
            return entries.map(BrowserColumnRow.entry)
        }

        let now = Date()
        var grouped: [String: (rank: Int, entries: [BrowserEntry])] = [:]
        for entry in entries {
            let key = groupKey(for: entry, now: now)
            var group = grouped[key.label] ?? (rank: key.rank, entries: [])
            group.entries.append(entry)
            grouped[key.label] = group
        }

        return grouped
            .sorted { left, right in
                if left.value.rank != right.value.rank {
                    return left.value.rank < right.value.rank
                }
                return left.key.localizedStandardCompare(right.key) == .orderedAscending
            }
            .flatMap { label, group in
                [BrowserColumnRow.group(label)] + group.entries.map(BrowserColumnRow.entry)
            }
    }

    mutating func toggleSortOrder() {
        sortAscending.toggle()
        refreshView()
    }

    mutating func setSortKey(_ key: BrowserColumnSortKey) {
        sortKey = key
        refreshView()
    }

    mutating func setFilter(_ value: BrowserColumnFilter) {
        filter = value
        refreshView()
    }

    mutating func setGroup(_ value: BrowserColumnGroup) {
        group = value
    }

    mutating func resetView() {
        sortKey = .name
        sortAscending = true
        filter = .all
        group = .none
        refreshView()
    }

    mutating func setSearchResults(query: String, entries: [BrowserEntry], truncated: Bool) {
        searchEntries = entries
        searchQuery = query
        searchTruncated = truncated
        refreshView()
    }

    mutating func clearSearchResults() {
        searchEntries = nil
        searchQuery = nil
        searchTruncated = false
        refreshView()
    }

    private var sourceEntries: [BrowserEntry] {
        searchEntries ?? directoryEntries
    }

    private mutating func refreshView() {
        entries = sourceEntries
            .filter { filter.matches($0) }
            .sorted { entryComesBefore($0, $1) }
        if let selectedPath,
           !entries.contains(where: { $0.path == selectedPath }) {
            self.selectedPath = nil
        }
    }

    private func entryComesBefore(_ left: BrowserEntry, _ right: BrowserEntry) -> Bool {
        if left.isDirectory != right.isDirectory {
            return left.isDirectory
        }

        let comparison: ComparisonResult
        switch sortKey {
        case .name:
            comparison = comparedNames(left, right)
        case .modified:
            switch (left.modifiedUnixMs, right.modifiedUnixMs) {
            case (nil, nil):
                return comparedNames(left, right) == .orderedAscending
            case (nil, _):
                return false
            case (_, nil):
                return true
            case (let leftValue?, let rightValue?):
                if leftValue == rightValue {
                    comparison = comparedNames(left, right)
                } else {
                    comparison = leftValue < rightValue ? .orderedAscending : .orderedDescending
                }
            }
        case .kind:
            comparison = left.kindLabel.localizedStandardCompare(right.kindLabel)
        case .size:
            if left.size == right.size {
                comparison = comparedNames(left, right)
            } else {
                comparison = left.size < right.size ? .orderedAscending : .orderedDescending
            }
        }

        if comparison == .orderedSame {
            return false
        }
        return sortAscending
            ? comparison == .orderedAscending
            : comparison == .orderedDescending
    }

    private func comparedNames(_ left: BrowserEntry, _ right: BrowserEntry) -> ComparisonResult {
        let comparison = left.name.localizedStandardCompare(right.name)
        return comparison == .orderedSame
            ? left.path.localizedStandardCompare(right.path)
            : comparison
    }

    private func groupKey(
        for entry: BrowserEntry,
        now: Date
    ) -> (rank: Int, label: String) {
        switch group {
        case .none:
            return (0, "")
        case .kind:
            return entry.isDirectory ? (0, "文件夹") : (1, "文件")
        case .extensionName:
            if entry.isDirectory {
                return (0, "文件夹")
            }
            if entry.fileExtension.isEmpty {
                return (2, "无扩展名")
            }
            return (1, entry.fileExtension.uppercased())
        case .modified:
            guard let modifiedUnixMs = entry.modifiedUnixMs else {
                return (4, "未知日期")
            }
            let modified = Date(timeIntervalSince1970: TimeInterval(modifiedUnixMs) / 1_000)
            let age = max(0, now.timeIntervalSince(modified))
            if age < 24 * 60 * 60 {
                return (0, "今天")
            }
            if age < 7 * 24 * 60 * 60 {
                return (1, "最近 7 天")
            }
            if age < 30 * 24 * 60 * 60 {
                return (2, "最近 30 天")
            }
            return (3, "更早")
        }
    }
}

struct BrowserTab: Identifiable {
    let id: UUID
    var columns: [BrowserColumn]
    var address: String

    var currentPath: String { columns.last?.path ?? address }
    var title: String {
        let name = FileManager.default.displayName(atPath: currentPath)
        return name.isEmpty ? currentPath : name
    }
}

struct UpdateDownloadState: Equatable {
    let downloadedBytes: UInt64
    let totalBytes: UInt64

    var fraction: Double? {
        guard totalBytes > 0 else { return nil }
        return min(max(Double(downloadedBytes) / Double(totalBytes), 0), 1)
    }

    var summaryText: String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        let downloaded = formatter.string(fromByteCount: Int64(clamping: downloadedBytes))
        guard totalBytes > 0 else { return downloaded }

        let total = formatter.string(fromByteCount: Int64(clamping: totalBytes))
        guard let fraction else { return "\(downloaded) / \(total)" }
        let percent = Int((fraction * 100).rounded())
        return "\(percent)% · \(downloaded) / \(total)"
    }
}

@MainActor
final class KiteModel: ObservableObject {
    private static let sessionSchemaVersion: UInt32 = 1

    private let core: KiteCore
    private var columnSearchGeneration: UInt64 = 0

    @Published private(set) var tabs: [BrowserTab] = []
    @Published private(set) var favorites: [FavoriteEntryDto] = []
    @Published private(set) var volumes: [VolumeDto] = []
    @Published private(set) var activeTabID: UUID?
    @Published var errorMessage: String?
    @Published var focusedAddressRequest = 0
    @Published private(set) var updateInProgress = false
    @Published private(set) var updateStatusMessage: String?
    @Published private(set) var updateDownload: UpdateDownloadState?
    @Published var activeColumnSearchPath: String?
    @Published var columnSearchText = ""
    @Published private(set) var columnSearchLoading = false

    init(core: KiteCore = KiteCore()) {
        self.core = core
        self.favorites = core.favorites()
        self.volumes = core.devices().volumes
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
            resetColumnSearchUI()
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
        resetColumnSearchUI()
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
        if wasActive {
            resetColumnSearchUI()
        }
        tabs.remove(at: index)
        if wasActive {
            activeTabID = tabs[min(index, tabs.count - 1)].id
        }
    }

    func activateTab(id: UUID) {
        guard tabs.contains(where: { $0.id == id }), activeTabID != id else { return }
        resetColumnSearchUI()
        activeTabID = id
    }

    func nextTab() {
        guard tabs.count > 1, let index = activeTabIndex else { return }
        resetColumnSearchUI()
        activeTabID = tabs[(index + 1) % tabs.count].id
    }

    func previousTab() {
        guard tabs.count > 1, let index = activeTabIndex else { return }
        resetColumnSearchUI()
        activeTabID = tabs[(index + tabs.count - 1) % tabs.count].id
    }

    func loadRoot(_ path: String) {
        guard let index = activeTabIndex else {
            newTab(path: path)
            return
        }
        do {
            let tab = try makeTab(path: path, id: tabs[index].id)
            resetColumnSearchUI()
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

    func toggleColumnSort(index: Int) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].toggleSortOrder()
    }

    func setColumnSortKey(index: Int, key: BrowserColumnSortKey) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].setSortKey(key)
    }

    func setColumnFilter(index: Int, filter: BrowserColumnFilter) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].setFilter(filter)
    }

    func setColumnGroup(index: Int, group: BrowserColumnGroup) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].setGroup(group)
    }

    func resetColumnView(index: Int) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].resetView()
    }

    func toggleColumnSearch(path: String) {
        if activeColumnSearchPath == path {
            clearColumnSearch(path: path)
            return
        }

        if let previousPath = activeColumnSearchPath {
            clearColumnSearch(path: previousPath)
        }

        resetColumnSearchRequest()
        activeColumnSearchPath = path
        columnSearchText = columnQuery(for: path) ?? ""
    }

    func setColumnSearchText(_ text: String) {
        columnSearchText = text
    }

    func submitColumnSearch(path: String) {
        let query = columnSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            clearColumnSearchResults(for: path)
            resetColumnSearchRequest()
            return
        }

        resetColumnSearchRequest()
        columnSearchGeneration &+= 1
        let generation = columnSearchGeneration
        columnSearchLoading = true
        let core = self.core

        Task {
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    try core.searchDirectory(path: path, query: query)
                }.value

                guard generation == self.columnSearchGeneration else { return }
                self.columnSearchLoading = false
                guard let result else { return }
                self.applyColumnSearchResult(result, path: path)
            } catch {
                guard generation == self.columnSearchGeneration else { return }
                self.columnSearchLoading = false
                self.errorMessage = error.localizedDescription
            }
        }
    }

    func clearColumnSearch(path: String) {
        resetColumnSearchRequest()
        clearColumnSearchResults(for: path)
        activeColumnSearchPath = nil
        columnSearchText = ""
    }

    private func applyColumnSearchResult(_ result: DirectorySearchDto, path: String) {
        guard let tabIndex = activeTabIndex,
              let columnIndex = tabs[tabIndex].columns.firstIndex(where: { $0.path == path }) else {
            return
        }
        let entries = result.entries.map(BrowserEntry.init(dto:))
        tabs[tabIndex].columns[columnIndex].setSearchResults(
            query: result.query,
            entries: entries,
            truncated: result.truncated
        )
        errorMessage = nil
    }

    private func clearColumnSearchResults(for path: String) {
        guard let tabIndex = activeTabIndex,
              let columnIndex = tabs[tabIndex].columns.firstIndex(where: { $0.path == path }) else {
            return
        }
        tabs[tabIndex].columns[columnIndex].clearSearchResults()
    }

    private func columnQuery(for path: String) -> String? {
        columns.first(where: { $0.path == path })?.searchQuery
    }

    private func resetColumnSearchRequest() {
        columnSearchGeneration &+= 1
        columnSearchLoading = false
        core.cancelDirectorySearch()
    }

    private func resetColumnSearchUI() {
        resetColumnSearchRequest()
        activeColumnSearchPath = nil
        columnSearchText = ""
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
        reloadVolumes()
    }

    func reloadVolumes() {
        volumes = core.devices().volumes
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

    func isFavorite(path: String) -> Bool {
        favorite(at: path) != nil
    }

    func toggleFavorite(path: String, name: String, isDirectory: Bool) {
        if let favorite = favorite(at: path) {
            removeFavorite(favorite)
        } else {
            addFavorite(path: path, name: name, isDirectory: isDirectory)
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

    private func favorite(at path: String) -> FavoriteEntryDto? {
        let normalizedPath = URL(fileURLWithPath: path).standardizedFileURL.path
        return favorites.first {
            URL(fileURLWithPath: $0.path).standardizedFileURL.path == normalizedPath
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

    func setAsDefaultFileViewer() {
        let canSetFolderHandler = KitePlatform.canSetDefaultFolderHandler
        guard confirmFileViewerChange(
            title: canSetFolderHandler ? "设为默认文件夹打开方式？" : "设为默认文件查看器？",
            message: canSetFolderHandler
                ? "Kite 将成为文件夹的默认应用。"
                : "Kite 将处理“在 Finder 中显示”请求。",
            detail: canSetFolderHandler
                ? "Finder 仍将作为系统外壳。"
                : "macOS 26 及更高版本保留 Finder 为默认文件夹应用。",
            action: "继续"
        ) else {
            return
        }
        Task {
            do {
                try await KitePlatform.setKiteAsDefaultFileViewer()
                errorMessage = nil
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func restoreFinderAsDefaultFileViewer() {
        let canSetFolderHandler = KitePlatform.canSetDefaultFolderHandler
        guard confirmFileViewerChange(
            title: "恢复 Finder？",
            message: canSetFolderHandler
                ? "Finder 将重新成为文件夹的默认应用。"
                : "Finder 将处理“在 Finder 中显示”请求。",
            detail: canSetFolderHandler
                ? "Kite 的其他文件和目录保持不变。"
                : "文件夹双击仍由 Finder 处理。",
            action: "恢复"
        ) else {
            return
        }
        Task {
            do {
                try await KitePlatform.restoreFinderAsDefaultFileViewer()
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
        updateDownload = nil
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
                let archive: String
                do {
                    let progressTask = startUpdateDownloadMonitor(initialTotal: update.assetSize)
                    defer {
                        progressTask.cancel()
                        updateDownload = nil
                    }
                    archive = try await Self.downloadUpdate(update)
                }
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

    @MainActor
    private func startUpdateDownloadMonitor(initialTotal: UInt64) -> Task<Void, Never> {
        updateDownload = UpdateDownloadState(
            downloadedBytes: 0,
            totalBytes: initialTotal
        )

        return Task { @MainActor [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                let progress = self.core.downloadProgress()
                if progress.active || progress.downloadedBytes == 0 {
                    self.updateDownload = UpdateDownloadState(
                        downloadedBytes: progress.downloadedBytes,
                        totalBytes: progress.totalBytes
                    )
                }
                try? await Task.sleep(nanoseconds: 150_000_000)
            }
        }
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

    func revealExternalItems(_ paths: [String]) {
        for path in paths {
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory) else {
                continue
            }

            if isDirectory.boolValue {
                newTab(path: path)
                continue
            }

            let parent = URL(fileURLWithPath: path).deletingLastPathComponent().path
            do {
                let tab = try makeTab(path: parent)
                tabs.append(tab)
                activeTabID = tab.id
                if let index = activeTabIndex,
                   let columnIndex = tabs[index].columns.indices.last {
                    tabs[index].columns[columnIndex].selectedPath = path
                }
                errorMessage = nil
            } catch {
                errorMessage = error.localizedDescription
            }
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
            skipped: listing.skipped,
            gitAvailable: listing.gitAvailable,
            svnAvailable: listing.svnAvailable
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

    private func confirmFileViewerChange(
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
