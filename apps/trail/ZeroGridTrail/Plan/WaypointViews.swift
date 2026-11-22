import MapLibre
import UIKit

extension UIColor {
    /// A colour as the core names it, "#rrggbb".
    convenience init(hex: String) {
        let value = UInt32(hex.dropFirst(), radix: 16) ?? 0
        self.init(
            red: CGFloat((value >> 16) & 0xff) / 255, green: CGFloat((value >> 8) & 0xff) / 255,
            blue: CGFloat(value & 0xff) / 255, alpha: 1)
    }
}

/// A waypoint of the planned route on the map.
final class WaypointAnnotation: MLNPointAnnotation {
    var index = 0
    var endsStage = false
}

/// Where the finger is on the elevation profile.
final class CursorAnnotation: MLNPointAnnotation {}

/// A numbered circle; the end of a stage is a dark rounded square. The view is
/// larger than what it draws, so that a finger hits it.
final class WaypointView: MLNAnnotationView {
    static let reuseIdentifier = "waypoint"
    private static let size: CGFloat = 24
    private static let touchSize: CGFloat = 44

    private let shape = UIView()
    private let label = UILabel()
    /// Dragging moves the waypoint at once, without pressing and holding first.
    let pan = UIPanGestureRecognizer()
    let tap = UITapGestureRecognizer()
    /// Where the finger took the view, relative to its centre.
    private var grip = CGPoint.zero

    var onTap: ((WaypointAnnotation) -> Void)?
    /// Called while the view is dragged, with its centre in the map view.
    var onDrag: ((WaypointAnnotation, CGPoint) -> Void)?
    var onDrop: ((WaypointAnnotation, CGPoint) -> Void)?

    override init(reuseIdentifier: String?) {
        super.init(reuseIdentifier: reuseIdentifier)
        frame = CGRect(x: 0, y: 0, width: Self.touchSize, height: Self.touchSize)
        let inset = (Self.touchSize - Self.size) / 2
        shape.frame = bounds.insetBy(dx: inset, dy: inset)
        shape.layer.borderColor = UIColor.white.cgColor
        shape.layer.borderWidth = 2
        shape.layer.shadowColor = UIColor.black.cgColor
        shape.layer.shadowOpacity = 0.3
        shape.layer.shadowRadius = 1.5
        shape.layer.shadowOffset = CGSize(width: 0, height: 1)
        shape.isUserInteractionEnabled = false
        addSubview(shape)
        label.frame = shape.bounds
        label.textAlignment = .center
        label.textColor = .white
        label.font = .systemFont(ofSize: 12, weight: .bold)
        label.adjustsFontSizeToFitWidth = true
        shape.addSubview(label)
        pan.addTarget(self, action: #selector(panned))
        tap.addTarget(self, action: #selector(tapped))
        addGestureRecognizer(pan)
        addGestureRecognizer(tap)
        isAccessibilityElement = true
        accessibilityTraits = .button
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func show(_ waypoint: WaypointAnnotation) {
        label.text = String(waypoint.index + 1)
        shape.backgroundColor = UIColor(hex: waypoint.endsStage ? "#1d1d1f" : "#0b63ce")
        shape.layer.cornerRadius = waypoint.endsStage ? 6 : Self.size / 2
        accessibilityIdentifier = "waypoint-\(waypoint.index + 1)"
        accessibilityLabel = waypoint.endsStage
            ? "Wegpunkt \(waypoint.index + 1), Etappenende" : "Wegpunkt \(waypoint.index + 1)"
    }

    @objc private func tapped() {
        if let waypoint = annotation as? WaypointAnnotation { onTap?(waypoint) }
    }

    @objc private func panned() {
        guard let waypoint = annotation as? WaypointAnnotation, let map = superview else { return }
        let finger = pan.location(in: map)
        switch pan.state {
        case .began:
            grip = CGPoint(x: finger.x - center.x, y: finger.y - center.y)
        case .changed:
            onDrag?(waypoint, CGPoint(x: finger.x - grip.x, y: finger.y - grip.y))
        case .ended:
            onDrop?(waypoint, CGPoint(x: finger.x - grip.x, y: finger.y - grip.y))
        case .cancelled, .failed:
            onDrop?(waypoint, center)
        default:
            break
        }
    }
}

/// A white circle with a blue rim, as the profile cursor on the desktop.
final class CursorView: MLNAnnotationView {
    static let reuseIdentifier = "cursor"

    override init(reuseIdentifier: String?) {
        super.init(reuseIdentifier: reuseIdentifier)
        frame = CGRect(x: 0, y: 0, width: 12, height: 12)
        backgroundColor = .white
        layer.cornerRadius = 6
        layer.borderColor = UIColor(hex: "#0b63ce").cgColor
        layer.borderWidth = 3
        isUserInteractionEnabled = false
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }
}
