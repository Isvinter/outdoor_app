import SwiftUI
import UIKit

/// The camera in Trail: shows itself full screen and hands over the shot as
/// JPEG with what the camera tells about it. Replaced with `-media-fakes`.
@MainActor
protocol CameraSource {
    var available: Bool { get }
    func view(capture: @escaping (_ jpeg: Data, _ metadata: [String: Any]) -> Void, cancel: @escaping () -> Void)
        -> AnyView
}

/// The system's camera (`UIImagePickerController`): it runs only while it shows.
struct SystemCamera: CameraSource {
    var available: Bool { UIImagePickerController.isSourceTypeAvailable(.camera) }

    func view(capture: @escaping (Data, [String: Any]) -> Void, cancel: @escaping () -> Void) -> AnyView {
        AnyView(CameraPicker(capture: capture, cancel: cancel).ignoresSafeArea())
    }
}

private struct CameraPicker: UIViewControllerRepresentable {
    let capture: (Data, [String: Any]) -> Void
    let cancel: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.cameraCaptureMode = .photo
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: UIImagePickerController, context: Context) {}

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let parent: CameraPicker

        init(_ parent: CameraPicker) {
            self.parent = parent
        }

        func imagePickerController(
            _ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]
        ) {
            // Full resolution; the orientation goes into the EXIF.
            guard let image = info[.originalImage] as? UIImage, let jpeg = image.jpegData(compressionQuality: 0.92)
            else { return parent.cancel() }
            parent.capture(jpeg, info[.mediaMetadata] as? [String: Any] ?? [:])
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            parent.cancel()
        }
    }
}

/// For the simulator, which has no camera: "Auslösen" hands over a fixed picture.
struct FakeCamera: CameraSource {
    var available: Bool { true }

    func view(capture: @escaping (Data, [String: Any]) -> Void, cancel: @escaping () -> Void) -> AnyView {
        AnyView(
            VStack(spacing: 24) {
                Spacer()
                Image(systemName: "camera.viewfinder")
                    .font(.system(size: 80))
                    .foregroundStyle(.white)
                Spacer()
                HStack {
                    Button("Abbrechen", action: cancel)
                        .accessibilityIdentifier("camera-cancel")
                    Spacer()
                    Button("Auslösen") { capture(MediaFakes.jpeg(), [:]) }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("camera-shutter")
                }
                .padding()
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.black))
    }
}
