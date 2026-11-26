import XCTest
@testable import ZeroGridTrail

@MainActor
final class TurnBannerTests: XCTestCase {
    func testSymbols() {
        XCTAssertEqual(TurnBanner.symbol(for: "right"), "arrow.turn.up.right")
        XCTAssertEqual(TurnBanner.symbol(for: "left"), "arrow.turn.up.left")
        XCTAssertEqual(TurnBanner.symbol(for: "slight_left"), "arrow.up.left")
        XCTAssertEqual(TurnBanner.symbol(for: "slight_right"), "arrow.up.right")
        XCTAssertEqual(TurnBanner.symbol(for: "sharp_left"), "arrow.down.left")
        XCTAssertEqual(TurnBanner.symbol(for: "sharp_right"), "arrow.down.right")
        XCTAssertEqual(TurnBanner.symbol(for: "keep_left"), "arrow.up.left")
        XCTAssertEqual(TurnBanner.symbol(for: "keep_right"), "arrow.up.right")
        XCTAssertEqual(TurnBanner.symbol(for: "u_turn"), "arrow.uturn.down")
        XCTAssertEqual(TurnBanner.symbol(for: "roundabout"), "arrow.triangle.turn.up.right.circle")
        XCTAssertEqual(TurnBanner.symbol(for: "stage_end"), "flag.checkered")
        XCTAssertEqual(TurnBanner.symbol(for: "something_new"), "arrow.up")
    }

    /// Every name is a symbol of the system.
    func testSymbolsExist() {
        for kind in [
            "slight_left", "slight_right", "left", "right", "sharp_left", "sharp_right", "u_turn", "keep_left",
            "keep_right", "roundabout", "stage_end", "",
        ] {
            XCTAssertNotNil(UIImage(systemName: TurnBanner.symbol(for: kind)), kind)
        }
    }

    func testDistanceOrNow() {
        let ahead = GuideControllerTests.turn(metres: 200)
        XCTAssertEqual(TurnBanner.distance(ahead), "200 m")
        XCTAssertEqual(TurnBanner.distance(GuideControllerTests.turn(stage: "now", metres: 8)), "Jetzt")
        XCTAssertEqual(TurnBanner.distance(GuideControllerTests.turn("stage_end", stage: "ahead", metres: 30)), "Jetzt")
    }
}
