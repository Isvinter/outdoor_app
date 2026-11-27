import XCTest

/// Photos, voice notes and notes on the way, with the stand-ins for camera, microphone and library.
final class MediaUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    private func recordingApp() -> XCUIApplication {
        let app = XCUIApplication.replaying(arguments: ["-media-fakes"])
        app.waitForStatus("map rendered") { $0.rendered }
        app.startRecording("hike")
        app.waitForStatus("first point") { $0.values["recording_xy"] != nil }
        return app
    }

    private func takePhoto(_ app: XCUIApplication, button: String = "media-photo") {
        app.buttons[button].tap()
        let shutter = app.buttons["camera-shutter"]
        XCTAssertTrue(shutter.waitForExistence(timeout: 5))
        shutter.tap()
        XCTAssertTrue(shutter.waitForNonExistence(timeout: 5))
    }

    private func addText(_ app: XCUIApplication, _ text: String) {
        app.buttons["media-text"].tap()
        let field = app.textFields["note-field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText(text)
        app.buttons["note-save"].tap()
        XCTAssertTrue(field.waitForNonExistence(timeout: 5))
    }

    /// "Foto", "Notiz" and "Text" in the bar, each at least 56 pt high.
    func testBarShowsMediaButtons() {
        let app = recordingApp()
        for (identifier, label) in [("media-photo", "Foto"), ("media-voice", "Notiz"), ("media-text", "Text")] {
            let button = app.buttons[identifier]
            XCTAssertTrue(button.exists, identifier)
            XCTAssertTrue(button.label.contains(label), button.label)
            XCTAssertGreaterThanOrEqual(button.frame.height, 56, identifier)
        }
        XCTAssertFalse(app.buttons["camera"].exists, "the bar's photo takes the place of the camera")
    }

    func testPhotoDuringRecording() {
        let app = recordingApp()
        takePhoto(app)
        let status = app.waitForStatus("photo stored") { $0.int("media") == 1 }
        XCTAssertEqual(status.int("media_files"), 1)
        app.waitForStatus("copy in the library") { $0.int("library") == 1 }
    }

    /// Held for two seconds: one note. Tap, tap: a second one.
    func testVoiceHoldAndTap() {
        let app = recordingApp()
        let voice = app.buttons["media-voice"]
        voice.press(forDuration: 2)
        app.waitForStatus("held note") { $0.int("media") == 1 && $0.int("voice") == 0 }

        voice.tap()
        app.waitForStatus("note running") { $0.int("voice") == 1 }
        XCTAssertEqual(voice.label, "Stopp")
        Thread.sleep(forTimeInterval: 1)
        voice.tap()
        let status = app.waitForStatus("tapped note") { $0.int("media") == 2 && $0.int("voice") == 0 }
        XCTAssertEqual(status.int("media_files"), 2)
    }

    /// "Sichern" keeps the note, "Abbrechen" nothing.
    func testTextNote() {
        let app = recordingApp()
        addText(app, "Quelle am Weg")
        app.waitForStatus("note stored") { $0.int("media") == 1 }

        app.buttons["media-text"].tap()
        let field = app.textFields["note-field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        XCTAssertTrue(app.navigationBars["Notiz"].exists)
        field.typeText("verworfen")
        app.buttons["note-cancel"].tap()
        XCTAssertTrue(field.waitForNonExistence(timeout: 5))
        Thread.sleep(forTimeInterval: 1)
        XCTAssertEqual(app.mapStatus.int("media"), 1)
    }

    /// Without a recording the camera on the map only fills the library.
    func testCameraWithoutRecording() {
        let app = XCUIApplication.replaying(arguments: ["-media-fakes"])
        app.waitForStatus("map rendered") { $0.rendered }
        takePhoto(app, button: "camera")
        let status = app.waitForStatus("copy in the library") { $0.int("library") == 1 }
        XCTAssertEqual(status.int("media_files"), 0)
    }

    func testDiscardWarnsAboutMedia() {
        let app = recordingApp()
        takePhoto(app)
        addText(app, "Rast am See")
        app.waitForStatus("two media") { $0.int("media") == 2 }
        app.buttons["recording-stop"].tap()
        XCTAssertTrue(app.buttons["track-discard"].waitForExistence(timeout: 5))
        app.buttons["track-discard"].tap()
        let warning = app.staticTexts["discard-media-warning"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        XCTAssertEqual(warning.label, "Verwerfen löscht auch 2 Fotos und Notizen.")
        app.buttons["discard-confirm"].tap()
        app.waitForStatus("files gone") { $0.int("media_files") == 0 }
    }
}
