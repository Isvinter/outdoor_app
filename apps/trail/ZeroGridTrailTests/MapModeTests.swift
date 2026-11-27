import CoreLocation
import XCTest
@testable import ZeroGridTrail

/// What a tap in "Abschnitte bearbeiten" chooses, and the choice of a section.
@MainActor
final class MapModeTests: XCTestCase {
    private var sandbox: Sandbox!

    override func setUp() async throws {
        sandbox = try Sandbox()
    }

    override func tearDown() async throws {
        sandbox.remove()
    }

    func testNothingNearIsATapOnTheMap() {
        XCTAssertEqual(RouteOverlay.target(leg: nil, section: nil), .map)
    }

    func testOneLineIsChosen() {
        XCTAssertEqual(RouteOverlay.target(leg: (2, 10), section: nil), .leg(2))
        XCTAssertEqual(RouteOverlay.target(leg: nil, section: (.track(1), 10)), .section(.track(1)))
    }

    func testNearerLineWins() {
        XCTAssertEqual(RouteOverlay.target(leg: (0, 3), section: (.track(1), 15)), .leg(0))
        XCTAssertEqual(RouteOverlay.target(leg: (0, 15), section: (.recording(2), 3)), .section(.recording(2)))
    }

    func testAboutAsNearAsks() {
        XCTAssertEqual(RouteOverlay.target(leg: (0, 5), section: (.track(1), 9)), .ask(0, .track(1)))
    }

    /// A point at the start of each section after the first; a rest is marked as one.
    func testMarks() {
        let a = CLLocationCoordinate2D(latitude: 47.5, longitude: 11.5)
        let b = CLLocationCoordinate2D(latitude: 47.6, longitude: 11.5)
        let c = CLLocationCoordinate2D(latitude: 47.7, longitude: 11.5)
        let marks = TrackOverlay.marks([[a, b], [b], [c, a], []], rests: [1])
        XCTAssertEqual(marks.map(\.at.latitude), [47.6, 47.7])
        XCTAssertEqual(marks.map(\.rest), [true, false])
    }

    /// Choosing outside "Abschnitte bearbeiten" asks for the mode; leaving it lets go of the choice.
    func testChoiceNeedsEditing() {
        let tracks = TracksController(
            core: sandbox.host.core, overlay: nil, recordingChanges: sandbox.host.recordingChanges)
        var wanted = 0
        tracks.onWantsEditing = {
            wanted += 1
            tracks.setEditing(true)
        }
        XCTAssertFalse(tracks.editing)
        tracks.select(.track(1))
        XCTAssertEqual(wanted, 1)
        XCTAssertTrue(tracks.editing)
        XCTAssertEqual(tracks.selectedSection, .track(1))

        tracks.select(.track(0))
        XCTAssertEqual(wanted, 1)
        tracks.setEditing(false)
        XCTAssertNil(tracks.selectedSection)
    }
}
