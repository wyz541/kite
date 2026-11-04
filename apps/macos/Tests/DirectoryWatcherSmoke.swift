import Foundation

@main
struct DirectoryWatcherSmoke {
    @MainActor
    static func main() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("kite-directory-watcher-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        var changed = false
        let watcher = DirectoryWatcher {
            changed = true
        }
        watcher.update(paths: [root.path])
        try Data("created".utf8).write(to: root.appendingPathComponent("created.txt"))

        let deadline = Date().addingTimeInterval(5)
        while !changed && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        watcher.update(paths: [])

        guard changed else {
            fputs("DirectoryWatcherSmoke failed: no change notification\n", stderr)
            exit(1)
        }
        print("DirectoryWatcherSmoke passed")
    }
}
