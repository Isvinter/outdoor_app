import AVFoundation
import Photos
import Speech
import UIKit

/// What photos and notes need the user's permission for. Asked at the first use only.
enum MediaPermission: String {
    case camera, microphone, speech, library

    var name: String {
        switch self {
        case .camera: "Kamera"
        case .microphone: "Mikrofon"
        case .speech: "Spracherkennung"
        case .library: "Fotomediathek"
        }
    }

    /// Shown with a button "Einstellungen" once the permission was refused.
    var hint: String { "\(name) in den Einstellungen erlauben" }
}

enum PermissionState {
    case allowed, denied, undetermined
}

/// Asks iOS; replaced in the tests and with `-media-fakes`.
protocol PermissionAsking {
    func state(_ permission: MediaPermission) -> PermissionState
    /// Asks if not yet asked; true when allowed.
    func request(_ permission: MediaPermission) async -> Bool
}

struct SystemPermissions: PermissionAsking {
    func state(_ permission: MediaPermission) -> PermissionState {
        switch permission {
        case .camera: Self.capture(.video)
        case .microphone: Self.capture(.audio)
        case .speech:
            switch SFSpeechRecognizer.authorizationStatus() {
            case .authorized: .allowed
            case .notDetermined: .undetermined
            default: .denied
            }
        case .library:
            switch PHPhotoLibrary.authorizationStatus(for: .addOnly) {
            case .authorized, .limited: .allowed
            case .notDetermined: .undetermined
            default: .denied
            }
        }
    }

    private static func capture(_ type: AVMediaType) -> PermissionState {
        switch AVCaptureDevice.authorizationStatus(for: type) {
        case .authorized: .allowed
        case .notDetermined: .undetermined
        default: .denied
        }
    }

    func request(_ permission: MediaPermission) async -> Bool {
        switch state(permission) {
        case .allowed: return true
        case .denied: return false
        case .undetermined: break
        }
        switch permission {
        case .camera: return await AVCaptureDevice.requestAccess(for: .video)
        case .microphone: return await AVCaptureDevice.requestAccess(for: .audio)
        case .speech:
            return await withCheckedContinuation { done in
                SFSpeechRecognizer.requestAuthorization { done.resume(returning: $0 == .authorized) }
            }
        case .library:
            let status = await PHPhotoLibrary.requestAuthorization(for: .addOnly)
            return status == .authorized || status == .limited
        }
    }
}

/// Everything allowed: for the simulator's test stand-ins.
struct AllowingPermissions: PermissionAsking {
    func state(_ permission: MediaPermission) -> PermissionState { .allowed }
    func request(_ permission: MediaPermission) async -> Bool { true }
}

/// Opens the app's page in the Settings app.
@MainActor
func openAppSettings() {
    if let url = URL(string: UIApplication.openSettingsURLString) {
        UIApplication.shared.open(url)
    }
}
