import XCTest
@testable import ZeroGridTrail

final class RouteColorTests: XCTestCase {
    func testDarkColorsBecomeReadableOnBlack() {
        for hex in ["#0b63ce", "#1d1d1f", "#7a1f1f", "#000000"] {
            let readable = RouteOverlay.readableOnBlack(hex)
            XCTAssertGreaterThanOrEqual(RouteOverlay.contrastOnBlack(readable), 4.5, hex)
        }
    }

    func testBrightColorsStayAsTheyAre() {
        XCTAssertEqual(RouteOverlay.readableOnBlack("#ffa033"), "#ffa033")
    }
}
