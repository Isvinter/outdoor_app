import CoreLocation
import ImageIO
import XCTest
@testable import ZeroGridTrail

/// Stands in for the core: notes what it was told and whether the file was there by then.
@MainActor
private final class FakeMediaCore: MediaCore {
    struct Call {
        let kind: String
        let source: String
        let file: String?
        let text: String?
        let fileExisted: Bool
    }

    let dataDirectory: URL
    var calls: [Call] = []
    var failing = false

    struct Refused: Error {}

    init(dataDirectory: URL) throws {
        self.dataDirectory = dataDirectory
        try FileManager.default.createDirectory(
            at: dataDirectory.appendingPathComponent("media"), withIntermediateDirectories: true)
    }

    func mediaDir() -> String { dataDirectory.appendingPathComponent("media").path }

    func addRecordingMedia(kind: String, source: String, file: String?, text: String?) throws -> MediaItem {
        let existed = file.map { FileManager.default.fileExists(atPath: dataDirectory.appendingPathComponent($0).path) }
        calls.append(Call(kind: kind, source: source, file: file, text: text, fileExisted: existed ?? false))
        if failing { throw Refused() }
        return MediaItem(
            id: UUID().uuidString, section: 0, kind: kind, source: source, timeMs: nil, lat: nil, lon: nil, file: file,
            text: text, transcribed: false, updatedMs: 0)
    }
}

@MainActor
final class MediaControllerTests: XCTestCase {
    private var root: URL!
    private var core: FakeMediaCore!
    private var library: FakeLibrary!
    private var audio: FakeAudio!
    private var defaults: UserDefaults!
    private var recording = true
    private let suite = "MediaControllerTests"
    private let fix = CLLocation(
        coordinate: CLLocationCoordinate2D(latitude: 47.5753, longitude: 11.5521), altitude: 812,
        horizontalAccuracy: 5, verticalAccuracy: 5, timestamp: Date())

