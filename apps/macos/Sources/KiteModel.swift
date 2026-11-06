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

enum BrowserColumnSortKey: String, CaseIterable, Identifiable, Codable {
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

    var defaultAscending: Bool {
        switch self {
        case .name, .kind: return true
        case .modified, .size: return false
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

enum BrowserColumnGroup: String, CaseIterable, Identifiable, Codable {
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
    case group(title: String, count: Int, isCollapsed: Bool)
    case entry(BrowserEntry)

    var id: String {
        switch self {
        case .group(let title, _, _): return "group:\(title)"
        case .entry(let entry): return "entry:\(entry.path)"
        }
    }
}

struct StoredColumnViewRule: Codable, Equatable {
    var sortKey: BrowserColumnSortKey
    var sortAscending: Bool
    var group: BrowserColumnGroup
    var collapsedGroups: Set<String>

    static let `default` = StoredColumnViewRule(
        sortKey: .name,
        sortAscending: true,
        group: .none,
        collapsedGroups: []
    )

    var withoutCollapsedGroups: StoredColumnViewRule {
        var rule = self
        rule.collapsedGroups = []
        return rule
    }
}

struct StoredColumnViewRules: Codable, Equatable {
    var defaultRule: StoredColumnViewRule
    var rules: [String: StoredColumnViewRule]

    static let `default` = StoredColumnViewRules(
        defaultRule: .default,
        rules: [:]
    )
}

enum BrowserSelectionMode: Equatable {
    case replace
    case toggle
    case range

    init(modifiers: NSEvent.ModifierFlags) {
        if modifiers.contains(.shift) {
            self = .range
        } else if modifiers.contains(.command) {
            self = .toggle
        } else {
            self = .replace
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
    var skipped: UInt64
    var gitAvailable: Bool
    var svnAvailable: Bool
    var selectedPath: String?
    var selectedPaths: Set<String>
    var selectionAnchorPath: String?
    var sortKey: BrowserColumnSortKey
    var sortAscending: Bool
    var filter: BrowserColumnFilter
    var group: BrowserColumnGroup
    var collapsedGroups: Set<String>
    var searchQuery: String?
    var searchTruncated: Bool

    init(
        path: String,
        entries: [BrowserEntry],
        skipped: UInt64,
        gitAvailable: Bool,
        svnAvailable: Bool,
        selectedPath: String? = nil,
        rule: StoredColumnViewRule = .default
    ) {
        self.path = path
        self.directoryEntries = entries
        self.searchEntries = nil
        self.entries = []
        self.skipped = skipped
        self.gitAvailable = gitAvailable
        self.svnAvailable = svnAvailable
        self.selectedPath = selectedPath
        self.selectedPaths = selectedPath.map { [$0] } ?? []
        self.selectionAnchorPath = selectedPath
        self.sortKey = rule.sortKey
        self.sortAscending = rule.sortAscending
        self.filter = .all
        self.group = rule.group
        self.collapsedGroups = rule.collapsedGroups
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
                let isCollapsed = collapsedGroups.contains(label)
                var rows = [
                    BrowserColumnRow.group(
                        title: label,
                        count: group.entries.count,
                        isCollapsed: isCollapsed
                    )
                ]
                if !isCollapsed {
                    rows.append(contentsOf: group.entries.map(BrowserColumnRow.entry))
                }
                return rows
            }
    }

    var storedRule: StoredColumnViewRule {
        StoredColumnViewRule(
            sortKey: sortKey,
            sortAscending: sortAscending,
            group: group,
            collapsedGroups: collapsedGroups
        )
    }

    mutating func toggleSortOrder() {
        sortAscending.toggle()
        refreshView()
    }

    mutating func setSortKey(_ key: BrowserColumnSortKey) {
        if sortKey != key {
            sortAscending = key.defaultAscending
        }
        sortKey = key
        refreshView()
    }

    mutating func toggleSort(key: BrowserColumnSortKey) {
        if sortKey == key {
            sortAscending.toggle()
        } else {
            sortKey = key
            sortAscending = key.defaultAscending
        }
        refreshView()
    }

    mutating func setFilter(_ value: BrowserColumnFilter) {
        filter = value
        refreshView()
    }

    mutating func setGroup(_ value: BrowserColumnGroup) {
        if group != value {
            collapsedGroups.removeAll()
        }
        group = value
    }

    mutating func toggleGroupCollapse(_ title: String) {
        if collapsedGroups.contains(title) {
            collapsedGroups.remove(title)
        } else {
            collapsedGroups.insert(title)
            pruneSelectionToVisibleRows()
        }
    }

    mutating func resetView() {
        sortKey = .name
        sortAscending = true
        filter = .all
        group = .none
        collapsedGroups.removeAll()
        refreshView()
    }

    private mutating func pruneSelectionToVisibleRows() {
        let visiblePaths = Set(displayRows.compactMap { row -> String? in
            guard case .entry(let entry) = row else { return nil }
            return entry.path
        })
        selectedPaths.formIntersection(visiblePaths)
        if let selectedPath, !visiblePaths.contains(selectedPath) {
            self.selectedPath = nil
        }
        if let selectionAnchorPath, !visiblePaths.contains(selectionAnchorPath) {
            self.selectionAnchorPath = nil
        }
        if selectedPath == nil {
            selectedPath = selectedPaths.first
        }
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

    mutating func updateListing(
        entries: [BrowserEntry],
        skipped: UInt64,
        gitAvailable: Bool,
        svnAvailable: Bool
    ) {
        directoryEntries = entries
        self.skipped = skipped
        self.gitAvailable = gitAvailable
        self.svnAvailable = svnAvailable
        refreshView()
    }

    mutating func select(path: String, mode: BrowserSelectionMode) {
        switch mode {
        case .replace:
            selectedPaths = [path]
            selectedPath = path
            selectionAnchorPath = path
        case .toggle:
            if selectedPaths.contains(path) {
                selectedPaths.remove(path)
                selectedPath = selectedPaths.min()
            } else {
                selectedPaths.insert(path)
                selectedPath = path
            }
            selectionAnchorPath = path
        case .range:
            let paths = displayRows.compactMap { row -> String? in
                guard case .entry(let entry) = row else { return nil }
                return entry.path
            }
            let anchor = selectionAnchorPath.flatMap { paths.firstIndex(of: $0) }
                ?? selectedPath.flatMap { paths.firstIndex(of: $0) }
            guard let anchor, let target = paths.firstIndex(of: path) else {
                selectedPaths = [path]
                selectedPath = path
                selectionAnchorPath = path
                return
            }
            let range = anchor <= target ? anchor...target : target...anchor
            selectedPaths = Set(paths[range])
            selectedPath = path
            selectionAnchorPath = paths[anchor]
        }
    }

    mutating func selectAll() {
        selectedPaths = Set(entries.map(\.path))
        selectedPath = entries.first?.path
        selectionAnchorPath = entries.first?.path
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
        let visiblePaths = Set(entries.map(\.path))
        selectedPaths.formIntersection(visiblePaths)
        if let selectionAnchorPath, !visiblePaths.contains(selectionAnchorPath) {
            self.selectionAnchorPath = nil
        }
        if self.selectedPath == nil {
            self.selectedPath = selectedPaths.min()
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

struct BrowserGlobalSearchState {
    let roots: [String]
    let query: String
    let entries: [BrowserEntry]
    let groups: [BrowserGlobalSearchGroup]
    let truncated: Bool

    init(dto: GlobalSearchResultDto) {
        roots = dto.roots
        query = dto.query
        entries = dto.entries.map(BrowserEntry.init(dto:))
        groups = Self.group(entries)
        truncated = dto.truncated
    }

    private static func group(_ entries: [BrowserEntry]) -> [BrowserGlobalSearchGroup] {
        var groups: [BrowserGlobalSearchGroup] = []
        var indexes: [String: Int] = [:]
        for entry in entries {
            let key = "\(entry.isDirectory ? "dir" : "file"):\(entry.name.lowercased())"
            if let index = indexes[key] {
                groups[index].entries.append(entry)
            } else {
                indexes[key] = groups.count
                groups.append(BrowserGlobalSearchGroup(
                    key: key,
                    name: entry.name,
                    entries: [entry]
                ))
            }
        }
        return groups
    }
}

struct BrowserGlobalSearchGroup: Identifiable {
    let key: String
    let name: String
    var entries: [BrowserEntry]

    var id: String { key }
}

enum GlobalSearchMode: String, CaseIterable, Identifiable {
    case name
    case content

    var id: String { rawValue }

    var title: String {
        switch self {
        case .name: return "名称"
        case .content: return "内容"
        }
    }
}

enum GlobalSearchScope: String, CaseIterable, Identifiable {
    case currentFolder
    case favorites
    case localDisks
    case rootDisk

    var id: String { rawValue }

    var title: String {
        switch self {
        case .currentFolder: return "当前文件夹"
        case .favorites: return "收藏文件夹"
        case .localDisks: return "本地磁盘"
        case .rootDisk: return "根磁盘"
        }
    }

    var allowsLiveSearch: Bool {
        self == .currentFolder
    }

    var preparesContentIndexInBackground: Bool {
        self == .currentFolder || self == .favorites
    }
}

struct BrowserContentLineMatch: Identifiable {
    let lineNumber: UInt64
    let columnStart: UInt64?
    let columnEnd: UInt64?
    let text: String

    var id: String { "\(lineNumber):\(columnStart ?? 0):\(text)" }
}

struct BrowserContentFileMatch: Identifiable {
    let path: String
    let root: String
    let relativePath: String
    let name: String
    let kindLabel: String
    let size: UInt64
    let modifiedUnixMs: Int64?
    let score: Double
    let matchCount: UInt64
    let matches: [BrowserContentLineMatch]

    var id: String { path }

    init(dto: ContentFileMatchDto) {
        path = dto.path
        root = dto.root
        relativePath = dto.relativePath
        name = dto.name
        kindLabel = dto.kindLabel
        size = dto.size
        modifiedUnixMs = dto.modifiedUnixMs
        score = dto.score
        matchCount = dto.matchCount
        matches = dto.matches.map { line in
            BrowserContentLineMatch(
                lineNumber: line.lineNumber,
                columnStart: line.columnStart,
                columnEnd: line.columnEnd,
                text: line.text
            )
        }
    }
}

struct BrowserContentSearchState {
    let query: String
    let roots: [String]
    let files: [BrowserContentFileMatch]
    let indexState: String
    let partial: Bool
    let truncated: Bool
    let scannedDocuments: UInt64
    let totalMatches: UInt64
    let durationMs: UInt64

    init(dto: ContentSearchResultDto) {
        query = dto.query
        roots = dto.roots
        files = dto.files.map(BrowserContentFileMatch.init(dto:))
        indexState = dto.indexState
        partial = dto.partial
        truncated = dto.truncated
        scannedDocuments = dto.scannedDocuments
        totalMatches = dto.totalMatches
        durationMs = dto.durationMs
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
    private static let columnSearchDebounceNanoseconds: UInt64 = 180_000_000
    private static let columnViewRulesDefaultsKey = "kite.macos.column-view-rules-v1"
    private static let maxStoredColumnRules = 512

    private let core: KiteCore
    private var storedColumnViewRules: StoredColumnViewRules
    private var columnSearchGeneration: UInt64 = 0
    private var globalSearchGeneration: UInt64 = 0
    private var contentSearchGeneration: UInt64 = 0
    private var globalSearchDebounceTask: Task<Void, Never>?
    private var columnSearchDebounceTask: Task<Void, Never>?
    private var columnAutoRefreshGeneration: UInt64 = 0
    private var columnAutoRefreshTask: Task<Void, Never>?
    private var contentSearchPowerPolicyTask: Task<Void, Never>?
    private lazy var columnDirectoryWatcher = DirectoryWatcher { [weak self] in
        self?.directoryContentsDidChange()
    }

    @Published private(set) var tabs: [BrowserTab] = []
    @Published private(set) var favorites: [FavoriteEntryDto] = []
    @Published private(set) var volumes: [VolumeDto] = []
    @Published private(set) var activeTabID: UUID?
    @Published var errorMessage: String?
    @Published var focusedAddressRequest = 0
    @Published var focusedGlobalSearchRequest = 0
    @Published private(set) var updateInProgress = false
    @Published private(set) var updateStatusMessage: String?
    @Published private(set) var updateDownload: UpdateDownloadState?
    @Published var activeColumnSearchPath: String?
    @Published var columnSearchText = ""
    @Published private(set) var columnSearchLoading = false
    @Published var globalSearchText = ""
    @Published var globalSearchMode: GlobalSearchMode = .name
    @Published var globalSearchScope: GlobalSearchScope = .currentFolder
    @Published private(set) var globalSearch: BrowserGlobalSearchState?
    @Published private(set) var globalSearchLoading = false
    @Published private(set) var contentSearch: BrowserContentSearchState?
    @Published private(set) var contentSearchLoading = false

    init(core: KiteCore = KiteCore()) {
        let storedColumnViewRules = Self.loadStoredColumnViewRules()
        self.core = core
        self.storedColumnViewRules = storedColumnViewRules
        self.favorites = core.favorites()
        self.volumes = core.devices().volumes
        restoreOrCreateTabs()
        syncColumnDirectoryWatchers()
        updateContentSearchPowerPolicy()
        contentSearchPowerPolicyTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 5_000_000_000)
                guard !Task.isCancelled else { return }
                self?.updateContentSearchPowerPolicy()
            }
        }
    }

    deinit {
        contentSearchPowerPolicyTask?.cancel()
        globalSearchDebounceTask?.cancel()
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

    var selectedColumnEntries: [BrowserEntry] {
        guard let column = columns.last else { return [] }
        return column.entries.filter { column.selectedPaths.contains($0.path) }
    }

    var selectedColumnEntry: BrowserEntry? {
        guard let column = columns.last,
              column.selectedPaths.count == 1,
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

    func select(
        column index: Int,
        entry: BrowserEntry,
        modifiers: NSEvent.ModifierFlags = []
    ) {
        guard let tabIndex = activeTabIndex, tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        let mode = BrowserSelectionMode(modifiers: modifiers)
        if mode != .replace, index != tabs[tabIndex].columns.count - 1 {
            return
        }
        tabs[tabIndex].columns[index].select(path: entry.path, mode: mode)
        guard mode == .replace else { return }
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

    func selectAll() {
        guard let tabIndex = activeTabIndex,
              let columnIndex = tabs[tabIndex].columns.indices.last else {
            return
        }
        tabs[tabIndex].columns[columnIndex].selectAll()
    }

    func toggleColumnSort(index: Int) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].toggleSortOrder()
        persistColumnViewRule(index: index)
    }

    func setColumnSortKey(index: Int, key: BrowserColumnSortKey) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].setSortKey(key)
        persistColumnViewRule(index: index)
    }

    func toggleColumnSort(index: Int, key: BrowserColumnSortKey) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].toggleSort(key: key)
        persistColumnViewRule(index: index)
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
        persistColumnViewRule(index: index)
    }

    func toggleColumnGroup(index: Int, title: String) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].toggleGroupCollapse(title)
        persistColumnViewRule(index: index)
    }

    func resetColumnView(index: Int) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        tabs[tabIndex].columns[index].resetView()
        persistColumnViewRule(index: index)
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
        guard let path = activeColumnSearchPath else { return }

        resetColumnSearchRequest()
        cancelColumnSearchDebounce()

        let query = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            clearColumnSearchResults(for: path)
            return
        }

