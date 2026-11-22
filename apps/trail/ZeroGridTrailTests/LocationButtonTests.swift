import XCTest
@testable import ZeroGridTrail

final class LocationButtonTests: XCTestCase {
    func testAccuracyText() {
        XCTAssertEqual(LocationButton.text(accuracy: 4.6), "± 5 m")
        XCTAssertEqual(LocationButton.text(accuracy: 0.2), "± 1 m")
        XCTAssertEqual(LocationButton.text(accuracy: 999.4), "± 999 m")
        XCTAssertEqual(LocationButton.text(accuracy: 1234), "± 1,2 km")
    }
}
