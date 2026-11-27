import AVFoundation
import CoreLocation
import Photos
import SwiftUI
import UIKit

/// What the media controller uses of the core; replaced in the tests.
@MainActor
protocol MediaCore: AnyObject {
    func mediaDir() -> String
    func addRecordingMedia(kind: String, source: String, file: String?, text: String?) throws -> MediaItem
}

extension CoreApp: MediaCore {}

/// Puts a photo into the photo library (adding only). Replaced in the tests and with `-media-fakes`.
@MainActor
protocol LibrarySaver: AnyObject {
    func save(photo: Data, date: Date, location: CLLocation?) async throws
}

@MainActor
final class SystemLibrary: LibrarySaver {
    func save(photo: Data, date: Date, location: CLLocation?) async throws {
        try await PHPhotoLibrary.shared().performChanges {
            let request = PHAssetCreationRequest.forAsset()
            request.addResource(with: .photo, data: photo, options: nil)
            request.creationDate = date
            request.location = location
        }
    }
}

/// Keeps what would go to the library, for the tests and the simulator.
@MainActor
final class FakeLibrary: LibrarySaver {
    private(set) var saved: [Data] = []

    func save(photo: Data, date: Date, location: CLLocation?) async throws {
        saved.append(photo)
    }
}

/// What the sheet "Notiz" is open for.
enum NoteTarget: Identifiable {
    /// "Text" in the recording bar: a note to the running section.
    case recording
    /// A new note to a section (`nil`: the whole tour) of a track or of the recording that runs.
    case section(track: String, section: Int?)
    /// The text of a note or of a voice note, changed.
    case edit(MediaItem)

    var id: String {
        switch self {
        case .recording: "recording"
        case .section(let track, let section): "section-\(track)-\(section.map(String.init) ?? "tour")"
        case .edit(let item): "edit-\(item.id)"
        }
    }

    var initial: String {
        if case .edit(let item) = self { return item.text ?? "" }
        return ""
    }
}

/// "Bild einfügen" is open for a section (`nil`: the whole tour) of a track.
struct PickTarget: Identifiable {
    let track: String
    let section: Int?

    var id: String { "\(track)-\(section.map(String.init) ?? "tour")" }
}

/// Photos, voice notes and notes taken on the way. Each file is written to
/// `media/` first and then, at once, told to the core, which gives it time,
/// place and section; nothing is lost if the app ends after that.
@MainActor
final class MediaController: ObservableObject {
    nonisolated static let saveToLibraryKey = "savePhotosToLibrary"
    /// The camera shows (full screen).
    @Published var cameraShown = false
    /// The sheet "Notiz" shows.
    @Published var note: NoteTarget?
    /// A photo of a track shows full screen.
    @Published var viewing: ViewedPhoto?
    /// "Bild einfügen" shows the photo library.
    @Published var picking: PickTarget?
    @Published private(set) var voiceRecording = false
    /// A failure worth telling the user, in German.
    @Published var message: String?
    /// A permission was refused: the hint says where to allow it.
    @Published var blocked: MediaPermission?
    /// Photos put into the library since the start; for the self-test status.
    @Published private(set) var librarySaved = 0
    /// A photo or note was stored with the core.
    var onAdded: ((MediaItem) -> Void)?

    let camera: CameraSource
    let picker: ImagePicking
    let permissions: PermissionAsking
    private let core: MediaCore
    private let audio: AudioSource
    private let library: LibrarySaver
    private let defaults: UserDefaults
    private let lastFix: () -> CLLocation?
    private let isRecording: () -> Bool
    private var voiceFile: URL?
    private var voiceStarted: Date?
    /// The press began while a note was spoken: letting go ends it.
    private var pressEnds = false
    /// Shorter than this, a press is a tap: the note goes on until the next one.
    static let tapSeconds: TimeInterval = 0.5

    init(
        core: MediaCore, camera: CameraSource, audio: AudioSource, library: LibrarySaver,
        picker: ImagePicking, permissions: PermissionAsking, defaults: UserDefaults = .standard,
        lastFix: @escaping () -> CLLocation?, isRecording: @escaping () -> Bool
    ) {
        self.core = core
        self.camera = camera
        self.picker = picker
        self.audio = audio
        self.library = library
        self.permissions = permissions
        self.defaults = defaults
        self.lastFix = lastFix
        self.isRecording = isRecording
    }

    /// Back to the settings of a first start; for the UI tests.
    nonisolated static func resetSettings(_ defaults: UserDefaults = .standard) {
        defaults.removeObject(forKey: saveToLibraryKey)
    }

