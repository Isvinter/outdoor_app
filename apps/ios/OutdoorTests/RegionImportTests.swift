import XCTest
@testable import Outdoor

/// A scratch directory with a core in it and `fixtures/regions/mini` ready to import.
struct Sandbox {
    let root: URL
    let host: CoreHost

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        host = try CoreHost(
            dataDirectory: root.appendingPathComponent("data", isDirectory: true),
            documentsDirectory: root.appendingPathComponent("Documents", isDirectory: true))
    }

    /// Copies the fixture to `Documents/Import/mini` and returns that folder.
    @discardableResult
    func stageMini() throws -> URL {
        let fixture = try XCTUnwrap(Bundle(for: RegionImportTests.self).url(forResource: "mini", withExtension: nil))
        try FileManager.default.createDirectory(at: host.importDirectory, withIntermediateDirectories: true)
        let folder = host.importDirectory.appendingPathComponent("mini", isDirectory: true)
        try FileManager.default.copyItem(at: fixture, to: folder)
        return folder
    }

    func remove() {
        try? FileManager.default.removeItem(at: root)
    }
}

final class RegionImportTests: XCTestCase {
    private var sandbox: Sandbox!

    override func setUpWithError() throws {
        sandbox = try Sandbox()
    }

    override func tearDown() {
        sandbox.remove()
    }

    func testImportInstallsAndRemovesFolder() throws {
        let folder = try sandbox.stageMini()

        let results = sandbox.host.importPendingRegions()

        XCTAssertEqual(results.count, 1)
        guard case .installed(let region) = try XCTUnwrap(results.first) else {
            return XCTFail("not installed: \(results)")
        }
        XCTAssertEqual(region.id, "mini")
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path))
        XCTAssertEqual(try sandbox.host.core.overview().regions.count, 1)
    }

    func testBrokenImportReportsErrorAndKeepsFolder() throws {
        let folder = try sandbox.stageMini()
        let basemap = try FileHandle(forWritingTo: folder.appendingPathComponent("basemap.pmtiles"))
        try basemap.truncate(atOffset: 1000)
        try basemap.close()

        let results = sandbox.host.importPendingRegions()

        XCTAssertEqual(results.count, 1)
        guard case .failed(let name, let code, let message) = try XCTUnwrap(results.first) else {
            return XCTFail("not failed: \(results)")
        }
        XCTAssertEqual(name, "mini")
        XCTAssertFalse(code.isEmpty)
        XCTAssertFalse(message.isEmpty)
        XCTAssertTrue(FileManager.default.fileExists(atPath: folder.appendingPathComponent("manifest.json").path))
        XCTAssertTrue(try sandbox.host.core.overview().regions.isEmpty)
    }

    func testRegionsStayOutOfTheBackup() throws {
        let regions = sandbox.root.appendingPathComponent("data/regions", isDirectory: true)
        XCTAssertEqual(try regions.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup, true)
    }
}
