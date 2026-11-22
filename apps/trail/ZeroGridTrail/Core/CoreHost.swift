import Foundation
import os

/// What became of one folder in `Documents/Import`.
enum ImportResult {
    case installed(RegionSummary)
    case failed(folder: String, code: String, message: String)
}

/// Background failures of the core end up in the log; the progress of a
/// download goes to whoever watches it.
private final class Listener: CoreListener, @unchecked Sendable {
    typealias Progress = @Sendable (_ region: String, _ done: UInt64, _ total: UInt64) -> Void

    private let log = Logger(subsystem: "de.zeibig.zerogrid.trail", category: "core")
    private let lock = NSLock()
    private var watcher: Progress?

    var progress: Progress? {
        get { lock.withLock { watcher } }
        set { lock.withLock { watcher = newValue } }
    }

    func onError(message: String) {
        log.error("\(message, privacy: .public)")
    }

    /// Called on the thread that runs `installRegion`.
    func onRegionProgress(region: String, done: UInt64, total: UInt64) {
        progress?(region, done, total)
    }
}

/// The Rust core and the directories it works in. One instance per app start;
/// as thread-safe as `CoreApp` itself.
final class CoreHost {
    let core: CoreApp
    /// Region packages put here (Finder or the Files app) are installed at the next start.
    let importDirectory: URL
    /// Where the core keeps the installed regions, one folder each.
    let regionsDirectory: URL
    private let listener = Listener()

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

        regionsDirectory = regions
        importDirectory = documentsDirectory.appendingPathComponent("Import", isDirectory: true)
        core = try CoreApp.open(dataDir: dataDirectory.path, listener: listener)
    }

    /// Who is told about the bytes of a running download, in place of whoever
    /// was before. Called on the downloading thread, at most every 150 ms.
    func watchRegionProgress(_ watcher: (@Sendable (_ region: String, _ done: UInt64, _ total: UInt64) -> Void)?) {
        listener.progress = watcher
    }

    /// The routing data of a region, as the core names it to the engine
    /// (`<regions>/<id>/routing.tar`).
    func routingTiles(region id: String) -> String {
        regionsDirectory.appendingPathComponent(id, isDirectory: true).appendingPathComponent("routing.tar").path
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
