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

    /// The files of a package may lie in `Import` itself instead of a folder of their own.
    func testImportOfFilesDirectlyInImportFolder() throws {
        let folder = try sandbox.stageMini()
        for file in try FileManager.default.contentsOfDirectory(atPath: folder.path) {
            try FileManager.default.moveItem(
                at: folder.appendingPathComponent(file), to: sandbox.host.importDirectory.appendingPathComponent(file))
        }
        try FileManager.default.removeItem(at: folder)

        let results = sandbox.host.importPendingRegions()

        XCTAssertEqual(results.count, 1)
        guard case .installed(let region) = try XCTUnwrap(results.first) else {
            return XCTFail("not installed: \(results)")
        }
        XCTAssertEqual(region.id, "mini")
        XCTAssertEqual(try sandbox.host.core.overview().regions.count, 1)
        // The folder is there again, empty, for the next package.
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: sandbox.host.importDirectory.path), [])
    }

    /// A package in `Import` itself must not take a rejected package beside it along.
    func testLooseImportKeepsOtherFolders() throws {
        let files = FileManager.default
        let broken = try sandbox.stageMini()
        let loose = sandbox.host.importDirectory
        for file in try files.contentsOfDirectory(atPath: broken.path) {
            try files.copyItem(at: broken.appendingPathComponent(file), to: loose.appendingPathComponent(file))
        }
        let basemap = try FileHandle(forWritingTo: broken.appendingPathComponent("basemap.pmtiles"))
        try basemap.truncate(atOffset: 1000)
        try basemap.close()

        let results = sandbox.host.importPendingRegions()

        XCTAssertEqual(results.count, 2)
        XCTAssertEqual(try sandbox.host.core.overview().regions.map(\.id), ["mini"])
        XCTAssertEqual(try files.contentsOfDirectory(atPath: loose.path), ["mini"])
        XCTAssertTrue(files.fileExists(atPath: broken.appendingPathComponent("manifest.json").path))
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
