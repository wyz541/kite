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
    @Published private(set) var activeTabID: UUID?
    @Published var errorMessage: String?
    @Published var focusedAddressRequest = 0

    init(core: KiteCore = KiteCore()) {
        self.core = core
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

    func moveToTrash(_ entry: BrowserEntry) {
        do {
            try KitePlatform.moveToTrash(entry.path)
            refresh()
        } catch {
            errorMessage = error.localizedDescription
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
}