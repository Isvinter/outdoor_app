import Combine
import CoreLocation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import ZeroGridTrail

/// A track recorded in the sandbox's core, opened in a tracks controller.
@MainActor
struct RecordedTrack {
    let tracks: TracksController
    let track: TrackSummary

    init(_ sandbox: Sandbox, suite: String) throws {
        let manager = FakeLocationManager()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        let recording = RecordingController(
            core: sandbox.host.core, location: location, changes: sandbox.host.recordingChanges, defaults: defaults)
        recording.start(activity: "hike")
        manager.deliver(FakeLocationManager.walk(count: 60, step: 0.00001))
        location.drain()
        recording.stop()
        track = try XCTUnwrap(recording.ended)
        tracks = TracksController(
            core: sandbox.host.core, overlay: nil, recordingChanges: Empty().eraseToAnyPublisher())
        tracks.open(track)
        XCTAssertNotNil(tracks.selected)
    }
}

@MainActor
final class LibraryImportTests: XCTestCase {
    private var sandbox: Sandbox!
    private let suite = "LibraryImportTests"

    override func setUp() async throws {
        sandbox = try Sandbox()
    }

    override func tearDown() async throws {
        UserDefaults().removePersistentDomain(forName: suite)
        sandbox.remove()
    }

    private func write(_ data: Data, _ name: String) throws -> URL {
        let url = LibraryImport.scratchFolder().appendingPathComponent(name)
        try data.write(to: url)
        return url
    }

    func testMetadataFromExif() throws {
        let taken = Date(timeIntervalSince1970: 1_795_511_100)
        let place = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 47.58, longitude: 11.56), altitude: 900,
            horizontalAccuracy: 5, verticalAccuracy: 5, timestamp: taken)
        let stamped = try XCTUnwrap(PhotoMetadata.stamp(MediaFakes.jpeg(), date: taken, location: place))
        let found = LibraryImport.metadata(of: try write(stamped, "a.jpg"))
        XCTAssertEqual(found.timeMs, 1_795_511_100_000)
        XCTAssertEqual(try XCTUnwrap(found.lat), 47.58, accuracy: 1e-6)
        XCTAssertEqual(try XCTUnwrap(found.lon), 11.56, accuracy: 1e-6)

        let screenshot = LibraryImport.metadata(of: try write(MediaFakes.png(), "b.png"))
        XCTAssertNil(screenshot.timeMs)
        XCTAssertNil(screenshot.lat)
        XCTAssertNil(screenshot.lon)
    }

    /// A HEIC stays HEIC; the copy lies in `media/` and belongs to the chosen section.
    func testImportCopiesWithOriginalExtension() throws {
        let types = CGImageDestinationCopyTypeIdentifiers() as? [String] ?? []
        try XCTSkipUnless(types.contains(UTType.heic.identifier), "no HEIC encoder here")
        let image = try XCTUnwrap(UIImage(data: MediaFakes.jpeg())?.cgImage)
        let heic = LibraryImport.scratchFolder().appendingPathComponent("IMG_0001.HEIC")
        let destination = try XCTUnwrap(
            CGImageDestinationCreateWithURL(heic as CFURL, UTType.heic.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(destination, image, nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))

        let recorded = try RecordedTrack(sandbox, suite: suite)
        recorded.tracks.insertImages(track: recorded.track.id, section: 0, files: [heic])
        XCTAssertNil(recorded.tracks.message)
        let item = try XCTUnwrap(recorded.tracks.media(for: 0).first)
        XCTAssertEqual(item.kind, "photo")
        XCTAssertEqual(item.source, "library")
        XCTAssertTrue(try XCTUnwrap(item.file).hasSuffix(".heic"))
        let copy = try XCTUnwrap(recorded.tracks.fileURL(item))
        XCTAssertTrue(FileManager.default.fileExists(atPath: copy.path))
        XCTAssertEqual(copy.deletingLastPathComponent().lastPathComponent, "media")
        XCTAssertFalse(
            FileManager.default.fileExists(atPath: heic.deletingLastPathComponent().path), "the scratch copy is gone")
        // Without a time of its own: the start of the section.
        XCTAssertEqual(item.timeMs, recorded.tracks.selected?.sections.first?.startMs)
        XCTAssertNil(item.lat)
    }

    /// A picture of the whole tour, and a note to it.
    func testWholeTourImageAndNote() throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        let tracks = recorded.tracks
        tracks.insertImages(track: recorded.track.id, section: nil, files: [try write(MediaFakes.jpeg(), "c.jpg")])
        tracks.addNote(track: recorded.track.id, section: nil, text: " Schöne Runde ")
        let items = tracks.media(for: nil)
        XCTAssertEqual(items.map(\.kind).sorted(), ["photo", "text"])
        XCTAssertEqual(items.first { $0.kind == "text" }?.text, "Schöne Runde")
        XCTAssertTrue(tracks.media(for: 0).isEmpty)
    }

    /// Deleting a photo deletes Trail's copy; emptying a note asks to delete it.
    func testDeleteAndEmptyNote() throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        let tracks = recorded.tracks
        tracks.insertImages(track: recorded.track.id, section: 0, files: [try write(MediaFakes.jpeg(), "d.jpg")])
        let photo = try XCTUnwrap(tracks.media(for: 0).first)
        let file = try XCTUnwrap(tracks.fileURL(photo))
        tracks.delete(id: photo.id)
        XCTAssertTrue(tracks.media(for: 0).isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.path))

        tracks.addNote(track: recorded.track.id, section: 0, text: "alt")
        let note = try XCTUnwrap(tracks.media(for: 0).first)
        tracks.editText(id: note.id, text: "neu")
        XCTAssertEqual(tracks.media(for: 0).first?.text, "neu")
        tracks.editText(id: note.id, text: "  ")
        XCTAssertEqual(tracks.confirmDelete?.id, note.id)
        XCTAssertNil(tracks.message)
    }
}
