import SwiftUI
import UIKit

/// The share sheet of iOS for files; `onDone` runs when it closes, shared or not.
struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]
    let onDone: () -> Void

    func makeUIViewController(context: Context) -> UIActivityViewController {
        let controller = UIActivityViewController(activityItems: items, applicationActivities: nil)
        controller.completionWithItemsHandler = { _, _, _, _ in onDone() }
        return controller
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
