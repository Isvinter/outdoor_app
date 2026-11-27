import SwiftUI

/// The sheet "Notiz": a text to type (the dictation key of the keyboard works
/// there too), kept with "Sichern".
struct NoteEditor: View {
    let initial: String
    let save: (String) -> Void
    let cancel: () -> Void
    @State private var text: String
    @FocusState private var focused: Bool

    init(initial: String = "", save: @escaping (String) -> Void, cancel: @escaping () -> Void) {
        self.initial = initial
        self.save = save
        self.cancel = cancel
        _text = State(initialValue: initial)
    }

    var body: some View {
        NavigationStack {
            TextField("Notiz", text: $text, axis: .vertical)
                .lineLimit(4...12)
                .font(.body)
                .padding(12)
                .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 10))
                .padding()
                .focused($focused)
                .accessibilityIdentifier("note-field")
                .frame(maxHeight: .infinity, alignment: .top)
                .navigationTitle("Notiz")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Abbrechen", action: cancel)
                            .accessibilityIdentifier("note-cancel")
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Sichern") { save(text) }
                            .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                            .accessibilityIdentifier("note-save")
                    }
                }
        }
        .presentationDetents([.medium, .large])
        .onAppear { focused = true }
    }
}
