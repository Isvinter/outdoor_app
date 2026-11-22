import XCTest
@testable import ZeroGridTrail

/// The same texts as `apps/base/ui/src/format.ts` gives in German.
final class FormatTests: XCTestCase {
    func testDistance() {
        XCTAssertEqual(Format.distance(850), "850 m")
        XCTAssertEqual(Format.distance(0), "0 m")
        XCTAssertEqual(Format.distance(1000), "1,00 km")
        XCTAssertEqual(Format.distance(12353), "12,35 km")
        XCTAssertEqual(Format.distance(1234567), "1.234,57 km")
    }

    func testHours() {
        XCTAssertEqual(Format.hours(3.6 * 3600), "3:36 h")
        XCTAssertEqual(Format.hours(7500), "2:05 h")
        XCTAssertEqual(Format.hours(29), "0:00 h")
        XCTAssertEqual(Format.hours(3599), "1:00 h")
    }

    func testElevation() {
        XCTAssertEqual(Format.elevation(nil), "–")
        XCTAssertEqual(Format.elevation(850.4), "850 m")
        XCTAssertEqual(Format.elevation(1210), "1.210 m")
    }

    func testOption() {
        XCTAssertEqual(Format.option("max_hiking_difficulty", 3), "T3")
        XCTAssertEqual(Format.option("use_hills", 0.8), "80 %")
        XCTAssertEqual(Format.option("use_roads", 0.1), "10 %")
    }

    func testRegionVersionIsADate() {
        XCTAssertEqual(Format.regionVersion("202611130000"), "13.11.2026")
        XCTAssertEqual(Format.regionVersion("v2"), "v2")
        XCTAssertEqual(Format.regionVersion(""), "")
    }
}
