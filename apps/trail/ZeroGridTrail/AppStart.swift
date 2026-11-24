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
        /// `-startZoom <zoom>`: the zoom the map starts at.
        var startZoom: Double?
        /// `-startCenter <lat>,<lon>`: where the map starts.
        var startCenter: [Double]?
        /// `-project <lat>,<lon>;<lat>,<lon>…`: places whose position on the
        /// screen the status reports, for tests that tap on them.
        var project: [[Double]] = []
        /// `-catalogSource <url>`: the address of the desktop's share.
        var catalogSource: String?
        /// `-replayGPX <path>`: the points of this GPX file stand in for the GPS.
        var replayGPX: String?
        /// `-route <lat>,<lon>;<lat>,<lon>`: a route to plan at the start.
        var route: [[Double]] = []

        init(arguments: [String] = ProcessInfo.processInfo.arguments) {
            guard arguments.contains("-selftest") else { return }
            selftest = true
            // Wiping the data is for the UI tests only, never for a released app.
            #if DEBUG
            resetData = arguments.contains("-resetData")
            if let flag = arguments.firstIndex(of: "-importFixture"), flag + 1 < arguments.count {
                importFixture = arguments[flag + 1]
            }
            if let flag = arguments.firstIndex(of: "-catalogSource"), flag + 1 < arguments.count {
                catalogSource = arguments[flag + 1]
            }
            if let flag = arguments.firstIndex(of: "-replayGPX"), flag + 1 < arguments.count {
                replayGPX = arguments[flag + 1]
            }
            if let flag = arguments.firstIndex(of: "-startZoom"), flag + 1 < arguments.count {
                startZoom = Double(arguments[flag + 1])
            }
            let places = { (text: String) in
                text.split(separator: ";").map { $0.split(separator: ",").compactMap { Double($0) } }
                    .filter { $0.count == 2 }
            }
            if let flag = arguments.firstIndex(of: "-startCenter"), flag + 1 < arguments.count {
                startCenter = places(arguments[flag + 1]).first
            }
            if let flag = arguments.firstIndex(of: "-project"), flag + 1 < arguments.count {
                project = places(arguments[flag + 1])
            }
            if let flag = arguments.firstIndex(of: "-route"), flag + 1 < arguments.count {
                route = places(arguments[flag + 1])
            }
            #endif
        }
    }

    /// When the app started, for the measurements of the self-test.
    static let launched = Date()

    struct Ready {
        let options: Options
        let host: CoreHost
        let engine: ValhallaMobileEngine
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
            MapController.resetLook()
            try? files.removeItem(at: data)
            try? files.removeItem(at: documents.appendingPathComponent("Import"))
        }
        let host = try CoreHost(dataDirectory: data, documentsDirectory: documents)
        let engine = ValhallaMobileEngine()
        host.core.setValhallaEngine(engine: engine)
        if let source = options.catalogSource {
            _ = try host.core.setCatalogSource(source: source)
        }
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
            options: options, host: host, engine: engine, regions: overview.regions, bounds: overview.bounds,
            failedImports: failed, importSeconds: importSeconds)
    }
}
