import Foundation
import UIKit
import UserNotifications

/// Where local notifications go; the notification center in the app.
protocol NotificationScheduling: AnyObject {
    func requestAuthorization()
    func schedule(id: String, title: String, body: String, sound: Bool)
}

/// A short vibration.
protocol Haptics: AnyObject {
    func deviation()
}

extension UNUserNotificationCenter: NotificationScheduling {
    func requestAuthorization() {
        requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    func schedule(id: String, title: String, body: String, sound: Bool) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = sound ? .default : nil
        add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
    }
}

final class SystemHaptics: Haptics {
    func deviation() {
        DispatchQueue.main.async {
            UINotificationFeedbackGenerator().notificationOccurred(.warning)
        }
    }
}

/// Tells the user once per deviation: a vibration, and in the background or
/// on the locked screen a notification, with sound after the switch.
final class DeviationAlert {
    static let soundKey = "deviationSound"
    static let title = "Abseits der Route"

    private let center: NotificationScheduling
    private let haptics: Haptics
    private let defaults: UserDefaults

    init(center: NotificationScheduling, haptics: Haptics, defaults: UserDefaults = .standard) {
        self.center = center
        self.haptics = haptics
        self.defaults = defaults
        defaults.register(defaults: [Self.soundKey: true])
    }

    /// The switch "Signalton bei Abweichung", on by default.
    var sound: Bool {
        get { defaults.bool(forKey: Self.soundKey) }
        set { defaults.set(newValue, forKey: Self.soundKey) }
    }

    /// Asked for at the first "Führen"; without it there are only vibration and display.
    func requestPermission() {
        center.requestAuthorization()
    }

    /// A new deviation; `waypoints` is the number of waypoints of the route.
    func handle(_ info: GuidanceInfo, appActive: Bool, waypoints: Int) {
        haptics.deviation()
        guard !appActive else { return }
        center.schedule(
            id: "deviation-\(info.deviation)-\(UUID().uuidString)", title: Self.title,
            body: Self.body(info, waypoints: waypoints), sound: sound)
    }

    /// "Weg zurück zu Wegpunkt 4: 650 m." or "Weg zurück zum Ziel: 650 m."
    static func body(_ info: GuidanceInfo, waypoints: Int) -> String {
        guard let metres = info.wayBackM else { return "Kein Weg zurück gefunden." }
        return "\(GuideBar.target(info, waypoints: waypoints)): \(Format.distance(metres))."
    }
}
