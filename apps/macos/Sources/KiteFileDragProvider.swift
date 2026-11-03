import AppKit
import Foundation
import UniformTypeIdentifiers

enum KiteFileDragProvider {
    static let contentType = UTType(exportedAs: "com.wyz541.kite.file-items")

    private struct FileDragPayload: Codable {
        let paths: [String]
    }

    static func makeProvider(paths: [String], primaryPath: String) -> NSItemProvider {
        var seen = Set<String>()
        let uniquePaths = paths.filter { seen.insert($0).inserted }
        let primaryURL = URL(fileURLWithPath: primaryPath)

        // Register the real file representation first. Otherwise Finder can
        // materialize the internal JSON payload as an unusable file with the real name.
        let provider = NSItemProvider(contentsOf: primaryURL) ?? NSItemProvider()
        provider.suggestedName = primaryURL.lastPathComponent
        provider.registerDataRepresentation(
            forTypeIdentifier: contentType.identifier,
            visibility: .all
        ) { completion in
            do {
                let data = try JSONEncoder().encode(FileDragPayload(paths: uniquePaths))
                completion(data, nil)
            } catch {
                completion(nil, error)
            }
            return nil
        }
        return provider
    }

    static func loadInternalPaths(
        from provider: NSItemProvider,
        completion: @escaping ([String]?) -> Void
    ) {
        provider.loadDataRepresentation(
            forTypeIdentifier: contentType.identifier
        ) { data, _ in
            guard let data else {
                completion(nil)
                return
            }
            let payload = try? JSONDecoder().decode(FileDragPayload.self, from: data)
            completion(payload?.paths)
        }
    }
}
