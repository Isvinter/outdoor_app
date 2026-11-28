import XCTest
@testable import ZeroGridTrail

/// "Tour exportieren": where the package goes, that it goes again, and what a failure says.
@MainActor
final class TourExportTests: XCTestCase {
    private var sandbox: Sandbox!
    private let suite = "TourExportTests"

    override func setUp() async throws {
        sandbox = try Sandbox()
    }

    override func tearDown() async throws {
        UserDefaults().removePersistentDomain(forName: suite)
        sandbox.remove()
        TracksController.removeLeftoverPackages()
    }

    private struct Refused: Error {}

    /// What the builder saw, from the background.
    private final class Seen<T>: @unchecked Sendable {
        var value: T?
    }

    /// The real core builds a ZIP in a folder of its own, named like the GPX file.
    func testExportWritesIntoOwnTempFolder() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        let offset = Seen<Int32>()
        let core = sandbox.host.core
        recorded.tracks.buildTour = { id, path, utcOffsetMin in
            offset.value = utcOffsetMin
            try core.exportTour(id: id, path: path, utcOffsetMin: utcOffsetMin)
        }
        await recorded.tracks.exportTour()

        let file = try XCTUnwrap(recorded.tracks.exportFile)
        let track = try XCTUnwrap(recorded.tracks.selected?.summary)
        let expected = TracksController.toursDirectory
            .appendingPathComponent(track.id, isDirectory: true)
            .appendingPathComponent(TracksController.fileName(track).replacingOccurrences(of: ".gpx", with: ".zip"))
        XCTAssertEqual(file.standardizedFileURL, expected.standardizedFileURL)
        XCTAssertTrue(file.lastPathComponent.hasSuffix(" Wandern.zip"), file.lastPathComponent)
        let start = Date(timeIntervalSince1970: Double(track.startedMs) / 1000)
        XCTAssertEqual(offset.value, Int32(TimeZone.current.secondsFromGMT(for: start) / 60))
        // A ZIP begins with "PK".
        let head = try Data(contentsOf: file).prefix(2)
        XCTAssertEqual(Array(head), [0x50, 0x4B])
        XCTAssertFalse(recorded.tracks.exporting)
        XCTAssertEqual(recorded.tracks.exportStatus, "file")
    }

    func testFinishExportRemovesFolder() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        await recorded.tracks.exportTour()
        let folder = try XCTUnwrap(recorded.tracks.exportFile).deletingLastPathComponent()
        recorded.tracks.finishExport()
        XCTAssertNil(recorded.tracks.exportFile)
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path))
        XCTAssertEqual(recorded.tracks.exportStatus, "none")
    }

    /// Closing the track while the share sheet shows takes the package along.
    func testCloseRemovesPackage() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        await recorded.tracks.exportTour()
        XCTAssertNotNil(recorded.tracks.exportFile)
        recorded.tracks.close()
        XCTAssertEqual(recorded.tracks.exportStatus, "none")
    }

    func testNoSpaceShowsMessage() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        recorded.tracks.buildTour = { _, path, _ in
            try Data("half".utf8).write(to: URL(fileURLWithPath: path))
            throw CoreError.Failed(code: "export_no_space", message: "voll")
        }
        await recorded.tracks.exportTour()
        XCTAssertEqual(recorded.tracks.message, "Nicht genug freier Speicher für das Paket.")
        XCTAssertNil(recorded.tracks.exportFile)
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: TracksController.packageURL(recorded.track).deletingLastPathComponent().path))
    }

    func testOtherErrorShowsMessage() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        recorded.tracks.buildTour = { _, _, _ in throw CoreError.Failed(code: "export_failed", message: "kaputt") }
        await recorded.tracks.exportTour()
        XCTAssertEqual(recorded.tracks.message, "Das Paket konnte nicht gebaut werden.")
        recorded.tracks.message = nil
        recorded.tracks.buildTour = { _, _, _ in throw Refused() }
        await recorded.tracks.exportTour()
        XCTAssertEqual(recorded.tracks.message, "Das Paket konnte nicht gebaut werden.")
        XCTAssertEqual(recorded.tracks.exportStatus, "none")
    }

    /// The package is built off the main thread.
    func testBuildsOffMainThread() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        let onMain = Seen<Bool>()
        recorded.tracks.buildTour = { _, _, _ in onMain.value = Thread.isMainThread }
        await recorded.tracks.exportTour()
        XCTAssertEqual(onMain.value, false)
    }
}
