import ImageIO
import SwiftUI

/// A photo of a track being looked at; with `editing`, it can be deleted.
struct ViewedPhoto: Identifiable {
    let item: MediaItem
    let url: URL
    let editing: Bool

    var id: String { item.id }
}

/// One photo, full screen, in full size.
struct PhotoViewer: View {
    let photo: ViewedPhoto
    let close: () -> Void
    let delete: () -> Void
    @State private var image: UIImage?
    @State private var asking = false

    var body: some View {
        NavigationStack {
            ZStack {
                Color.black.ignoresSafeArea()
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                        .accessibilityIdentifier("photo-full")
                        .accessibilityLabel("Foto")
                } else {
                    ProgressView()
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Schließen", action: close)
                        .accessibilityIdentifier("photo-close")
                }
                if photo.editing {
                    ToolbarItem(placement: .destructiveAction) {
                        Button("Löschen", role: .destructive) { asking = true }
                            .accessibilityIdentifier("photo-delete")
                    }
                }
            }
            .toolbarBackground(.visible, for: .navigationBar)
            .toolbarBackground(Color.black, for: .navigationBar)
        }
        .preferredColorScheme(.dark)
        .task {
            let url = photo.url
            image = await Task.detached { UIImage(contentsOfFile: url.path) }.value
        }
        .alert("Foto löschen?", isPresented: $asking) {
            Button("Löschen", role: .destructive, action: delete)
            Button("Abbrechen", role: .cancel) {}
        }
    }
}

/// A small picture of a photo, made from the file without loading it whole.
struct MediaThumbnail: View {
    let url: URL
    static let size: CGFloat = 64
    @State private var image: UIImage?

    var body: some View {
        ZStack {
            Color(.secondarySystemFill)
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                Image(systemName: "photo")
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: Self.size, height: Self.size)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .task(id: url) {
            image = await Task.detached { [url] in Self.thumbnail(url) }.value
        }
    }

    nonisolated static func thumbnail(_ url: URL) -> UIImage? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 240,
        ] as CFDictionary
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options).map { UIImage(cgImage: $0) }
    }
}
