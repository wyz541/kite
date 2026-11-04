import Darwin
import Foundation

@MainActor
final class DirectoryWatcher {
    private let onChange: @MainActor () -> Void
    private var sources: [String: DispatchSourceFileSystemObject] = [:]
    private var pendingNotification: DispatchWorkItem?

    init(onChange: @escaping @MainActor () -> Void) {
        self.onChange = onChange
    }

    func update(paths: [String]) {
        let desired = Set(paths.map(Self.normalizedPath))
        let stalePaths = sources.keys.filter { !desired.contains($0) }
        for path in stalePaths {
            removeSource(for: path)
        }
        for path in desired where sources[path] == nil {
            addSource(for: path)
        }
    }

    private func addSource(for path: String) {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            return
        }

        let descriptor = open(path, O_EVTONLY)
        guard descriptor >= 0 else {
            return
        }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: descriptor,
            eventMask: [.write, .delete, .rename, .attrib, .extend],
            queue: .main
        )
        source.setEventHandler { [weak self] in
            self?.handleEvent(for: path)
        }
        source.setCancelHandler {
            close(descriptor)
        }
        sources[path] = source
        source.resume()
    }

    private func removeSource(for path: String) {
        sources.removeValue(forKey: path)?.cancel()
    }

    private func handleEvent(for path: String) {
        // A descriptor can stop reporting after rename or deletion. The model
        // recreates all directory watchers after the debounced refresh.
        removeSource(for: path)
        pendingNotification?.cancel()
        let notification = DispatchWorkItem { [weak self] in
            self?.pendingNotification = nil
            self?.onChange()
        }
        pendingNotification = notification
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.16, execute: notification)
    }

    private static func normalizedPath(_ path: String) -> String {
        URL(fileURLWithPath: path).standardizedFileURL.path
    }

    deinit {
        for source in sources.values {
            source.cancel()
        }
    }
}
