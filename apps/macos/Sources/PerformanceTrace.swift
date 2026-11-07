import Foundation

@MainActor
enum PerformanceTrace {
    private static let tracePath = ProcessInfo.processInfo.environment["KITE_PERF_TRACE_PATH"]
    private static let startedUptime = ProcessInfo.processInfo.systemUptime
    private static var emittedEvents: Set<String> = []

    static func mark(_ event: String) {
        write(event: event)
    }

    static func markOnce(_ event: String) {
        guard emittedEvents.insert(event).inserted else { return }
        write(event: event)
    }

    private static func write(event: String) {
        guard let tracePath, !tracePath.isEmpty else { return }
        let now = Date()
        let payload: [String: Any] = [
            "schema_version": 1,
            "platform": "macos",
            "version": Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "unknown",
            "pid": ProcessInfo.processInfo.processIdentifier,
            "event": event,
            "timestamp_unix_ms": Int64(now.timeIntervalSince1970 * 1_000),
            "elapsed_ms": max(0, Int64((ProcessInfo.processInfo.systemUptime - startedUptime) * 1_000)),
        ]
        do {
            let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
            let url = URL(fileURLWithPath: tracePath)
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            if !FileManager.default.fileExists(atPath: tracePath) {
                FileManager.default.createFile(atPath: tracePath, contents: nil)
            }
            let handle = try FileHandle(forWritingTo: url)
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
            try handle.write(contentsOf: Data([0x0A]))
        } catch {
            fputs("Kite performance trace unavailable: \(error)\n", stderr)
        }
    }
}
