import XCTest
@testable import Outdoor

/// Routing through the core with valhalla-mobile. The tests with real routing
/// data need `ISARWINKEL_PACKAGE`, the path of a package of the region
/// `isarwinkel-achensee` on the Mac (`scripts/test.sh` sets it if it can).
final class ValhallaEngineTests: XCTestCase {
    private var sandbox: Sandbox!

    override func setUpWithError() throws {
        sandbox = try Sandbox()
    }

    override func tearDown() {
        sandbox.remove()
    }

    /// Installs the package named by `ISARWINKEL_PACKAGE` and routes with valhalla-mobile.
    private func coreWithIsarwinkel() throws -> CoreApp {
        guard let path = ProcessInfo.processInfo.environment["ISARWINKEL_PACKAGE"], !path.isEmpty else {
            throw XCTSkip("ISARWINKEL_PACKAGE is not set")
        }
        let files = FileManager.default
        let host = sandbox.host
        try files.createDirectory(at: host.importDirectory, withIntermediateDirectories: true)
        try files.copyItem(
            at: URL(fileURLWithPath: path), to: host.importDirectory.appendingPathComponent("isarwinkel-achensee"))
        let results = host.importPendingRegions()
        guard case .installed = try XCTUnwrap(results.first) else {
            XCTFail("not installed: \(results)")
            throw XCTSkip("the package could not be installed")
        }
        host.core.setValhallaEngine(engine: ValhallaMobileEngine())
        return host.core
    }

    /// The reference hike of the spike: three points, 12 353 m on the desktop.
    static func referenceHike() -> PlanInput {
        let leg = LegInput(
            profile: "hike",
            options: [OptionValue(key: "use_hills", value: 0.8), OptionValue(key: "max_hiking_difficulty", value: 3)])
        let points = [
            LatLon(lat: 47.6803, lon: 11.5741), LatLon(lat: 47.6625, lon: 11.5233), LatLon(lat: 47.645, lon: 11.556),
        ]
        return points.reduce(planEmpty()) { planAddWaypoint(request: $0, at: $1, leg: leg) }
    }

    func testRoutesReferenceHike() throws {
        let core = try coreWithIsarwinkel()

        let plan = try core.planRoute(request: Self.referenceHike())

        XCTAssertEqual(plan.total.failedLegs, 0, "\(plan.legs.map(\.errorMessage))")
        XCTAssertEqual(plan.total.distanceM, 12353, accuracy: 124)
        XCTAssertEqual(plan.legs.count, 2)
        XCTAssertGreaterThan(plan.legs[0].line.count, 100)
    }

    func testConcurrentRoutesAreSerialised() throws {
        let core = try coreWithIsarwinkel()
        let request = Self.referenceHike()
        let lock = NSLock()
        var distances: [Double] = []
        var failures: [String] = []

        DispatchQueue.concurrentPerform(iterations: 8) { _ in
            let result = Result { try core.planRoute(request: request) }
            lock.lock()
            defer { lock.unlock() }
            switch result {
            case .success(let plan) where plan.total.failedLegs == 0: distances.append(plan.total.distanceM)
            case .success(let plan): failures.append("\(plan.legs.map(\.errorMessage))")
            case .failure(let error): failures.append(describe(error))
            }
        }

        XCTAssertEqual(failures, [])
        XCTAssertEqual(distances.count, 8)
        XCTAssertEqual(Set(distances).count, 1, "\(distances)")
    }

    /// The engine itself, asked from many threads at once for the same tiles.
    func testEngineSerialisesOneInstance() throws {
        guard let package = ProcessInfo.processInfo.environment["ISARWINKEL_PACKAGE"], !package.isEmpty else {
            throw XCTSkip("ISARWINKEL_PACKAGE is not set")
        }
        let tiles = URL(fileURLWithPath: package).appendingPathComponent("routing.tar").path
        let request = #"{"costing":"pedestrian","directions_type":"none","locations":"#
            + #"[{"lat":47.6803,"lon":11.5741},{"lat":47.6625,"lon":11.5233}],"units":"kilometers"}"#
        let engine = ValhallaMobileEngine()
        let lock = NSLock()
        var answers: [String] = []

        DispatchQueue.concurrentPerform(iterations: 16) { _ in
            let answer = (try? engine.route(tilesPath: tiles, requestJson: request)) ?? "failed"
            lock.lock()
            answers.append(answer)
            lock.unlock()
        }

        XCTAssertEqual(Set(answers).count, 1)
        XCTAssertTrue(try XCTUnwrap(answers.first).contains("\"trip\""))
    }

    func testEngineFailureIsReported() {
        let engine = ValhallaMobileEngine()
        let missing = sandbox.root.appendingPathComponent("nothing/routing.tar").path

        XCTAssertThrowsError(try engine.route(tilesPath: missing, requestJson: "{}")) { error in
            guard case EngineFailure.Failed(let message) = error else { return XCTFail("\(error)") }
            XCTAssertTrue(message.contains("routing tiles are missing"), message)
        }
    }

    /// Routing data that vanished after the import: the leg fails, the app goes on.
    func testMissingTilesFailTheLeg() throws {
        let core = try coreWithIsarwinkel()
        let regions = sandbox.root.appendingPathComponent("data/regions", isDirectory: true)
        let found = FileManager.default.enumerator(at: regions, includingPropertiesForKeys: nil)?
            .compactMap { $0 as? URL }.first { $0.lastPathComponent == "routing.tar" }
        try FileManager.default.removeItem(at: try XCTUnwrap(found))

        let plan = try core.planRoute(request: Self.referenceHike())

        XCTAssertEqual(plan.total.failedLegs, 2)
        XCTAssertNotNil(plan.legs[0].errorCode)
        XCTAssertTrue(plan.legs[0].line.isEmpty)
    }

    func testVersionMatchesThePackage() {
        XCTAssertEqual(ValhallaMobileEngine().version(), "3.9.1")
    }
}
