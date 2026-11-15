import Foundation

/// What the app does before the map appears: open the core, install the
/// regions waiting in `Documents/Import`, hook the tiles into the map.
enum AppStart {
    /// Start arguments of the self-test (`-selftest`); without it the others are ignored.
    struct Options {
        var selftest = false
        /// `-resetData`: start without regions and database.
        var resetData = false
        /// `-importFixture <folder>`: a region package to copy to `Documents/Import` first.
        var importFixture: String?

        init(arguments: [String] = ProcessInfo.processInfo.arguments) {
            guard arguments.contains("-selftest") else { return }
            selftest = true
            resetData = arguments.contains("-resetData")
            if let flag = arguments.firstIndex(of: "-importFixture"), flag + 1 < arguments.count {
                importFixture = arguments[flag + 1]
            }
        }
    }

    /// When the app started, for the measurements of the self-test.
    static let launched = Date()

    struct Ready {
        let options: Options
        let host: CoreHost
        let regions: [RegionSummary]
        let bounds: [Double]?
        /// One line per package that could not be installed.
        let failedImports: [String]
        /// How long checking and moving the regions to import took.
        let importSeconds: TimeInterval
    }

    /// Blocking: checks and moves the files of the regions to import.
    static func run(options: Options = Options()) throws -> Ready {
        let files = FileManager.default
        let data = try CoreHost.dataDirectory()
        let documents = try CoreHost.documentsDirectory()
        if options.resetData {
            try? files.removeItem(at: data)
            try? files.removeItem(at: documents.appendingPathComponent("Import"))
        }
        let host = try CoreHost(dataDirectory: data, documentsDirectory: documents)
        if let fixture = options.importFixture {
            let source = URL(fileURLWithPath: fixture)
            try files.createDirectory(at: host.importDirectory, withIntermediateDirectories: true)
            try files.copyItem(at: source, to: host.importDirectory.appendingPathComponent(source.lastPathComponent))
        }

        let importStarted = Date()
        let failed = host.importPendingRegions().compactMap { result -> String? in
            guard case .failed(let folder, _, let message) = result else { return nil }
            return "Import von „\(folder)“ fehlgeschlagen: \(message)"
        }
        let importSeconds = Date().timeIntervalSince(importStarted)
        let overview = try host.core.overview()
        TileURLProtocol.install(core: host.core)
        return Ready(
            options: options, host: host, regions: overview.regions, bounds: overview.bounds, failedImports: failed,
            importSeconds: importSeconds)
    }
}
