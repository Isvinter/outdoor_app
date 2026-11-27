import XCTest
@testable import ZeroGridTrail

/// Counts what it was asked to turn into text.
@MainActor
private final class CountingTranscriber: Transcribing {
    var supportsOnDevice = true
    private(set) var files: [URL] = []

    func transcribe(_ url: URL) async throws -> String {
        files.append(url)
        return "Text \(files.count)"
    }
}

@MainActor
final class TranscriberTests: XCTestCase {
    private var sandbox: Sandbox!
    private let suite = "TranscriberTests"

    override func setUp() async throws {
        sandbox = try Sandbox()
    }

    override func tearDown() async throws {
        UserDefaults().removePersistentDomain(forName: suite)
        sandbox.remove()
    }

    /// A voice note of the track, with a second of silence as its file.
    private func addVoice(_ recorded: RecordedTrack) throws -> MediaItem {
        let directory = URL(fileURLWithPath: sandbox.host.core.mediaDir(), isDirectory: true)
        let (url, relative) = MediaController.newFileURL(in: directory, ext: "m4a")
        try MediaFakes.writeSilence(to: url)
        let item = try sandbox.host.core.addTrackMedia(
            trackId: recorded.track.id, section: 0,
            input: MediaInput(kind: "voice", source: "spoken", file: relative, text: nil, timeMs: nil, lat: nil, lon: nil))
        recorded.tracks.mediaChanged(track: recorded.track.id)
        return item
    }

    /// Without recognition on the device nothing is sent anywhere: the note stays as it is.
    func testTranscriberRefusesWithoutOnDevice() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        let tracks = recorded.tracks
        let fake = CountingTranscriber()
        fake.supportsOnDevice = false
        tracks.transcriber = fake
        tracks.permissions = AllowingPermissions()
        let voice = try addVoice(recorded)

        await tracks.transcribe(id: voice.id)
        XCTAssertEqual(tracks.message, "Umwandeln geht für Deutsch auf diesem iPhone nur mit Netz. Das Audio bleibt.")
        XCTAssertTrue(fake.files.isEmpty)
        let after = try XCTUnwrap(tracks.media(for: 0).first)
        XCTAssertNil(after.text)
        XCTAssertFalse(after.transcribed)
        XCTAssertTrue(FileManager.default.fileExists(atPath: try XCTUnwrap(tracks.fileURL(after)).path))
    }

    func testTranscribeSetsText() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        let tracks = recorded.tracks
        tracks.transcriber = FakeTranscriber()
        tracks.permissions = AllowingPermissions()
        let voice = try addVoice(recorded)

        await tracks.transcribe(id: voice.id)
        XCTAssertNil(tracks.message)
        let after = try XCTUnwrap(tracks.media(for: 0).first)
        XCTAssertEqual(after.text, FakeTranscriber.text)
        XCTAssertTrue(after.transcribed)
        XCTAssertNotNil(after.file, "the audio stays")
    }

    /// "Alle umwandeln" takes only the voice notes without text.
    func testTranscribeAllSkipsDone() async throws {
        let recorded = try RecordedTrack(sandbox, suite: suite)
        let tracks = recorded.tracks
        let fake = CountingTranscriber()
        tracks.transcriber = fake
        tracks.permissions = AllowingPermissions()
        let done = try addVoice(recorded)
        _ = try sandbox.host.core.setMediaTranscript(id: done.id, text: "schon da")
        let open = try addVoice(recorded)
        tracks.mediaChanged(track: recorded.track.id)
        XCTAssertEqual(tracks.untranscribed.map(\.id), [open.id])

        await tracks.transcribeAll()
        XCTAssertEqual(fake.files.count, 1)
        XCTAssertTrue(tracks.untranscribed.isEmpty)
        let texts = Dictionary(uniqueKeysWithValues: tracks.media(for: 0).map { ($0.id, $0.text) })
        XCTAssertEqual(texts[done.id], "schon da")
        XCTAssertEqual(texts[open.id], "Text 1")
    }
}
