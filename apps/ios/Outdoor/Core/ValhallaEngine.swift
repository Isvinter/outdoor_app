import Foundation
import Valhalla
import ValhallaConfigModels

/// Valhalla in the app, as the router of the core: the core builds the request
/// and reads the answer, this only passes the JSON text to valhalla-mobile.
///
/// One Valhalla instance per `routing.tar`, opened at the first request (about
/// 100 ms) and kept. An instance is never asked from two threads at once.
final class ValhallaMobileEngine: ValhallaEngine, @unchecked Sendable {
    /// The Valhalla inside valhalla-mobile 0.6.4 (`project.yml`). The core
    /// compares it with the version the routing data of a region was built with.
    static let valhallaVersion = "3.9.1"

    /// The instance for one `routing.tar` and the lock that serialises its use.
    private final class Region {
        let lock = NSLock()
        /// Each instance needs a configuration file of its own.
        let configName: String
        var valhalla: Valhalla?

        init(configName: String) {
            self.configName = configName
        }
    }

    /// Guards `regions`, `configs` and `opens`.
    private let lock = NSLock()
    private var regions: [String: Region] = [:]
    /// Counts the configuration files, so that no two instances share one.
    private var configs = 0
    private var opens = 0

    /// How many Valhalla instances were opened so far; for the tests.
    var opened: Int {
        lock.withLock { opens }
    }

    /// Lets go of the instance for `tilesPath`: the file was replaced or
    /// deleted, and the next request has to open what lies there now. A
    /// request that is running ends on the old instance. The core calls this
    /// before it opens the router of a region (anew); the app calls it after
    /// an update and a deletion, which also closes the old file.
    func forget(tilesPath: String) {
        lock.withLock { _ = regions.removeValue(forKey: tilesPath) }
    }

    func version() -> String {
        Self.valhallaVersion
    }

    func route(tilesPath: String, requestJson: String) throws -> String {
        let region = self.region(for: tilesPath)
        region.lock.lock()
        defer { region.lock.unlock() }
        do {
            let valhalla: Valhalla
            if let open = region.valhalla {
                valhalla = open
            } else {
                valhalla = try open(tilesPath, configName: region.configName)
                region.valhalla = valhalla
                lock.withLock { opens += 1 }
            }
            return try valhalla.route(rawRequest: requestJson)
        } catch ValhallaError.valhallaError(_, let message) {
            // Valhalla's own words: the core tells "no route" from other failures by them.
            throw EngineFailure.Failed(message: message)
        } catch let failure as EngineFailure {
            throw failure
        } catch {
            throw EngineFailure.Failed(message: error.localizedDescription)
        }
    }

    private func region(for tilesPath: String) -> Region {
        lock.lock()
        defer { lock.unlock() }
        if let region = regions[tilesPath] { return region }
        let region = Region(configName: "valhalla-\(configs).json")
        configs += 1
        regions[tilesPath] = region
        return region
    }

    private func open(_ tilesPath: String, configName: String) throws -> Valhalla {
        // Valhalla would open without the file and then find no way anywhere.
        guard FileManager.default.fileExists(atPath: tilesPath) else {
            throw EngineFailure.Failed(message: "routing tiles are missing: \(tilesPath)")
        }
        var config = try ValhallaConfig(tileExtractTar: URL(fileURLWithPath: tilesPath))
        // Valhalla still writes a line per request to the console: valhalla-mobile
        // never hands the `logging` section of the configuration to Valhalla.
        // The default names a traffic archive that does not exist, and Valhalla warns about it.
        config.mjolnir?.trafficExtract = nil
        return try Valhalla(config, configName: configName)
    }
}
