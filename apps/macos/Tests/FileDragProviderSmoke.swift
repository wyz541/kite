import AppKit
import Foundation
import UniformTypeIdentifiers

@main
enum FileDragProviderSmoke {
    static func main() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("kite-file-drag-smoke-\(UUID().uuidString)")
        let file = root.appendingPathComponent("sample.txt")
        let folder = root.appendingPathComponent("Sample Folder", isDirectory: true)
        let nested = folder.appendingPathComponent("nested.txt")

        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("complete file\n".utf8).write(to: file)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data("complete folder\n".utf8).write(to: nested)

        try await verify(url: file)
        try await verify(url: folder)
        print("FileDragProviderSmoke passed")
    }

    private static func verify(url: URL) async throws {
        let provider = KiteFileDragProvider.makeProvider(
            paths: [url.path],
            primaryPath: url.path
        )
        let types = provider.registeredTypeIdentifiers
        let fileTypeIndex = types.firstIndex(of: UTType.fileURL.identifier)
        let internalTypeIndex = types.firstIndex(of: KiteFileDragProvider.contentType.identifier)

        guard types.first == UTType.fileURL.identifier,
              let fileTypeIndex,
              let internalTypeIndex,
              fileTypeIndex < internalTypeIndex else {
            throw SmokeError("file URL must be the primary drag type: \(types)")
        }

        let loadedItem = try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<NSSecureCoding, Error>) in
            provider.loadItem(
                forTypeIdentifier: UTType.fileURL.identifier,
                options: nil
            ) { item, error in
                if let error {
                    continuation.resume(throwing: error)
                } else if let item {
                    continuation.resume(returning: item)
                } else {
                    continuation.resume(throwing: SmokeError("file URL item was missing"))
                }
            }
        }
        guard let loadedURL = dragURL(from: loadedItem),
              loadedURL.standardizedFileURL == url.standardizedFileURL else {
            throw SmokeError("file URL representation did not point to the original item")
        }
        try verifyContents(of: loadedURL)

        let paths = await withCheckedContinuation { continuation in
            KiteFileDragProvider.loadInternalPaths(from: provider) {
                continuation.resume(returning: $0)
            }
        }
        guard paths == [url.path] else {
            throw SmokeError("internal drag payload did not preserve its paths")
        }
    }

    private static func dragURL(from item: NSSecureCoding) -> URL? {
        if let url = item as? URL {
            return url
        }
        if let url = item as? NSURL {
            return url as URL
        }
        if let data = item as? Data {
            return URL(dataRepresentation: data, relativeTo: nil)
        }
        return nil
    }

    private static func verifyContents(of url: URL) throws {
        if url.hasDirectoryPath {
            let nested = url.appendingPathComponent("nested.txt")
            guard try Data(contentsOf: nested) == Data("complete folder\n".utf8) else {
                throw SmokeError("folder contents were incomplete")
            }
        } else {
            guard try Data(contentsOf: url) == Data("complete file\n".utf8) else {
                throw SmokeError("file contents were incomplete")
            }
        }
    }

}

private struct SmokeError: Error, CustomStringConvertible {
    let description: String

    init(_ description: String) {
        self.description = description
    }
}
