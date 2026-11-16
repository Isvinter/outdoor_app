import Foundation

/// Numbers as the desktop shows them (`apps/desktop/ui/src/format.ts`), in German.
enum Format {
    private static func number(_ value: Double, digits: Int) -> String {
        let formatter = NumberFormatter()
        formatter.locale = Locale(identifier: "de_DE")
        formatter.numberStyle = .decimal
        formatter.minimumFractionDigits = digits
        formatter.maximumFractionDigits = digits
        return formatter.string(from: NSNumber(value: value)) ?? String(value)
    }

    /// "850 m" below a kilometre, "12,35 km" from there on.
    static func distance(_ metres: Double) -> String {
        metres < 1000 ? "\(number(metres, digits: 0)) m" : "\(number(metres / 1000, digits: 2)) km"
    }

    /// A travel time such as "2:05 h".
    static func hours(_ seconds: Double) -> String {
        let minutes = Int((seconds / 60).rounded())
        return String(format: "%d:%02d h", minutes / 60, minutes % 60)
    }

    static func elevation(_ metres: Double?) -> String {
        metres.map { "\(number($0, digits: 0)) m" } ?? "–"
    }

    /// The size of a region, such as "1,2 GB".
    static func bytes(_ count: UInt64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: Int64(clamping: count))
    }

    /// The version of a region ("202611130000", the time it was built) as a
    /// date, "13.11.2026"; anything else as it is.
    static func regionVersion(_ version: String) -> String {
        guard version.count == 12, version.allSatisfy(\.isNumber) else { return version }
        let digits = Array(version)
        return "\(String(digits[6..<8])).\(String(digits[4..<6])).\(String(digits[0..<4]))"
    }

    /// A setting of a profile: a hiking grade "T3", otherwise a share "80 %".
    static func option(_ key: String, _ value: Double) -> String {
        key == "max_hiking_difficulty" ? "T\(Int(value.rounded()))" : "\(Int((value * 100).rounded())) %"
    }
}
