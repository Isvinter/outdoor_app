import XCTest
@testable import Outdoor

final class ShareAddressTests: XCTestCase {
    func testAcceptsTheDesktopsAddress() {
        XCTAssertEqual(ShareAddress.accept("http://192.168.178.20:8765/"), "http://192.168.178.20:8765/")
        XCTAssertEqual(ShareAddress.accept("https://karten.example/catalog.json"), "https://karten.example/catalog.json")
        // A line break around the code is none of its content.
        XCTAssertEqual(ShareAddress.accept(" http://10.0.0.5:8765/\n"), "http://10.0.0.5:8765/")
    }

    func testRejectsForeignCodes() {
        XCTAssertNil(ShareAddress.accept("WIFI:S:x;;"))
        XCTAssertNil(ShareAddress.accept("hello"))
        XCTAssertNil(ShareAddress.accept("ftp://h/"))
        XCTAssertNil(ShareAddress.accept(""))
        XCTAssertNil(ShareAddress.accept("http://"))
        XCTAssertNil(ShareAddress.accept("http://192.168.1.5:8765/ und mehr"))
        XCTAssertNil(ShareAddress.accept("192.168.1.5:8765"))
    }
}