    /// "Fotos auch in der Mediathek sichern", on by default.
    var saveToLibrary: Bool {
        get { defaults.object(forKey: Self.saveToLibraryKey) as? Bool ?? true }
        set { defaults.set(newValue, forKey: Self.saveToLibraryKey) }
    }

    var mediaDirectory: URL { URL(fileURLWithPath: core.mediaDir(), isDirectory: true) }

    /// A new file in `media/`, and the path the core is told (relative to the data directory).
    func newFileURL(ext: String) -> (url: URL, relative: String) {
        Self.newFileURL(in: mediaDirectory, ext: ext)
    }

    nonisolated static func newFileURL(in directory: URL, ext: String) -> (url: URL, relative: String) {
        let name = "\(UUID().uuidString.lowercased()).\(ext)"
        return (directory.appendingPathComponent(name), "media/\(name)")
    }

    /// The files in `media/`; for the self-test status.
    var fileCount: Int {
        (try? FileManager.default.contentsOfDirectory(atPath: mediaDirectory.path))?.count ?? 0
    }

    // MARK: Photos

    /// "Foto" in the recording bar or the camera on the map; asks for the camera at the first use.
    func openCamera() {
        guard camera.available else {
            message = "Dieses Gerät hat keine Kamera."
            return
        }
        switch permissions.state(.camera) {
        case .allowed: cameraShown = true
        case .denied: blocked = .camera
        case .undetermined:
            Task {
                if await permissions.request(.camera) { cameraShown = true } else { blocked = .camera }
            }
        }
    }

    /// A shot of the camera. During a recording Trail keeps it (and the core
    /// learns of it), and the library gets a copy if so set; without a
    /// recording it only goes to the library.
    func capturePhoto(_ image: Data, metadata: [String: Any] = [:]) async {
        cameraShown = false
        let date = Date()
        let fix = lastFix()
        let photo = PhotoMetadata.stamp(image, date: date, location: fix, camera: metadata) ?? image
        var kept = false
        if isRecording() {
            let (url, relative) = newFileURL(ext: "jpg")
            do {
                try photo.write(to: url, options: .atomic)
                kept = true
                register(kind: "photo", source: "camera", file: relative, text: nil, what: "Das Foto")
            } catch {
                message = "Das Foto konnte nicht gesichert werden: \(describe(error))"
            }
        }
        // A photo Trail could not keep goes to the library in any case.
        if !kept || saveToLibrary {
            await saveInLibrary(photo, date: date, location: fix)
        }
    }

    private func saveInLibrary(_ photo: Data, date: Date, location: CLLocation?) async {
        guard await permissions.request(.library) else {
            blocked = .library
            return
        }
        do {
            try await library.save(photo: photo, date: date, location: location)
            librarySaved += 1
        } catch {
            message = "Das Foto konnte nicht in die Mediathek: \(describe(error))"
        }
    }

    // MARK: Notes

    /// "Text" in the recording bar.
    func openNote() {
        note = .recording
    }

    /// "Sichern" in the sheet "Notiz" of the recording; an empty text is not kept.
    func addText(_ text: String) {
        note = nil
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        register(kind: "text", source: "typed", file: nil, text: text, what: "Die Notiz")
    }

    /// "Notiz" pressed: the voice note starts (or, already running, ends when let go).
    func voiceDown() {
        if voiceRecording {
            pressEnds = true
            return
        }
        pressEnds = false
        switch permissions.state(.microphone) {
        case .allowed: break
        case .denied:
            blocked = .microphone
            return
        case .undetermined:
            // The question of iOS comes in between; the next press records.
            Task {
                if !(await permissions.request(.microphone)) { blocked = .microphone }
            }
            return
        }
        let (url, _) = newFileURL(ext: "m4a")
        do {
            try audio.start(to: url)
            voiceFile = url
            voiceStarted = Date()
            voiceRecording = true
        } catch {
            message = "Die Sprachnotiz konnte nicht starten: \(describe(error))"
        }
    }

    /// "Notiz" let go: held, the note ends; after a tap it goes on until the next tap.
    func voiceUp() {
        guard voiceRecording, let started = voiceStarted else { return }
        if pressEnds || Date().timeIntervalSince(started) >= Self.tapSeconds {
            endVoice()
        }
    }

    /// Ends a voice note that is spoken, and keeps it; also when the recording ends.
    func endVoice() {
        guard voiceRecording, let url = voiceFile else { return }
        audio.stop()
        voiceRecording = false
        voiceFile = nil
        voiceStarted = nil
        pressEnds = false
        finishVoice(at: url)
    }

