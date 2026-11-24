import SwiftUI

/// What the tour is recorded as; the last choice stands out. A tap starts.
struct ActivityPicker: View {
    let preselected: String
    let choose: (String) -> Void
    let cancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Aufzeichnen")
                .font(.headline)
            HStack(spacing: 8) {
                ForEach(Activity.all, id: \.self) { activity in
                    let selected = activity == preselected
                    Button {
                        choose(activity)
                    } label: {
                        Text(Activity.name(activity))
                            .font(.callout.weight(selected ? .semibold : .regular))
                            .foregroundStyle(selected ? Color.white : Color.primary)
                            .frame(maxWidth: .infinity, minHeight: 40)
                            .background(
                                selected ? Color.accentColor : Color(.secondarySystemFill),
                                in: RoundedRectangle(cornerRadius: 10))
                    }
                    .accessibilityIdentifier("activity-\(activity)")
                    .accessibilityAddTraits(selected ? .isSelected : [])
                    .accessibilityValue(selected ? "zuletzt gewählt" : "")
                }
            }
            Button("Abbrechen", action: cancel)
                .accessibilityIdentifier("activity-cancel")
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
    }
}
