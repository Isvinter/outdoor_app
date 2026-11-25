import XCTest
@testable import ZeroGridTrail

private final class FakeCenter: NotificationScheduling {
    var asked = 0
    var scheduled: [(title: String, body: String, sound: Bool)] = []
    func requestAuthorization() { asked += 1 }
    func schedule(id: String, title: String, body: String, sound: Bool) {
        scheduled.append((title, body, sound))
    }
}

private final class FakeHaptics: Haptics {
    var count = 0
    func deviation() { count += 1 }
}

@MainActor
final class DeviationAlertTests: XCTestCase {
    private let suite = "DeviationAlertTests"
    private var defaults: UserDefaults!
    private var center: FakeCenter!
    private var haptics: FakeHaptics!
    private var alert: DeviationAlert!

    override func setUp() {
        UserDefaults().removePersistentDomain(forName: suite)
        defaults = UserDefaults(suiteName: suite)
        center = FakeCenter()
        haptics = FakeHaptics()
        alert = DeviationAlert(center: center, haptics: haptics, defaults: defaults)
    }

    override func tearDown() {
        UserDefaults().removePersistentDomain(forName: suite)
    }

    private func info(next: UInt32?, wayBack: Double? = 650) -> GuidanceInfo {
        GuidanceInfo(
            phase: "off_route", alongM: 0, nextWaypoint: next, deviation: 1, wayBack: [], wayBackM: wayBack,
            toStageEndM: nil, toStageEndS: nil, toDestinationM: 5000, toDestinationS: nil)
    }

    func testBackgroundDeviationSchedulesOneNotification() {
        alert.handle(info(next: 3), appActive: false, waypoints: 6)
        XCTAssertEqual(center.scheduled.count, 1)
        XCTAssertEqual(center.scheduled.first?.title, "Abseits der Route")
        XCTAssertEqual(center.scheduled.first?.body, "Weg zurück zu Wegpunkt 4: 650 m.")
        XCTAssertEqual(center.scheduled.first?.sound, true)
        XCTAssertEqual(haptics.count, 1)
    }

    func testDestinationTextWithoutInnerWaypoint() {
        alert.handle(info(next: 1), appActive: false, waypoints: 2)
        XCTAssertEqual(center.scheduled.first?.body, "Weg zurück zum Ziel: 650 m.")
        XCTAssertEqual(DeviationAlert.body(info(next: 1, wayBack: 1250), waypoints: 2), "Weg zurück zum Ziel: 1,25 km.")
    }

    func testForegroundVibratesWithoutNotification() {
        alert.handle(info(next: 3), appActive: true, waypoints: 6)
        XCTAssertTrue(center.scheduled.isEmpty)
        XCTAssertEqual(haptics.count, 1)
    }

    func testSoundSwitchOffSendsSilentNotification() {
        XCTAssertTrue(alert.sound, "on by default")
        alert.sound = false
        alert.handle(info(next: 3), appActive: false, waypoints: 6)
        XCTAssertEqual(center.scheduled.first?.sound, false)
    }

    func testWithoutWayBack() {
        XCTAssertEqual(DeviationAlert.body(info(next: 3, wayBack: nil), waypoints: 6), "Kein Weg zurück gefunden.")
    }
}
