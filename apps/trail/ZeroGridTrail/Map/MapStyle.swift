import Foundation

/// The two maps, named after their style files in `styles/`.
enum MapBase: String, CaseIterable {
    case summer = "topo-summer"
    case winter = "topo-winter"
}

enum MapStyleError: LocalizedError {
    case missing(String)

    var errorDescription: String? {
        switch self {
        case .missing(let name): return "Der Kartenstil \(name) fehlt in der App."
        }
    }
}

enum MapStyle {
    /// The bundled style of `base`, filled by the core for the installed
    /// regions and written to the caches, because the map loads styles by URL.
    static func url(for base: MapBase, core: CoreApp) throws -> URL {
        guard let source = Bundle.main.url(forResource: base.rawValue, withExtension: "json", subdirectory: "styles")
        else { throw MapStyleError.missing(base.rawValue) }
        let template = try String(contentsOf: source, encoding: .utf8)
        let style = try core.offlineStyle(styleJson: template, tilesBase: TileURLProtocol.base, dark: false)
        let caches = try FileManager.default.url(
            for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let target = caches.appendingPathComponent("style-\(base.rawValue).json")
        try Data(style.utf8).write(to: target, options: .atomic)
        return target
    }
}
