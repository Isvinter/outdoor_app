import XCTest
@testable import ZeroGridTrail

final class TileURLProtocolTests: XCTestCase {
    private var sandbox: Sandbox!
    private var session: URLSession!

    override func setUpWithError() throws {
        sandbox = try Sandbox()
        try sandbox.stageMini()
        _ = sandbox.host.importPendingRegions()
        XCTAssertEqual(try sandbox.host.core.overview().regions.count, 1)
        TileURLProtocol.core = sandbox.host.core

        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [TileURLProtocol.self]
        session = URLSession(configuration: configuration)
    }

    override func tearDown() {
        session.invalidateAndCancel()
        TileURLProtocol.core = nil
        sandbox.remove()
    }

    private func get(_ path: String) async throws -> (Data, HTTPURLResponse) {
        let url = try XCTUnwrap(URL(string: TileURLProtocol.base + path))
        let (data, response) = try await session.data(from: url)
        return (data, try XCTUnwrap(response as? HTTPURLResponse))
    }

    /// The tile at `zoom` around a place, as `z/x/y`.
    private func tile(zoom: Int, latitude: Double, longitude: Double) -> String {
        let n = pow(2.0, Double(zoom))
        let x = Int((longitude + 180) / 360 * n)
        let phi = latitude * .pi / 180
        let y = Int((1 - asinh(tan(phi)) / .pi) / 2 * n)
        return "\(zoom)/\(x)/\(y)"
    }

    func testTileInsideRegionIs200() async throws {
        // The middle of the fixture's bbox.
        let path = "basemap/" + tile(zoom: 12, latitude: 47.575, longitude: 11.55)
        let expected = try XCTUnwrap(try sandbox.host.core.tile(path: "/" + path))
        let before = TileURLProtocol.stats.served

        let (data, response) = try await get(path)

        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual(response.value(forHTTPHeaderField: "Content-Type"), expected.mime)
        XCTAssertFalse(data.isEmpty)
        XCTAssertEqual(data, expected.data)
        XCTAssertEqual(TileURLProtocol.stats.served, before + 1)
    }

    func testTileOutsideRegionIs204() async throws {
        // 0/0/0 covers the world and so lies in every region; this one is far away.
        let before = TileURLProtocol.stats.empty

        let (data, response) = try await get("basemap/" + tile(zoom: 12, latitude: 0, longitude: 0))

        XCTAssertEqual(response.statusCode, 204)
        XCTAssertTrue(data.isEmpty)
        XCTAssertEqual(TileURLProtocol.stats.empty, before + 1)
    }

    func testLayerNoRegionHasIs204() async throws {
        // The fixture has no slope tiles.
        let (data, response) = try await get("slope/" + tile(zoom: 12, latitude: 47.575, longitude: 11.55))

        XCTAssertEqual(response.statusCode, 204)
        XCTAssertTrue(data.isEmpty)
    }

    func testUnknownLayerIs400() async throws {
        let before = TileURLProtocol.stats.failed

        let (data, response) = try await get("nope/1/2/3")

        XCTAssertEqual(response.statusCode, 400)
        XCTAssertTrue(data.isEmpty)
        XCTAssertEqual(TileURLProtocol.stats.failed, before + 1)
    }

    func testFontFromBundle() async throws {
        let (glyphs, response) = try await get("fonts/Noto%20Sans%20Regular/0-255.pbf")
        XCTAssertEqual(response.statusCode, 200)
        XCTAssertFalse(glyphs.isEmpty)

        let (missing, missingResponse) = try await get("fonts/Noto%20Sans%20Regular/65280-65535.pbf")
        XCTAssertEqual(missingResponse.statusCode, 200)
        XCTAssertTrue(missing.isEmpty)
    }

    func testFontPathCannotLeaveTheFonts() async throws {
        // Info.plist lies next to the fonts folder in the bundle.
        XCTAssertNotNil(Bundle.main.url(forResource: "Info", withExtension: "plist"))
        let (data, response) = try await get("fonts/../Info.plist")
        XCTAssertEqual(response.statusCode, 200)
        XCTAssertTrue(data.isEmpty)
    }
}
