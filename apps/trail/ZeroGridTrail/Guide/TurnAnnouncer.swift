import AVFoundation
import Foundation

/// Where spoken texts go; the voice of the device in the app.
protocol Speaking: AnyObject {
    func speak(_ text: String)
}

/// Speaks with the German voice on the device, also with the ring switch on
/// silent. The audio session is active only while it speaks: other audio
/// (music) is lowered and comes back afterwards. A new text cuts off the old.
final class SystemSpeaker: NSObject, Speaking, AVSpeechSynthesizerDelegate {
    private let synthesizer = AVSpeechSynthesizer()
    private let voice = AVSpeechSynthesisVoice(language: "de-DE")
    /// The utterance spoken last; the session is released when it ends.
    private var latest: AVSpeechUtterance?

    override init() {
        super.init()
        synthesizer.delegate = self
    }

    func speak(_ text: String) {
        DispatchQueue.main.async { [self] in
            let session = AVAudioSession.sharedInstance()
            try? session.setCategory(
                .playback, mode: .voicePrompt, options: [.duckOthers, .interruptSpokenAudioAndMixWithOthers])
            try? session.setActive(true)
            if synthesizer.isSpeaking { synthesizer.stopSpeaking(at: .immediate) }
            let utterance = AVSpeechUtterance(string: text)
            utterance.voice = voice
            latest = utterance
            synthesizer.speak(utterance)
        }
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        ended(utterance)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        ended(utterance)
    }

    /// A text cut off by a newer one keeps the session for that one.
    private func ended(_ utterance: AVSpeechUtterance) {
        DispatchQueue.main.async { [self] in
            guard utterance === latest else { return }
            latest = nil
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        }
    }
}

/// Tells the turn hints as they come, after two switches, both off by default:
/// as a notification in the background or on the locked screen, and by voice.
/// With the voice on, getting off the network of ways and the arrival are spoken too.
final class TurnAnnouncer {
    static let notificationsKey = "turnNotifications"
    static let speechKey = "turnSpeech"
    /// One identifier for all hints: the next replaces the last, at most one stands there.
    static let notificationID = "turn"

    private let center: NotificationScheduling
    private let speaker: Speaking
    private let defaults: UserDefaults

    init(center: NotificationScheduling, speaker: Speaking, defaults: UserDefaults = .standard) {
        self.center = center
        self.speaker = speaker
        self.defaults = defaults
        defaults.register(defaults: [Self.notificationsKey: false, Self.speechKey: false])
    }

    /// The switch "Abbiegehinweise als Mitteilung".
    var notifications: Bool {
        get { defaults.bool(forKey: Self.notificationsKey) }
        set { defaults.set(newValue, forKey: Self.notificationsKey) }
    }

    /// The switch "Abbiegehinweise ansagen".
    var speech: Bool {
        get { defaults.bool(forKey: Self.speechKey) }
        set { defaults.set(newValue, forKey: Self.speechKey) }
    }

    /// A new hint.
    func turn(_ info: TurnInfo, appActive: Bool) {
        if speech { speaker.speak(info.spoken) }
        guard notifications, !appActive else { return }
        // The sound follows the switch "Signalton bei Abweichung"; none when spoken, so as not to ring twice.
        let sound = defaults.object(forKey: DeviationAlert.soundKey) as? Bool ?? true
        center.schedule(id: Self.notificationID, title: info.spoken, body: "", sound: sound && !speech)
    }

    /// The hint no longer holds: past the turn, off the route, or the guidance ended.
    func cleared() {
        center.remove(id: Self.notificationID)
    }

    /// Off the network of ways: "Abseits der Route. Die Route liegt 120 m bei 150 Grad."
    func offNetwork(_ spoken: String) {
        if speech { speaker.speak(spoken) }
    }

    func arrived() {
        if speech { speaker.speak("Ziel erreicht") }
    }
}
