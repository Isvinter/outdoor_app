import Speech

/// Turns a voice note into text, on the device only: nothing goes over the
/// network. Replaced in the tests and with `-media-fakes`.
@MainActor
protocol Transcribing {
    /// German can be recognised on this device without a network.
    var supportsOnDevice: Bool { get }
    func transcribe(_ url: URL) async throws -> String
}

struct SystemTranscriber: Transcribing {
    private let recognizer = SFSpeechRecognizer(locale: Locale(identifier: "de-DE"))

    var supportsOnDevice: Bool { recognizer?.supportsOnDeviceRecognition == true }

    struct Failed: LocalizedError {
        var errorDescription: String? { "Die Spracherkennung ist nicht bereit." }
    }

    func transcribe(_ url: URL) async throws -> String {
        guard let recognizer, recognizer.supportsOnDeviceRecognition else { throw Failed() }
        let request = SFSpeechURLRecognitionRequest(url: url)
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = false
        request.addsPunctuation = true
        return try await withCheckedThrowingContinuation { done in
            var finished = false
            _ = recognizer.recognitionTask(with: request) { result, error in
                guard !finished else { return }
                if let error {
                    finished = true
                    done.resume(throwing: error)
                } else if let result, result.isFinal {
                    finished = true
                    done.resume(returning: result.bestTranscription.formattedString)
                }
            }
        }
    }
}

/// For the tests and the simulator: a fixed text, or no recognition on the device.
struct FakeTranscriber: Transcribing {
    static let text = "Hier gibt es frisches Quellwasser."
    var supportsOnDevice = true
    var text = FakeTranscriber.text

    func transcribe(_ url: URL) async throws -> String { text }
}