    override func setUp() async throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        core = try FakeMediaCore(dataDirectory: root)
        library = FakeLibrary()
        audio = FakeAudio()
        UserDefaults().removePersistentDomain(forName: suite)
        defaults = UserDefaults(suiteName: suite)
        recording = true
    }

    override func tearDown() async throws {
        UserDefaults().removePersistentDomain(forName: suite)
        try? FileManager.default.removeItem(at: root)
    }

    private func controller() -> MediaController {
        MediaController(
            core: core, camera: FakeCamera(), audio: audio, library: library, picker: FakeImagePicker(),
            permissions: AllowingPermissions(),
            defaults: defaults, lastFix: { [fix] in fix }, isRecording: { [unowned self] in recording })
    }

    private var mediaFiles: [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: core.mediaDir())) ?? []
    }

    /// The file lies in `media/` before the core learns of it.
    func testPhotoIsWrittenThenRegistered() async throws {
        let media = controller()
        await media.capturePhoto(MediaFakes.jpeg())
        let call = try XCTUnwrap(core.calls.first)
        XCTAssertEqual(core.calls.count, 1)
        XCTAssertEqual(call.kind, "photo")
        XCTAssertEqual(call.source, "camera")
        XCTAssertTrue(call.fileExisted)
        let file = try XCTUnwrap(call.file)
        XCTAssertNotNil(
            file.range(of: "^media/[0-9a-f-]{36}\\.jpg$", options: .regularExpression), file)
        XCTAssertNil(media.message)
    }

    /// The JPEG says when and where it was taken: now, at the last fix.
    func testPhotoCarriesExifTimeAndPlace() async throws {
        let media = controller()
        let before = Date()
        await media.capturePhoto(MediaFakes.jpeg())
        let file = try XCTUnwrap(core.calls.first?.file)
        let url = root.appendingPathComponent(file)
        let source = try XCTUnwrap(CGImageSourceCreateWithURL(url as CFURL, nil))
        let properties = try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
        let exif = try XCTUnwrap(properties[kCGImagePropertyExifDictionary] as? [CFString: Any])
        XCTAssertNotNil(exif[kCGImagePropertyExifDateTimeOriginal])

        let read = PhotoMetadata.read(url)
        let time = try XCTUnwrap(read.timeMs)
        // EXIF holds whole seconds.
        XCTAssertEqual(Double(time) / 1000, before.timeIntervalSince1970, accuracy: 2)
        XCTAssertEqual(try XCTUnwrap(read.lat), 47.5753, accuracy: 1e-6)
        XCTAssertEqual(try XCTUnwrap(read.lon), 11.5521, accuracy: 1e-6)
    }

    func testLibraryCopyFollowsSetting() async {
        let media = controller()
        XCTAssertTrue(media.saveToLibrary, "on by default")
        await media.capturePhoto(MediaFakes.jpeg())
        XCTAssertEqual(library.saved.count, 1)
        XCTAssertEqual(media.librarySaved, 1)

        media.saveToLibrary = false
        await media.capturePhoto(MediaFakes.jpeg())
        XCTAssertEqual(library.saved.count, 1)
        XCTAssertEqual(core.calls.count, 2)
    }

    /// Without a recording the camera only fills the library.
    func testPhotoWithoutRecordingGoesOnlyToLibrary() async {
        recording = false
        let media = controller()
        media.saveToLibrary = false
        await media.capturePhoto(MediaFakes.jpeg())
        XCTAssertTrue(core.calls.isEmpty)
        XCTAssertTrue(mediaFiles.isEmpty)
        XCTAssertEqual(library.saved.count, 1)
    }

    /// The core refuses: the file stays, and the failure is told.
    func testFailedRegistrationKeepsFile() async {
        core.failing = true
        let media = controller()
        await media.capturePhoto(MediaFakes.jpeg())
        XCTAssertEqual(mediaFiles.count, 1)
        XCTAssertNotNil(media.message)
    }

    func testEmptyTextIsNotSaved() throws {
        let media = controller()
        media.openNote()
        media.addText("  \n ")
        XCTAssertTrue(core.calls.isEmpty)
        XCTAssertNil(media.note)

        media.addText(" Gipfel erreicht ")
        let call = try XCTUnwrap(core.calls.first)
        XCTAssertEqual(call.kind, "text")
        XCTAssertEqual(call.source, "typed")
        XCTAssertEqual(call.text, "Gipfel erreicht")
        XCTAssertNil(call.file)
    }

    /// Held, the note ends when let go; a short tap starts it, the next tap ends it.
    func testVoiceHoldAndTap() throws {
        let media = controller()
        media.voiceDown()
        XCTAssertTrue(media.voiceRecording)
        Thread.sleep(forTimeInterval: MediaController.tapSeconds + 0.1)
        media.voiceUp()
        XCTAssertFalse(media.voiceRecording)
        XCTAssertEqual(core.calls.count, 1)
        let call = try XCTUnwrap(core.calls.first)
        XCTAssertEqual(call.kind, "voice")
        XCTAssertEqual(call.source, "spoken")
        XCTAssertTrue(call.fileExisted)
        XCTAssertTrue(try XCTUnwrap(call.file).hasSuffix(".m4a"))

        media.voiceDown()
        media.voiceUp()
        XCTAssertTrue(media.voiceRecording, "a tap starts it")
        media.voiceDown()
        media.voiceUp()
        XCTAssertFalse(media.voiceRecording, "the next tap ends it")
        XCTAssertEqual(core.calls.count, 2)
    }

    /// A voice note still spoken when the recording ends is kept.
    func testEndVoiceKeepsNote() {
        let media = controller()
        media.voiceDown()
        media.voiceUp()
        media.endVoice()
        XCTAssertFalse(media.voiceRecording)
        XCTAssertEqual(core.calls.map(\.kind), ["voice"])
    }

    func testOffsetText() {
        XCTAssertEqual(PhotoMetadata.offsetText(3600), "+01:00")
        XCTAssertEqual(PhotoMetadata.offsetText(-9000), "-02:30")
        XCTAssertEqual(PhotoMetadata.offsetSeconds("+01:00"), 3600)
        XCTAssertEqual(PhotoMetadata.offsetSeconds("-02:30"), -9000)
    }
}
