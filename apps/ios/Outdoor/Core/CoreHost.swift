import Foundation
import os

/// What became of one folder in `Documents/Import`.
enum ImportResult {
    case installed(RegionSummary)
    case failed(folder: String, code: String, message: String)
}

/// Background failures of the core end up in the log.
private final class LogListener: CoreListener {
    private let log = Logger(subsystem: "de.zeibig.outdoor", category: "core")

    func onError(message: String) {
        log.error("\(message, privacy: .public)")
    }
}

/// The Rust core and the directories it works in. One instance per app start;
/// as thread-safe as `CoreApp` itself.
final class CoreHost {
    let core: CoreApp
    /// Region packages put here (Finder or the Files app) are installed at the next start.
    let importDirectory: URL

    /// `Application Support/outdoor`; `init` creates it. The regions below it
    /// can be fetched again and stay out of the backup.
    static func dataDirectory() throws -> URL {
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return support.appendingPathComponent("outdoor", isDirectory: true)
    }

    /// A package whose files were put into `Import` itself gets a folder of its
    /// own, so that installing it removes nothing else that lies in `Import`.
    private func gatherLoosePackage() {
        let files = FileManager.default
        guard files.fileExists(atPath: importDirectory.appendingPathComponent("manifest.json").path),
              let entries = try? files.contentsOfDirectory(
                  at: importDirectory, includingPropertiesForKeys: [.isDirectoryKey], options: .skipsHiddenFiles)
        else { return }
        var folder = importDirectory.appendingPathComponent("Paket", isDirectory: true)
        var number = 2
        while files.fileExists(atPath: folder.path) {
            folder = importDirectory.appendingPathComponent("Paket \(number)", isDirectory: true)
            number += 1
        }
        guard (try? files.createDirectory(at: folder, withIntermediateDirectories: false)) != nil else { return }
        for entry in entries where (try? entry.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) != true {
            try? files.moveItem(at: entry, to: folder.appendingPathComponent(entry.lastPathComponent))
        }
    }

    static func documentsDirectory() throws -> URL {
        try FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
    }

    convenience init(dataDirectory: URL) throws {
        try self.init(dataDirectory: dataDirectory, documentsDirectory: Self.documentsDirectory())
    }

    init(dataDirectory: URL, documentsDirectory: URL) throws {
        var regions = dataDirectory.appendingPathComponent("regions", isDirectory: true)
        try FileManager.default.createDirectory(at: regions, withIntermediateDirectories: true)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try regions.setResourceValues(values)

        importDirectory = documentsDirectory.appendingPathComponent("Import", isDirectory: true)
        core = try CoreApp.open(dataDir: dataDirectory.path, listener: LogListener())
    }

    /// Installs every folder in `Documents/Import` that holds a `manifest.json`.
    /// The core verifies and moves the files and removes the folder; a package
    /// it rejects stays where it is, to be fixed.
    func importPendingRegions() -> [ImportResult] {
        let files = FileManager.default
        // Also makes the folder show up in the Files app before the first import.
        try? files.createDirectory(at: importDirectory, withIntermediateDirectories: true)
        gatherLoosePackage()
        let folders = ((try? files.contentsOfDirectory(
            at: importDirectory, includingPropertiesForKeys: nil, options: .skipsHiddenFiles)) ?? [])
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
        return folders
            .filter { files.fileExists(atPath: $0.appendingPathComponent("manifest.json").path) }
            .map { folder in
                do {
                    return .installed(try core.installLocalRegion(packageDir: folder.path))
                } catch let CoreError.Failed(code, message) {
                    return .failed(folder: folder.lastPathComponent, code: code, message: message)
                } catch {
                    return .failed(folder: folder.lastPathComponent, code: "unknown", message: describe(error))
                }
            }
    }
}

/// What to show for a failure: the core's own message instead of the Swift
/// description of its error type.
func describe(_ error: Error) -> String {
    if case CoreError.Failed(_, let message) = error { return message }
    return error.localizedDescription
}
