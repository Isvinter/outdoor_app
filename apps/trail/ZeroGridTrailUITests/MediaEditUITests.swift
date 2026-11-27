import XCTest

/// Photos and notes in the detail of a track: shown always, changed in "Abschnitte bearbeiten" only.
final class MediaEditUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// A track of three sections (Wandern, Rast, Rad) with a photo and a voice
    /// note in the first, opened; the sheet at its middle step, the mode switch visible.
    private func trackWithMedia() -> XCUIApplication {
        let app = XCUIApplication.recordingSections(arguments: ["-media-fakes"]) { app in
            app.waitForStatus("first point") { $0.values["recording_xy"] != nil }
            app.buttons["media-photo"].tap()
            let shutter = app.buttons["camera-shutter"]
            XCTAssertTrue(shutter.waitForExistence(timeout: 5))
            shutter.tap()
            app.buttons["media-voice"].press(forDuration: 1.5)
            app.waitForStatus("photo and voice note") { $0.int("media") == 2 }
        }
        return app
    }

    private func element(_ app: XCUIApplication, _ identifier: String) -> XCUIElement {
        app.descendants(matching: .any)[identifier]
    }

    /// Switches the mode on the map, then pulls the sheet all the way up, which covers the switch.
    private func switchMode(_ app: XCUIApplication, _ mode: String) {
        app.buttons["mode-\(mode)"].tap()
        app.waitForStatus("\(mode) mode") { $0.values["mode"] == mode }
        app.pullSheet(by: -400)
    }

    private func chooseSection(_ app: XCUIApplication, _ index: Int) {
        switchMode(app, "sections")
        element(app, "section-\(index)").tap()
        app.waitForStatus("section chosen") { $0.values["section"] == "track:\(index)" }
    }

    private func writeNote(_ app: XCUIApplication, button: String, _ text: String) {
        app.buttons[button].tap()
        let field = app.textFields["note-field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText(text)
        app.buttons["note-save"].tap()
        XCTAssertTrue(field.waitForNonExistence(timeout: 5))
    }

    func testDetailShowsMediaWithoutEditButtons() {
        let app = trackWithMedia()
        // A note to the whole tour, written in "Abschnitte bearbeiten".
        switchMode(app, "sections")
        XCTAssertTrue(element(app, "whole-tour").waitForExistence(timeout: 5))
        writeNote(app, button: "add-note-tour", "Ganze Runde bei Sonne")
        app.pullSheet(by: 400)
        switchMode(app, "waypoints")

        XCTAssertTrue(element(app, "photo-0-0").waitForExistence(timeout: 5))
        XCTAssertTrue(element(app, "note-0-0").exists)
        XCTAssertTrue(element(app, "note-0-0-play").exists)
        XCTAssertEqual(element(app, "note-tour-0-text").label, "Ganze Runde bei Sonne")
        for absent in ["add-note-tour", "insert-image-tour", "add-note-0", "note-0-0-delete", "note-tour-0-edit"] {
            XCTAssertFalse(element(app, absent).exists, absent)
        }

        element(app, "photo-0-0").tap()
        XCTAssertTrue(element(app, "photo-full").waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["photo-delete"].exists)
        app.buttons["photo-close"].tap()
        XCTAssertTrue(element(app, "photo-full").waitForNonExistence(timeout: 5))
    }

    func testEditModeAddsNoteToSelectedSection() {
        let app = trackWithMedia()
        chooseSection(app, 2)
        writeNote(app, button: "add-note-2", "Ab hier mit dem Rad")
        let text = element(app, "note-2-0-text")
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        XCTAssertEqual(text.label, "Ab hier mit dem Rad")
        XCTAssertFalse(element(app, "note-0-1").exists, "not in the first section")
    }

    func testInsertImageFromLibrary() {
        let app = trackWithMedia()
        chooseSection(app, 2)
        app.buttons["insert-image-2"].tap()
        let choose = app.buttons["picker-choose"]
        XCTAssertTrue(choose.waitForExistence(timeout: 5))
        choose.tap()
        XCTAssertTrue(element(app, "photo-2-0").waitForExistence(timeout: 5))
        XCTAssertTrue(element(app, "photo-0-0").exists)
        XCTAssertEqual(app.mapStatus.int("media_files"), 3)
    }

    func testDeletePhotoAsks() {
        let app = trackWithMedia()
        chooseSection(app, 0)
        element(app, "photo-0-0").tap()
        app.buttons["photo-delete"].tap()
        let alert = app.alerts["Foto löschen?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        alert.buttons["Löschen"].tap()
        XCTAssertTrue(element(app, "photo-0-0").waitForNonExistence(timeout: 5))
        app.waitForStatus("file gone") { $0.int("media_files") == 1 }
    }

    /// The transcribed text can be changed; it stays marked as transcribed.
    func testEditTranscribedText() {
        let app = trackWithMedia()
        chooseSection(app, 0)
        app.buttons["note-0-0-transcribe"].tap()
        let text = element(app, "note-0-0-text")
        expectation(for: NSPredicate(format: "label == %@", "Hier gibt es frisches Quellwasser."), evaluatedWith: text)
        waitForExpectations(timeout: 10)

        app.buttons["note-0-0-edit"].tap()
        let field = app.textFields["note-field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        XCTAssertEqual(field.value as? String, "Hier gibt es frisches Quellwasser.")
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 12))
        field.typeText("Brunnen.")
        app.buttons["note-save"].tap()
        expectation(for: NSPredicate(format: "label == %@", "Hier gibt es frisches Brunnen."), evaluatedWith: text)
        waitForExpectations(timeout: 5)
        XCTAssertFalse(app.buttons["note-0-0-transcribe"].exists)
    }

    /// "In Text umwandeln": the text shows, the audio stays.
    func testTranscribeVoiceNote() {
        let app = trackWithMedia()
        chooseSection(app, 0)
        XCTAssertEqual(element(app, "note-0-0-text").label.hasSuffix("Sprachnotiz"), true)
        app.buttons["note-0-0-transcribe"].tap()
        let text = element(app, "note-0-0-text")
        expectation(for: NSPredicate(format: "label == %@", "Hier gibt es frisches Quellwasser."), evaluatedWith: text)
        waitForExpectations(timeout: 10)
        XCTAssertTrue(app.buttons["note-0-0-play"].exists)
        XCTAssertFalse(app.buttons["transcribe-all"].exists, "nothing left to turn into text")
    }

    /// The card of a section of the recording that runs takes notes too.
    func testRecordingSectionCardEditsMedia() {
        let app = XCUIApplication.replaying(arguments: ["-replaySpeed", "4", "-media-fakes"])
        app.waitForStatus("map rendered") { $0.rendered }
        app.startRecording("hike")
        app.waitForStatus("recorded line") { $0.values["recording_xy"] != nil }
        app.buttons["mode-sections"].tap()
        app.tapStatusPoint("recording_xy", 0)
        app.waitForStatus("section of the recording chosen") { $0.values["section"] == "recording:0" }
        XCTAssertTrue(app.buttons["add-note-0"].waitForExistence(timeout: 5))
        writeNote(app, button: "add-note-0", "Unterwegs nachgetragen")
        let text = element(app, "note-0-0-text")
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        XCTAssertEqual(text.label, "Unterwegs nachgetragen")
        app.waitForStatus("note stored") { $0.int("media") == 1 }
    }
}