        columnSearchDebounceTask = Task { [weak self] in
            try? await Task<Never, Never>.sleep(
                nanoseconds: Self.columnSearchDebounceNanoseconds
            )
            guard !Task.isCancelled, let self else { return }
            guard self.activeColumnSearchPath == path,
                  self.columnSearchText.trimmingCharacters(in: .whitespacesAndNewlines) == query
            else { return }
            self.startColumnSearch(path: path, query: query)
        }
    }

    func submitColumnSearch(path: String) {
        cancelColumnSearchDebounce()
        startColumnSearch(path: path, query: columnSearchText)
    }

    private func startColumnSearch(path: String, query: String) {
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            clearColumnSearchResults(for: path)
            resetColumnSearchRequest()
            return
        }

        resetColumnSearchRequest()
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
        cancelColumnSearchDebounce()
        resetColumnSearchRequest()
        clearColumnSearchResults(for: path)
        activeColumnSearchPath = nil
        columnSearchText = ""
    }

    var hasGlobalSearchActivity: Bool {
        globalSearch != nil
            || globalSearchLoading
            || contentSearch != nil
            || contentSearchLoading
    }

    var currentSearchScopeTitle: String {
        globalSearchScope.title
    }

    func submitGlobalSearch() {
        if !globalSearchScope.allowsLiveSearch && (globalSearchLoading || contentSearchLoading) {
            errorMessage = "当前范围正在搜索，请等待本次搜索结束"
            return
        }
        cancelGlobalSearchDebounce()
        switch globalSearchMode {
        case .name:
            submitNameGlobalSearch()
        case .content:
            submitContentGlobalSearch()
        }
    }

    private func submitNameGlobalSearch() {
        let query = globalSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            clearGlobalSearch()
            return
        }

        globalSearchGeneration &+= 1
        let generation = globalSearchGeneration
        let roots = searchRoots()
        guard !roots.isEmpty else {
            errorMessage = "当前搜索范围没有可搜索的文件夹"
            return
        }
        core.cancelDirectorySearch()
        core.cancelContentSearch()
        globalSearchLoading = true
        contentSearchLoading = false
        globalSearch = nil
        contentSearch = nil
        errorMessage = nil
        let core = self.core

        Task {
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    try core.searchDirectories(paths: roots, query: query)
                }.value

                guard generation == self.globalSearchGeneration else { return }
                self.globalSearchLoading = false
                guard let result else { return }
                self.globalSearch = BrowserGlobalSearchState(dto: result)
                self.errorMessage = nil
            } catch {
                guard generation == self.globalSearchGeneration else { return }
                self.globalSearchLoading = false
                self.errorMessage = error.localizedDescription
            }
        }
    }

    private func submitContentGlobalSearch() {
        let query = globalSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            clearGlobalSearch()
            return
        }
        let roots = searchRoots()
        guard !roots.isEmpty else {
            errorMessage = "当前搜索范围没有可搜索的文件夹"
            return
        }

        contentSearchGeneration &+= 1
        let generation = contentSearchGeneration
        core.cancelDirectorySearch()
        core.cancelContentSearch()
        core.noteContentSearchInteraction()
        globalSearchLoading = false
        contentSearchLoading = true
        globalSearch = nil
        contentSearch = nil
        errorMessage = nil
        let core = self.core

        Task {
            do {
                try await Task.detached(priority: .utility) {
                    try core.prepareContentSearch(roots: roots)
                }.value
                let result = try await Task.detached(priority: .userInitiated) {
                    try core.searchContent(
                        roots: roots,
                        query: query,
                        regex: false,
                        maxFiles: 50
                    )
                }.value

                guard generation == self.contentSearchGeneration else { return }
                self.contentSearchLoading = false
                guard let result else { return }
                self.contentSearch = BrowserContentSearchState(dto: result)
                self.errorMessage = nil
            } catch {
                guard generation == self.contentSearchGeneration else { return }
                self.contentSearchLoading = false
                self.errorMessage = error.localizedDescription
            }
        }
    }

    func globalSearchModeDidChange() {
        cancelGlobalSearchDebounce()
        globalSearchGeneration &+= 1
        contentSearchGeneration &+= 1
        core.cancelDirectorySearch()
        core.cancelContentSearch()
        globalSearchLoading = false
        contentSearchLoading = false
        globalSearch = nil
        contentSearch = nil
        errorMessage = nil
        if globalSearchMode == .content {
            prepareContentIndex()
        }
        scheduleGlobalSearch()
    }

    func globalSearchScopeDidChange() {
        cancelGlobalSearchDebounce()
        globalSearchGeneration &+= 1
        contentSearchGeneration &+= 1
        core.cancelDirectorySearch()
        core.cancelContentSearch()
        globalSearchLoading = false
        contentSearchLoading = false
        globalSearch = nil
        contentSearch = nil
        errorMessage = nil
        if globalSearchMode == .content {
            prepareContentIndex()
        }
        scheduleGlobalSearch()
    }

    func globalSearchTextDidChange() {
        if globalSearchMode == .content {
            noteContentSearchInteraction()
        }
        scheduleGlobalSearch()
    }

    func noteContentSearchInteraction() {
        core.noteContentSearchInteraction()
    }

    private func prepareContentIndex() {
        guard globalSearchScope.preparesContentIndexInBackground else { return }
        updateContentSearchPowerPolicy()
        let roots = searchRoots()
        guard !roots.isEmpty else { return }
        let core = self.core
        Task.detached(priority: .utility) {
            try? core.prepareContentSearch(roots: roots)
        }
    }

    func globalSearchScopeHasRoots(_ scope: GlobalSearchScope) -> Bool {
        !roots(for: scope).isEmpty
    }

    private func searchRoots() -> [String] {
        roots(for: globalSearchScope)
    }

    private func roots(for scope: GlobalSearchScope) -> [String] {
        switch scope {
        case .currentFolder:
            return [currentPath]
        case .favorites:
            return favorites.filter(\.isDirectory).map(\.path)
        case .localDisks:
            return volumes
                .filter { ["fixed", "usb", "removable"].contains($0.kind) }
                .map(\.path)
        case .rootDisk:
            return ["/"]
        }
    }

    private func scheduleGlobalSearch() {
        cancelGlobalSearchDebounce()
        guard globalSearchScope.allowsLiveSearch else { return }
        let query = globalSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard query.count >= 2 else { return }
        globalSearchDebounceTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 280_000_000)
            guard !Task.isCancelled, let self else { return }
            let current = self.globalSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
            guard current == query else { return }
            self.submitGlobalSearch()
        }
    }

    private func cancelGlobalSearchDebounce() {
        globalSearchDebounceTask?.cancel()
        globalSearchDebounceTask = nil
    }

    private func updateContentSearchPowerPolicy() {
        let processInfo = ProcessInfo.processInfo
        let thermalPressure = processInfo.thermalState == .serious
            || processInfo.thermalState == .critical
        core.setContentSearchPaused(
            paused: processInfo.isLowPowerModeEnabled || thermalPressure
        )
    }

    func clearGlobalSearch() {
        cancelGlobalSearchDebounce()
        globalSearchGeneration &+= 1
        contentSearchGeneration &+= 1
        core.cancelDirectorySearch()
        core.cancelContentSearch()
        globalSearchLoading = false
        contentSearchLoading = false
        globalSearch = nil
        contentSearch = nil
        globalSearchText = ""
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

    private func cancelColumnSearchDebounce() {
        columnSearchDebounceTask?.cancel()
        columnSearchDebounceTask = nil
    }

    private func resetColumnSearchRequest() {
        columnSearchGeneration &+= 1
        columnSearchLoading = false
        core.cancelDirectorySearch()
    }

    private func resetColumnSearchUI() {
        cancelColumnSearchDebounce()
        resetColumnSearchRequest()
        activeColumnSearchPath = nil
        columnSearchText = ""
        clearGlobalSearch()
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

    func focusGlobalSearch() {
        focusedGlobalSearchRequest += 1
    }

    func refresh() {
        if hasGlobalSearchActivity {
            submitGlobalSearch()
            reloadVolumes()
            return
        }
        refreshVisibleColumns()
    }

    func syncColumnDirectoryWatchers() {
        columnDirectoryWatcher.update(paths: columns.map(\.path))
    }

    private func directoryContentsDidChange() {
        scheduleAutomaticColumnRefresh()
    }

    private func scheduleAutomaticColumnRefresh() {
        guard let tabIndex = activeTabIndex, !tabs[tabIndex].columns.isEmpty else {
            return
        }

        let tabID = tabs[tabIndex].id
        let paths = tabs[tabIndex].columns.map(\.path)
        let core = self.core
        columnAutoRefreshGeneration &+= 1
        let generation = columnAutoRefreshGeneration
        columnAutoRefreshTask?.cancel()
        columnAutoRefreshTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 80_000_000)
            guard !Task.isCancelled, let self else { return }

            let listings = await Task.detached(priority: .utility) {
                paths.map { path in
                    try? core.listDirectory(path: path)
                }
            }.value
            guard !Task.isCancelled,
                  self.columnAutoRefreshGeneration == generation,
                  self.activeTabID == tabID,
                  let currentIndex = self.activeTabIndex,
                  self.tabs[currentIndex].columns.map(\.path) == paths else {
                return
            }

            var changed = false
            for (index, listing) in listings.enumerated() {
                guard let listing,
                      self.tabs[currentIndex].columns.indices.contains(index),
                      self.tabs[currentIndex].columns[index].path == listing.path else {
                    continue
                }
                let column = self.tabs[currentIndex].columns[index]
                self.tabs[currentIndex].columns[index].updateListing(
                    entries: listing.entries.map(BrowserEntry.init(dto:)),
                    skipped: listing.skipped,
                    gitAvailable: listing.gitAvailable,
                    svnAvailable: listing.svnAvailable
                )
                let updated = self.tabs[currentIndex].columns[index]
                changed = changed
                    || column.directoryEntries != updated.directoryEntries
                    || column.skipped != updated.skipped
                    || column.gitAvailable != updated.gitAvailable
                    || column.svnAvailable != updated.svnAvailable
            }

            if changed {
                self.trimColumnsAfterMissingSelection(tabIndex: currentIndex)
            }
            self.syncColumnDirectoryWatchers()
        }
    }

    private func trimColumnsAfterMissingSelection(tabIndex: Int) {
        guard tabs.indices.contains(tabIndex) else { return }
        var keepCount = tabs[tabIndex].columns.count
        for index in 1..<tabs[tabIndex].columns.count {
            let childPath = tabs[tabIndex].columns[index].path
            let parentColumn = tabs[tabIndex].columns[index - 1]
            let keepsChild = parentColumn.selectedPath == childPath
                && parentColumn.entries.contains { $0.path == childPath && $0.isDirectory }
            if !keepsChild {
                keepCount = index
                break
            }
        }
        if keepCount < tabs[tabIndex].columns.count {
            tabs[tabIndex].columns.removeSubrange(keepCount..<tabs[tabIndex].columns.count)
        }
    }

    func reloadVolumes() {
        volumes = core.devices().volumes
    }

    func copyFile(_ path: String) {
        copyFiles([path])
    }

    func copyFiles(_ paths: [String]) {
        KitePlatform.writeFilePasteboard(paths, cut: false)
        errorMessage = nil
    }

    func cutFile(_ path: String) {
        cutFiles([path])
    }

    func cutFiles(_ paths: [String]) {
        KitePlatform.writeFilePasteboard(paths, cut: true)
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
            var completed = 0
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
                    completed += 1
                }
                if moveFiles {
                    KitePlatform.clearFilePasteboard()
                }
                errorMessage = nil
                refreshVisibleColumns()
            } catch {
                if moveFiles, completed > 0 {
                    let remaining = Array(paths.dropFirst(completed))
                    if remaining.isEmpty {
                        KitePlatform.clearFilePasteboard()
                    } else {
                        KitePlatform.writeFilePasteboard(remaining, cut: true)
                    }
                }
                refreshVisibleColumns()
                errorMessage = completed > 0
                    ? "已处理 \(completed) 项，后续操作失败：\(error.localizedDescription)"
                    : error.localizedDescription
            }
        }
    }

    func copyTo(path: String) {
        copyTo(paths: [path])
    }

    func copyTo(paths: [String]) {
        guard let destination = KitePlatform.chooseDirectory(prompt: "拷贝到这里") else {
            return
        }
        transferPaths(paths, to: destination, moveFiles: false)
    }

    func compress(_ path: String, format: String) {
        let core = self.core
        Task {
            do {
                _ = try await Task.detached(priority: .userInitiated) {
                    try core.compressPath(path: path, format: format)
                }.value
                errorMessage = nil
                refreshVisibleColumns()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func extract(_ path: String, to directory: String) {
        let core = self.core
        Task {
            do {
                _ = try await Task.detached(priority: .userInitiated) {
                    try core.extractArchive(path: path, destinationDirectory: directory)
                }.value
                errorMessage = nil
                refreshVisibleColumns()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func extractTo(_ path: String) {
        guard let destination = KitePlatform.chooseDirectory(prompt: "解压到这里") else {
            return
        }
        extract(path, to: destination)
    }

    func dragPaths(column index: Int, entry: BrowserEntry) -> [String] {
        guard columns.indices.contains(index) else { return [entry.path] }
        let column = columns[index]
        guard column.selectedPaths.contains(entry.path),
              column.selectedPaths.count > 1 else {
            return [entry.path]
        }
        return column.entries
            .filter { column.selectedPaths.contains($0.path) }
            .map(\.path)
    }

    func handleFileDrop(_ providers: [NSItemProvider], destination: String) {
        let copiesInternalDrop = NSEvent.modifierFlags.contains(.option)
        KitePlatform.loadDroppedFilePaths(from: providers) { [weak self] paths, isInternal in
            Task { @MainActor in
                guard let self, !paths.isEmpty else { return }
                guard isInternal else {
                    self.transferPaths(paths, to: destination, moveFiles: false)
                    return
                }

                switch KiteFileDragProvider.internalDragOperation(
                    copyRequested: copiesInternalDrop
                ) {
                case .cut:
                    self.cutFiles(paths)
                case .copy:
                    self.copyFiles(paths)
                }
                self.pasteFiles(into: destination)
            }
        }
    }

    private func transferPaths(_ paths: [String], to directory: String, moveFiles: Bool) {
        var seen = Set<String>()
        let paths = paths.filter { seen.insert($0).inserted }
        guard !paths.isEmpty else { return }
        let core = self.core
        Task {
            var completed = 0
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
                    completed += 1
                }
                errorMessage = nil
                revealDropDestination(directory)
            } catch {
                refresh()
                errorMessage = completed > 0
                    ? "已处理 \(completed) 项，后续操作失败：\(error.localizedDescription)"
                    : error.localizedDescription
            }
        }
    }

    private func revealDropDestination(_ directory: String) {
        let current = URL(fileURLWithPath: currentPath).standardizedFileURL
        let destination = URL(fileURLWithPath: directory).standardizedFileURL
        if current.path == destination.path {
            refresh()
        } else {
            loadRoot(destination.path)
        }
    }

    private func refreshVisibleColumns() {
        guard let tabIndex = activeTabIndex, !tabs[tabIndex].columns.isEmpty else {
            return
        }

        do {
            var refreshedColumns = tabs[tabIndex].columns
            for index in refreshedColumns.indices {
                let listing = try core.listDirectory(path: refreshedColumns[index].path)
                refreshedColumns[index].updateListing(
                    entries: listing.entries.map(BrowserEntry.init(dto:)),
                    skipped: listing.skipped,
                    gitAvailable: listing.gitAvailable,
                    svnAvailable: listing.svnAvailable
                )
            }
            tabs[tabIndex].columns = refreshedColumns
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
        syncColumnDirectoryWatchers()
        reloadVolumes()
    }

    func moveToTrash(_ entry: BrowserEntry) {
        moveToTrash(entries: [entry])
    }

    func moveToTrash(entries: [BrowserEntry]) {
        moveToTrash(paths: entries.map(\.path))
    }

    func moveToTrash(path: String) {
        moveToTrash(paths: [path])
    }

    func moveToTrash(paths: [String]) {
        guard !paths.isEmpty else { return }
        var destination = currentPath
        var completed = 0
        do {
            for path in paths {
                try KitePlatform.moveToTrash(path)
                destination = pathAfterRecycle(destination, removed: path)
                completed += 1
            }
            loadRoot(destination)
        } catch {
            if completed > 0 {
                loadRoot(destination)
            }
            errorMessage = completed > 0
                ? "已处理 \(completed) 项，后续操作失败：\(error.localizedDescription)"
                : error.localizedDescription
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
                    tabs[index].columns[columnIndex].select(path: path, mode: .replace)
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
                restoredColumns[index].select(
                    path: restoredColumns[index + 1].path,
                    mode: .replace
                )
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
            svnAvailable: listing.svnAvailable,
            rule: storedColumnViewRule(for: listing.path)
        )
    }

    private func storedColumnViewRule(for path: String) -> StoredColumnViewRule {
        let key = normalizedRulePath(path)
        if let rule = storedColumnViewRules.rules[key] {
            return rule
        }
        return storedColumnViewRules.defaultRule.withoutCollapsedGroups
    }

    private func persistColumnViewRule(index: Int) {
        guard let tabIndex = activeTabIndex,
              tabs[tabIndex].columns.indices.contains(index) else {
            return
        }
        let column = tabs[tabIndex].columns[index]
        let rule = column.storedRule
        storedColumnViewRules.rules[normalizedRulePath(column.path)] = rule
        storedColumnViewRules.defaultRule = rule.withoutCollapsedGroups
        pruneStoredColumnViewRules()
        saveStoredColumnViewRules()
    }

    private func normalizedRulePath(_ path: String) -> String {
        URL(fileURLWithPath: path).standardizedFileURL.path
    }

    private func pruneStoredColumnViewRules() {
        let overflow = storedColumnViewRules.rules.count - Self.maxStoredColumnRules
        guard overflow > 0 else { return }
        for key in storedColumnViewRules.rules.keys.sorted().prefix(overflow) {
            storedColumnViewRules.rules.removeValue(forKey: key)
        }
    }

    private static func loadStoredColumnViewRules() -> StoredColumnViewRules {
        guard let data = UserDefaults.standard.data(forKey: columnViewRulesDefaultsKey),
              var rules = try? JSONDecoder().decode(StoredColumnViewRules.self, from: data) else {
            return .default
        }
        rules.defaultRule = rules.defaultRule.withoutCollapsedGroups
        return rules
    }

    private func saveStoredColumnViewRules() {
        do {
            let data = try JSONEncoder().encode(storedColumnViewRules)
            UserDefaults.standard.set(data, forKey: Self.columnViewRulesDefaultsKey)
        } catch {
            errorMessage = "无法保存分栏显示规则：\(error.localizedDescription)"
        }
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