    /// The voice note in `url` (in `media/`) is complete: the core learns of it.
    func finishVoice(at url: URL) {
        guard FileManager.default.fileExists(atPath: url.path) else {
            message = "Die Sprachnotiz ist leer geblieben."
            return
        }
        register(kind: "voice", source: "spoken", file: "media/\(url.lastPathComponent)", text: nil, what: "Die Sprachnotiz")
    }

    /// Tells the core; a failure leaves the file where it is and says so.
    private func register(kind: String, source: String, file: String?, text: String?, what: String) {
        do {
            let item = try core.addRecordingMedia(kind: kind, source: source, file: file, text: text)
            onAdded?(item)
        } catch {
            message = "\(what) konnte nicht zur Aufzeichnung gespeichert werden: \(describe(error))"
        }
    }
}

/// Stand-ins for the camera and the microphone, for the tests and the simulator.
enum MediaFakes {
    /// A small JPEG without metadata.
    static func jpeg(color: UIColor = .systemTeal) -> Data {
        renderer().jpegData(withCompressionQuality: 0.9) { context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 64, height: 48))
        }
    }

    /// A small PNG without metadata, like a screenshot.
    static func png() -> Data {
        renderer().pngData { context in
            UIColor.systemOrange.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 64, height: 48))
        }
    }

    private static func renderer() -> UIGraphicsImageRenderer {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: 64, height: 48), format: format)
    }

    /// `seconds` of silence as AAC in an m4a file.
    static func writeSilence(to url: URL, seconds: Double = 1) throws {
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 44_100, AVNumberOfChannelsKey: 1,
        ]
        let file = try AVAudioFile(forWriting: url, settings: settings)
        let format = file.processingFormat
        let frames = AVAudioFrameCount(seconds * format.sampleRate)
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames) else { return }
        buffer.frameLength = frames
        if let channels = buffer.floatChannelData {
            for channel in 0..<Int(format.channelCount) {
                channels[channel].initialize(repeating: 0, count: Int(frames))
            }
        }
        try file.write(from: buffer)
    }
}

/// The camera, the sheet "Notiz", a photo full screen, the photo library and
/// the question before deleting, presented from the plan sheet, since the map
/// already presents that sheet.
struct MediaPresenter: ViewModifier {
    @ObservedObject var media: MediaController
    @ObservedObject var tracks: TracksController

    func body(content: Content) -> some View {
        content
            .fullScreenCover(isPresented: $media.cameraShown) {
                media.camera.view(
                    capture: { image, metadata in Task { await media.capturePhoto(image, metadata: metadata) } },
                    cancel: { media.cameraShown = false })
            }
            .sheet(item: $media.note) { target in
                NoteEditor(initial: target.initial) { text in
                    switch target {
                    case .recording:
                        media.addText(text)
                    case .section(let track, let section):
                        media.note = nil
                        tracks.addNote(track: track, section: section, text: text)
                    case .edit(let item):
                        media.note = nil
                        tracks.editText(id: item.id, text: text)
                    }
                } cancel: {
                    media.note = nil
                }
            }
            .fullScreenCover(item: $media.viewing) { photo in
                PhotoViewer(photo: photo) {
                    media.viewing = nil
                } delete: {
                    media.viewing = nil
                    tracks.delete(id: photo.item.id)
                }
            }
            .sheet(item: $media.picking) { target in
                media.picker.view { files in
                    media.picking = nil
                    tracks.insertImages(track: target.track, section: target.section, files: files)
                }
            }
            .alert(
                tracks.confirmDelete?.kind == "photo" ? "Foto löschen?" : "Notiz löschen?",
                isPresented: Binding(
                    get: { tracks.confirmDelete != nil }, set: { if !$0 { tracks.confirmDelete = nil } }),
                presenting: tracks.confirmDelete
            ) { item in
                Button("Löschen", role: .destructive) { tracks.delete(id: item.id) }
                    .accessibilityIdentifier("media-delete-confirm")
                Button("Abbrechen", role: .cancel) {}
            }
    }
}

/// A refused permission: where to allow it.
struct PermissionNotice: View {
    let permission: MediaPermission
    let close: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Text(permission.hint)
                .font(.callout)
                .accessibilityIdentifier("permission-hint")
            Spacer(minLength: 0)
            Button("Einstellungen") {
                close()
                openAppSettings()
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("permission-settings")
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .onTapGesture(perform: close)
    }
}
