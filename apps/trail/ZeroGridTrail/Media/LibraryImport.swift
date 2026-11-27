import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// "Bild einfügen": pictures from the photo library, without access to all of
/// it (`PHPickerViewController` needs no permission). Each picture is handed
/// over as a copy in the temporary directory, in its own format.
@MainActor
protocol ImagePicking {
    func view(done: @escaping ([URL]) -> Void) -> AnyView
}

enum LibraryImport {
    /// When and where the picture was taken, as far as its metadata says.
    static func metadata(of url: URL) -> (timeMs: Int64?, lat: Double?, lon: Double?) {
        PhotoMetadata.read(url)
    }

    /// The extension a copy keeps: that of the picture, "jpg" if it has none.
    static func fileExtension(of url: URL) -> String {
        let ext = url.pathExtension.lowercased()
        if ext == "jpeg" { return "jpg" }
        return ext.isEmpty ? "jpg" : ext
    }

    /// A folder of its own in the temporary directory, for the copies of one choice.
    static func scratchFolder() -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("picked", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }
}

struct SystemImagePicker: ImagePicking {
    func view(done: @escaping ([URL]) -> Void) -> AnyView {
        AnyView(PickerView(done: done).ignoresSafeArea())
    }
}

private struct PickerView: UIViewControllerRepresentable {
    let done: ([URL]) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    func makeUIViewController(context: Context) -> PHPickerViewController {
        var configuration = PHPickerConfiguration()
        configuration.filter = .images
        configuration.selectionLimit = 0
        // The file as it is, HEIC stays HEIC.
        configuration.preferredAssetRepresentationMode = .current
        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: PHPickerViewController, context: Context) {}

    final class Coordinator: NSObject, PHPickerViewControllerDelegate {
        let done: ([URL]) -> Void

        init(done: @escaping ([URL]) -> Void) {
            self.done = done
        }

        func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
            guard !results.isEmpty else { return done([]) }
            let folder = LibraryImport.scratchFolder()
            let group = DispatchGroup()
            let lock = NSLock()
            var copies: [(Int, URL)] = []
            for (index, result) in results.enumerated() {
                let provider = result.itemProvider
                guard let type = provider.registeredTypeIdentifiers.first(where: {
                    UTType($0)?.conforms(to: .image) == true
                }) else { continue }
                group.enter()
                // The file lives only during the call: it is copied at once.
                provider.loadFileRepresentation(forTypeIdentifier: type) { url, _ in
                    defer { group.leave() }
                    guard let url else { return }
                    let copy = folder.appendingPathComponent("\(index).\(LibraryImport.fileExtension(of: url))")
                    if (try? FileManager.default.copyItem(at: url, to: copy)) != nil {
                        lock.withLock { copies.append((index, copy)) }
                    }
                }
            }
            group.notify(queue: .main) { [done] in
                done(copies.sorted { $0.0 < $1.0 }.map(\.1))
            }
        }
    }
}

/// For the simulator: "Bild wählen" hands over one JPEG without time and place.
struct FakeImagePicker: ImagePicking {
    func view(done: @escaping ([URL]) -> Void) -> AnyView {
        AnyView(
            NavigationStack {
                VStack(spacing: 24) {
                    Image(systemName: "photo.on.rectangle")
                        .font(.system(size: 60))
                    Button("Bild wählen") {
                        let file = LibraryImport.scratchFolder().appendingPathComponent("bild.jpg")
                        try? MediaFakes.jpeg(color: .systemPurple).write(to: file)
                        done([file])
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("picker-choose")
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Abbrechen") { done([]) }
                            .accessibilityIdentifier("picker-cancel")
                    }
                }
            })
    }
}
