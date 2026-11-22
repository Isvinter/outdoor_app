import Foundation
import MapLibre

/// Answers `outdoor-tiles://localhost/…` from the core and the bundled glyphs,
/// so that the map never needs a network. Same answers as the desktop's
/// `tiles` scheme (apps/base/src-tauri/src/tiles.rs):
///
///     outdoor-tiles://localhost/<layer>/<z>/<x>/<y>       tile, or 204 if no region has it
///     outdoor-tiles://localhost/fonts/<font>/<range>.pbf  glyph range bundled with the app
final class TileURLProtocol: URLProtocol {
    static let scheme = "outdoor-tiles"
    static let base = "outdoor-tiles://localhost/"

    private static let lock = NSLock()
    private static var sharedCore: CoreApp?
    private static var counts = (served: 0, empty: 0, failed: 0)

    /// The core that answers the tiles.
    static var core: CoreApp? {
        get { lock.withLock { sharedCore } }
        set { lock.withLock { sharedCore = newValue } }
    }

    /// How the requests ended so far: with content, without, with an error.
    /// For the tests and the self-test only.
    static var stats: (served: Int, empty: Int, failed: Int) {
        lock.withLock { counts }
    }

    /// Makes MapLibre fetch its tiles and glyphs here. Call before the first map exists.
    static func install(core: CoreApp) {
        self.core = core
        let network = MLNNetworkConfiguration.sharedManager
        let configuration: URLSessionConfiguration = network.sessionConfiguration ?? .default
        var classes: [AnyClass] = configuration.protocolClasses ?? []
        if !classes.contains(where: { $0 == TileURLProtocol.self }) {
            classes.insert(TileURLProtocol.self, at: 0)
        }
        configuration.protocolClasses = classes
        // The regions are the store; a second copy would go stale when a region is replaced.
        configuration.urlCache = nil
        network.sessionConfiguration = configuration
        MLNOfflineStorage.shared.setMaximumAmbientCacheSize(0) { _ in }
    }

    override class func canInit(with request: URLRequest) -> Bool {
        request.url?.scheme == scheme
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    // Blocking like the desktop's handler: one read from a local file.
    override func startLoading() {
        guard let url = request.url else {
            return client?.urlProtocol(self, didFailWithError: URLError(.badURL)) ?? ()
        }
        let (status, mime, body) = Self.answer(path: url.path(percentEncoded: true))
        Self.lock.withLock {
            switch status {
            case 200 where !body.isEmpty: Self.counts.served += 1
            case 200, 204: Self.counts.empty += 1
            default: Self.counts.failed += 1
            }
        }
        let headers = ["Content-Type": mime, "Content-Length": String(body.count), "Cache-Control": "no-store"]
        guard let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)
        else {
            return client?.urlProtocol(self, didFailWithError: URLError(.badServerResponse)) ?? ()
        }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        if !body.isEmpty {
            client?.urlProtocol(self, didLoad: body)
        }
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}

    private static func answer(path: String) -> (status: Int, mime: String, body: Data) {
        let none = "application/octet-stream"
        if path.hasPrefix("/fonts/") {
            // Font names contain spaces. A range the app does not bundle is answered
            // with an empty glyph set, so labels render with what is there.
            let name = String(path.dropFirst(1)).replacingOccurrences(of: "%20", with: " ")
            let glyphs = name.contains("..") ? nil
                : Bundle.main.resourceURL.flatMap { try? Data(contentsOf: $0.appendingPathComponent(name)) }
            return (200, "application/x-protobuf", glyphs ?? Data())
        }
        guard let core else { return (500, none, Data()) }
        do {
            guard let tile = try core.tile(path: path) else { return (204, none, Data()) }
            return (200, tile.mime, tile.data)
        } catch let CoreError.Failed(code, _) where code == "tile_path" {
            return (400, none, Data())
        } catch {
            return (500, none, Data())
        }
    }
}
