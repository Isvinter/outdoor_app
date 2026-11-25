import XCTest
@testable import ZeroGridTrail

@MainActor
final class TurnAnnouncerTests: XCTestCase {
    private let suite = "TurnAnnouncerTests"
    private var defaults: UserDefaults!
    private var center: FakeCenter!
    private var speaker: RecordingSpeaker!
    private var announcer: TurnAnnouncer!

    override func setUp() {
        UserDefaults().removePersistentDomain(forName: suite)
        defaults = UserDefaults(suiteName: suite)
        center = FakeCenter()
        speaker = RecordingSpeaker()
        announcer = TurnAnnouncer(center: center, speaker: speaker, defaults: defaults)
    }

    override func tearDown() {
        UserDefaults().removePersistentDomain(forName: suite)
    }

    private let ahead = GuideControllerTests.turn(metres: 200)
    private let now = GuideControllerTests.turn(stage: "now", metres: 8)

    func testNothingWhenSwitchesOff() {
        XCTAssertFalse(announcer.notifications, "off by default")
        XCTAssertFalse(announcer.speech, "off by default")
        announcer.turn(ahead, appActive: false)
        announcer.deviation()
        announcer.arrived()
        XCTAssertTrue(center.scheduled.isEmpty)
        XCTAssertTrue(speaker.spoken.isEmpty)
    }

    func testNotificationReplacesAndIsRemoved() {
        announcer.notifications = true
        announcer.turn(ahead, appActive: false)
        announcer.turn(now, appActive: false)
        XCTAssertEqual(center.scheduled.map(\.id), ["turn", "turn"])
        XCTAssertEqual(center.scheduled.map(\.title), ["In 200 m rechts", "Jetzt rechts"])
        XCTAssertEqual(center.scheduled.first?.sound, true, "follows the switch of the deviation sound")
        announcer.cleared()
        XCTAssertEqual(center.removed, ["turn"])
    }

    func testNotificationSoundFollowsTheDeviationSwitch() {
        announcer.notifications = true
        defaults.set(false, forKey: DeviationAlert.soundKey)
        announcer.turn(ahead, appActive: false)
        XCTAssertEqual(center.scheduled.first?.sound, false)
    }

    func testNoNotificationInForeground() {
        announcer.notifications = true
        announcer.turn(ahead, appActive: true)
        XCTAssertTrue(center.scheduled.isEmpty)
    }

    func testSpeechSpeaksSpokenText() {
        announcer.speech = true
        announcer.turn(now, appActive: true)
        announcer.turn(now, appActive: false)
        XCTAssertEqual(speaker.spoken, ["Jetzt rechts", "Jetzt rechts"])
        XCTAssertTrue(center.scheduled.isEmpty, "notifications stay off")
    }

    func testSilentNotificationWhenSpeaking() {
        announcer.notifications = true
        announcer.speech = true
        announcer.turn(ahead, appActive: false)
        XCTAssertEqual(center.scheduled.count, 1)
        XCTAssertEqual(center.scheduled.first?.sound, false)
        XCTAssertEqual(speaker.spoken, ["In 200 m rechts"])
    }

    func testSpeaksDeviationAndArrival() {
        announcer.speech = true
        announcer.deviation()
        announcer.arrived()
        XCTAssertEqual(speaker.spoken, ["Abseits der Route", "Ziel erreicht"])
    }
}
