import AVFoundation
import SwiftUI

/// Records a voice note into a file. Replaced in the tests and with `-media-fakes`.
@MainActor
protocol AudioSource: AnyObject {
    /// Records into `url` until `stop`.
    func start(to url: URL) throws
    /// Ends the recording; the file is complete afterwards.
    func stop()
}

/// The microphone, AAC in an m4a file; it runs only while a note is spoken.
@MainActor
final class SystemAudio: AudioSource {
    private var recorder: AVAudioRecorder?

    struct Failed: LocalizedError {
        var errorDescription: String? { "Das Mikrofon nimmt nicht auf." }
    }

    func start(to url: URL) throws {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
        try session.setActive(true)
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 44_100,
            AVNumberOfChannelsKey: 1,
            AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
        ]
        let recorder = try AVAudioRecorder(url: url, settings: settings)
        guard recorder.record() else { throw Failed() }
        self.recorder = recorder
    }

    func stop() {
        recorder?.stop()
        recorder = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}

/// For the simulator: each note is a second of silence.
@MainActor
final class FakeAudio: AudioSource {
    private var url: URL?

    func start(to url: URL) throws {
        self.url = url
    }

    func stop() {
        guard let url else { return }
        self.url = nil
        try? MediaFakes.writeSilence(to: url)
    }
}

/// "Notiz" in the recording bar: held, it records until let go; a short tap
/// starts it, and the next tap ends it.
struct VoiceButton: View {
    @ObservedObject var media: MediaController
    @State private var pressing = false

    var body: some View {
        MediaButtonLabel(
            title: media.voiceRecording ? "Stopp" : "Notiz",
            symbol: media.voiceRecording ? "stop.circle.fill" : "mic",
            highlighted: media.voiceRecording
        )
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in
                    guard !pressing else { return }
                    pressing = true
                    media.voiceDown()
                }
                .onEnded { _ in
                    pressing = false
                    media.voiceUp()
                }
        )
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(media.voiceRecording ? "Stopp" : "Notiz")
        .accessibilityIdentifier("media-voice")
        .accessibilityValue(media.voiceRecording ? "läuft" : "")
        // VoiceOver's double tap: start, and end with the next.
        .accessibilityAction {
            media.voiceDown()
            media.voiceUp()
        }
    }
}

/// The look of the big buttons in the recording bar; at least 56 pt high, for gloves.
struct MediaButtonLabel: View {
    let title: String
    let symbol: String
    var highlighted = false

    var body: some View {
        VStack(spacing: 2) {
            Image(systemName: symbol)
                .font(.title3)
            Text(title)
                .font(.callout.weight(.semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.8)
        }
        .foregroundStyle(highlighted ? Color.white : Color.primary)
        .frame(maxWidth: .infinity, minHeight: 56)
        .background(
            highlighted ? Color(TrackOverlay.recordingColor) : Color(.secondarySystemFill),
            in: RoundedRectangle(cornerRadius: 12))
        .contentShape(RoundedRectangle(cornerRadius: 12))
    }
}
