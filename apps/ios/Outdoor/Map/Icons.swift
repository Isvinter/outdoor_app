import UIKit

/// Small pictograms for the points of interest: the shapes and colours of
/// `drawIcon` in apps/desktop/ui/src/map.ts, which the styles ask for by name.
enum Icons {
    static let names = ["peak", "saddle", "hut", "shelter", "viewpoint", "spring"]

    /// The desktop draws 32 × 32 pixels at pixel ratio 2, so 16 points on the
    /// map. Same here, with more pixels for the sharper displays.
    private static let size = CGSize(width: 16, height: 16)
    private static let scale: CGFloat = 4
    private static let house: [(CGFloat, CGFloat)] = [(16, 5), (28, 15), (25, 15), (25, 26), (7, 26), (7, 15), (4, 15)]

    static func image(named name: String) -> UIImage? {
        let colour: UInt32
        let path: UIBezierPath
        switch name {
        case "peak": (colour, path) = (0x4a3b28, polygon([(16, 6), (27, 25), (5, 25)]))
        case "saddle": (colour, path) = (0x6d5a43, polygon([(5, 9), (16, 16), (27, 9), (27, 14), (16, 22), (5, 14)]))
        case "hut": (colour, path) = (0xa3261f, polygon(house))
        case "shelter": (colour, path) = (0x6d5a43, polygon(house))
        case "viewpoint": (colour, path) = (0x6d5a43, polygon([(16, 24), (4, 10), (28, 10)]))
        case "spring":
            (colour, path) = (0x2f7fc4, UIBezierPath(
                arcCenter: CGPoint(x: 16, y: 16), radius: 7, startAngle: 0, endAngle: 2 * .pi, clockwise: true))
        default: return nil
        }
        let format = UIGraphicsImageRendererFormat()
        format.scale = scale
        format.opaque = false
        return UIGraphicsImageRenderer(size: size, format: format).image { context in
            // The shapes are given in the desktop's 32 × 32 grid.
            context.cgContext.scaleBy(x: size.width / 32, y: size.height / 32)
            // The white outline first, the fill over its inner half.
            path.lineJoinStyle = .round
            path.lineWidth = 3
            UIColor.white.setStroke()
            path.stroke()
            UIColor(
                red: CGFloat((colour >> 16) & 0xff) / 255, green: CGFloat((colour >> 8) & 0xff) / 255,
                blue: CGFloat(colour & 0xff) / 255, alpha: 1
            ).setFill()
            path.fill()
        }
    }

    private static func polygon(_ points: [(CGFloat, CGFloat)]) -> UIBezierPath {
        let path = UIBezierPath()
        for (index, point) in points.enumerated() {
            let point = CGPoint(x: point.0, y: point.1)
            if index == 0 { path.move(to: point) } else { path.addLine(to: point) }
        }
        path.close()
        return path
    }
}
