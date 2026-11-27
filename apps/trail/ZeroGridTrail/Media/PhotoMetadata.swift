import CoreLocation
import ImageIO

/// Time and place in the EXIF of a photo: written for the photos of the
/// camera, read from the pictures put in from the library.
enum PhotoMetadata {
    /// `data` with the time it was taken and, if known, the place; the image
    /// itself is not encoded anew. `camera` is what the camera tells about the
    /// shot (`UIImagePickerController.InfoKey.mediaMetadata`). Nil if `data` is no image.
    static func stamp(_ data: Data, date: Date, location: CLLocation?, camera: [String: Any] = [:]) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let type = CGImageSourceGetType(source)
        else { return nil }
        let metadata = CGImageMetadataCreateMutable()
        func set(_ dictionary: CFString, _ key: CFString, _ value: Any) {
            CGImageMetadataSetValueMatchingImageProperty(metadata, dictionary, key, value as CFTypeRef)
        }
        // What the camera knows of the shot (exposure, lens), as far as it maps to EXIF.
        for dictionary in [kCGImagePropertyExifDictionary, kCGImagePropertyTIFFDictionary] {
            let values = camera[dictionary as String] as? [String: Any] ?? [:]
            for (key, value) in values where key != (kCGImagePropertyTIFFOrientation as String) {
                set(dictionary, key as CFString, value)
            }
        }
        let local = formatter("yyyy:MM:dd HH:mm:ss", zone: .current)
        let offset = offsetText(TimeZone.current.secondsFromGMT(for: date))
        set(kCGImagePropertyExifDictionary, kCGImagePropertyExifDateTimeOriginal, local.string(from: date))
        set(kCGImagePropertyExifDictionary, kCGImagePropertyExifDateTimeDigitized, local.string(from: date))
        set(kCGImagePropertyExifDictionary, kCGImagePropertyExifOffsetTimeOriginal, offset)
        set(kCGImagePropertyExifDictionary, kCGImagePropertyExifOffsetTimeDigitized, offset)
        if let location {
            let gps = kCGImagePropertyGPSDictionary
            let coordinate = location.coordinate
            set(gps, kCGImagePropertyGPSLatitude, abs(coordinate.latitude))
            set(gps, kCGImagePropertyGPSLatitudeRef, coordinate.latitude < 0 ? "S" : "N")
            set(gps, kCGImagePropertyGPSLongitude, abs(coordinate.longitude))
            set(gps, kCGImagePropertyGPSLongitudeRef, coordinate.longitude < 0 ? "W" : "E")
            if location.verticalAccuracy >= 0 {
                set(gps, kCGImagePropertyGPSAltitude, abs(location.altitude))
                set(gps, kCGImagePropertyGPSAltitudeRef, location.altitude < 0 ? 1 : 0)
            }
            if location.horizontalAccuracy >= 0 {
                set(gps, kCGImagePropertyGPSHPositioningError, location.horizontalAccuracy)
            }
            let utc = TimeZone(identifier: "UTC")!
            set(gps, kCGImagePropertyGPSDateStamp, formatter("yyyy:MM:dd", zone: utc).string(from: location.timestamp))
            set(gps, kCGImagePropertyGPSTimeStamp, formatter("HH:mm:ss", zone: utc).string(from: location.timestamp))
        }
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(output, type, 1, nil) else { return nil }
        let options = [kCGImageDestinationMetadata: metadata, kCGImageDestinationMergeMetadata: true] as CFDictionary
        guard CGImageDestinationCopyImageSource(destination, source, options, nil) else { return nil }
        return output as Data
    }

    /// When and where the picture was taken, as far as it says.
    static func read(_ url: URL) -> (timeMs: Int64?, lat: Double?, lon: Double?) {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        else { return (nil, nil, nil) }
        var time: Int64?
        if let exif = properties[kCGImagePropertyExifDictionary] as? [CFString: Any],
           let text = (exif[kCGImagePropertyExifDateTimeOriginal] ?? exif[kCGImagePropertyExifDateTimeDigitized])
            as? String
        {
            let offset = (exif[kCGImagePropertyExifOffsetTimeOriginal] ?? exif[kCGImagePropertyExifOffsetTime])
                as? String
            let zone = offset.flatMap(offsetSeconds).flatMap(TimeZone.init(secondsFromGMT:)) ?? .current
            if let date = formatter("yyyy:MM:dd HH:mm:ss", zone: zone).date(from: text) {
                time = Int64((date.timeIntervalSince1970 * 1000).rounded())
            }
        }
        var lat: Double?
        var lon: Double?
        if let gps = properties[kCGImagePropertyGPSDictionary] as? [CFString: Any],
           let latitude = (gps[kCGImagePropertyGPSLatitude] as? NSNumber)?.doubleValue,
           let longitude = (gps[kCGImagePropertyGPSLongitude] as? NSNumber)?.doubleValue
        {
            lat = (gps[kCGImagePropertyGPSLatitudeRef] as? String) == "S" ? -latitude : latitude
            lon = (gps[kCGImagePropertyGPSLongitudeRef] as? String) == "W" ? -longitude : longitude
        }
        return (time, lat, lon)
    }

    private static func formatter(_ format: String, zone: TimeZone) -> DateFormatter {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = zone
        formatter.dateFormat = format
        return formatter
    }

    /// "+01:00"
    static func offsetText(_ seconds: Int) -> String {
        let minutes = abs(seconds) / 60
        return String(format: "%@%02d:%02d", seconds < 0 ? "-" : "+", minutes / 60, minutes % 60)
    }

    static func offsetSeconds(_ text: String) -> Int? {
        let parts = text.dropFirst().split(separator: ":").compactMap { Int($0) }
        guard let sign = text.first, sign == "+" || sign == "-", parts.count == 2 else { return nil }
        return (sign == "-" ? -1 : 1) * (parts[0] * 3600 + parts[1] * 60)
    }
}
