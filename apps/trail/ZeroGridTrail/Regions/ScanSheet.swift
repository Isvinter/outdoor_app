import AVFoundation
import SwiftUI
import VisionKit

/// The camera, looking for the QR code the desktop shows beside its share.
/// `found` gets the address of the first code that is one; other codes are
/// passed over. Where the camera cannot be used, it says why.
struct ScanSheet: View {
    let found: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    private enum Camera {
        case asking
        case ready
        case unavailable(String)
    }

    @State private var camera = Camera.asking
    /// A code was seen that is not the address of a share.
    @State private var foreign = false

    var body: some View {
        NavigationStack {
            Group {
                switch camera {
                case .asking:
                    ProgressView()
                case .ready:
                    QRScanner { text in
                        if let address = ShareAddress.accept(text) {
                            found(address)
                            dismiss()
                        } else {
                            foreign = true
                        }
                    }
                    .ignoresSafeArea(edges: .bottom)
                    .overlay(alignment: .bottom) {
                        Text(foreign
                            ? "Das ist kein Code der Freigabe. Er steht am Desktop unter Karten → Im Heimnetz freigeben."
                            : "Halte die Kamera auf den QR-Code am Desktop.")
                            .font(.callout)
                            .padding(12)
                            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                            .padding()
                    }
                case .unavailable(let reason):
                    Text(reason)
                        .font(.callout)
                        .multilineTextAlignment(.center)
                        .padding()
                        .accessibilityIdentifier("scan-unavailable")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .navigationTitle("QR-Code scannen")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Abbrechen") { dismiss() }
                        .accessibilityIdentifier("scan-cancel")
                }
            }
        }
        .task { camera = await Self.check() }
    }

    private static let typeInstead = "Die Adresse lässt sich stattdessen über „Adresse eingeben“ eintippen."

    /// Asks for the camera the first time and says whether the scanner can run.
    @MainActor
    private static func check() async -> Camera {
        guard DataScannerViewController.isSupported else {
            return .unavailable("Dieses Gerät kann keine QR-Codes scannen. \(typeInstead)")
        }
        var status = AVCaptureDevice.authorizationStatus(for: .video)
        if status == .notDetermined {
            status = await AVCaptureDevice.requestAccess(for: .video) ? .authorized : .denied
        }
        guard status == .authorized else {
            return .unavailable(
                "Kein Zugriff auf die Kamera – in den Einstellungen unter ZeroGrid Trail erlauben. \(typeInstead)")
        }
        guard DataScannerViewController.isAvailable else {
            return .unavailable("Die Kamera ist gerade nicht verfügbar. \(typeInstead)")
        }
        return .ready
    }
}

/// VisionKit's scanner, for QR codes only.
private struct QRScanner: UIViewControllerRepresentable {
    let scanned: (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(scanned: scanned)
    }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])], qualityLevel: .balanced,
            recognizesMultipleItems: false, isHighFrameRateTrackingEnabled: false, isHighlightingEnabled: true)
        scanner.delegate = context.coordinator
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ scanner: DataScannerViewController, context: Context) {}

    static func dismantleUIViewController(_ scanner: DataScannerViewController, coordinator: Coordinator) {
        scanner.stopScanning()
    }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        private let scanned: (String) -> Void

        init(scanned: @escaping (String) -> Void) {
            self.scanned = scanned
        }

        func dataScanner(
            _ scanner: DataScannerViewController, didAdd added: [RecognizedItem], allItems: [RecognizedItem]
        ) {
            for item in added {
                if case .barcode(let code) = item, let text = code.payloadStringValue {
                    scanned(text)
                }
            }
        }
    }
}
