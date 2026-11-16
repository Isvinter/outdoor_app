import Foundation

// Wegwerfcode. Ein Lauf = eine CSV im Dokumentenverzeichnis.
// Zeilentypen: P = Position, A = Akku, E = Ereignis.

final class Protokoll {
    static let kopf = "zeit,typ,stufe,phase,hintergrund,lat,lon,hoehe,h_gen,v_gen,geschw,kurs,alter_s,behalten,akku,therm,text\n"

    let url: URL
    private let handle: FileHandle

    init(url: URL) throws {
        let fm = FileManager.default
        if !fm.fileExists(atPath: url.path) {
            // Auch bei gesperrtem Bildschirm schreibbar.
            fm.createFile(atPath: url.path, contents: Data(Self.kopf.utf8),
                          attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        }
        handle = try FileHandle(forWritingTo: url)
        try handle.seekToEnd()
        self.url = url
    }

    func zeile(_ felder: [String]) {
        let s = felder.map(Self.maskiert).joined(separator: ",") + "\n"
        try? handle.write(contentsOf: Data(s.utf8))
    }

    func schliessen() {
        try? handle.synchronize()
        try? handle.close()
    }

    private static func maskiert(_ s: String) -> String {
        guard s.contains(",") || s.contains("\"") || s.contains("\n") else { return s }
        return "\"" + s.replacingOccurrences(of: "\"", with: "\"\"") + "\""
    }

    static let zeitFormat: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    static var ordner: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }
}
