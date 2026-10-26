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
    var title: String { URL(fileURLWithPath: path).lastPathComponent }
}

@MainActor
final class KiteModel: ObservableObject {
    private let core: KiteCore

    @Published var columns: [BrowserColumn] = []
    @Published var address: String
    @Published var errorMessage: String?
    @Published var focusedAddressRequest = 0

    init(core: KiteCore = KiteCore()) {
        self.core = core
        self.address = core.defaultDirectory()
        loadRoot(address)
    }

    var currentPath: String {
        columns.last?.path ?? address
    }

    func loadRoot(_ path: String) {
        do {
            let listing = try core.listDirectory(path: path)
            address = listing.path
            columns = [column(from: listing)]
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func select(column index: Int, entry: BrowserEntry) {
        guard columns.indices.contains(index) else { return }
        columns[index].selectedPath = entry.path
        if columns.count > index + 1 {
            columns.removeSubrange((index + 1)..<columns.count)
        }
        guard entry.isDirectory else { return }
        do {
            let listing = try core.listDirectory(path: entry.path)
            columns.append(column(from: listing))
            address = listing.path
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

    private func column(from listing: DirectoryListingDto) -> BrowserColumn {
        BrowserColumn(
            path: listing.path,
            entries: listing.entries.map(BrowserEntry.init(dto:)),
            selectedPath: nil
        )
    }
}
