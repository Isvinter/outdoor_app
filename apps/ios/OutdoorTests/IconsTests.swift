import XCTest
@testable import Outdoor

final class IconsTests: XCTestCase {
    func testKnownNamesHaveImages() throws {
        for name in ["peak", "saddle", "hut", "shelter", "viewpoint", "spring"] {
            let image = try XCTUnwrap(Icons.image(named: name), name)
            // As large on the map as on the desktop: 32 px at pixel ratio 2.
            XCTAssertEqual(image.size, CGSize(width: 16, height: 16), name)
            XCTAssertEqual(image.cgImage?.width, 64, name)
        }
    }

    func testUnknownNameIsNil() {
        XCTAssertNil(Icons.image(named: "castle"))
        XCTAssertNil(Icons.image(named: ""))
    }

    /// Every icon a bundled style asks for can be drawn.
    func testStylesOnlyAskForKnownIcons() throws {
        for base in MapBase.allCases {
            let url = try XCTUnwrap(Bundle.main.url(forResource: base.rawValue, withExtension: "json", subdirectory: "styles"))
            let style = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
            let layers = try XCTUnwrap(style["layers"] as? [[String: Any]])
            for layer in layers {
                guard let icon = (layer["layout"] as? [String: Any])?["icon-image"] as? String else { continue }
                XCTAssertNotNil(Icons.image(named: icon), "\(base.rawValue): \(icon)")
            }
        }
    }
}
