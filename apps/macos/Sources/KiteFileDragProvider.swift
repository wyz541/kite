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

        // Expose the file URL as the first and primary representation. Providers
        // created with contentsOf also advertise public.plain-text first, which
        // makes web upload targets treat the drag as text instead of a file.
        let provider = NSItemProvider()
        provider.suggestedName = primaryURL.lastPathComponent
        provider.registerItem(forTypeIdentifier: UTType.fileURL.identifier) {
            completion, _, _ in
            completion?(primaryURL as NSURL, nil)
        }
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
