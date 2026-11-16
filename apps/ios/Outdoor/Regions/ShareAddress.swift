import Foundation

/// What a scanned QR code has to be to count as the address of the desktop's
/// share. This only sorts out foreign codes (a WLAN, a ticket, a shop); the
/// core puts the address into shape and stores it.
enum ShareAddress {
    /// The address in `scanned`, or nil if it is none: only `http://` and
    /// `https://`, with a host, without spaces.
    static func accept(_ scanned: String) -> String? {
        let text = scanned.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !text.contains(where: \.isWhitespace),
              let parts = URLComponents(string: text),
              let scheme = parts.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = parts.host, !host.isEmpty
        else { return nil }
        return text
    }
}
