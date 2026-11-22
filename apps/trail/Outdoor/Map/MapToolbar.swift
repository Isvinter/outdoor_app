import SwiftUI

/// The bar above the map: which map, and how strongly the slope shading shows.
struct MapToolbar: View {
    @ObservedObject var controller: MapController

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 0) {
                baseButton("Sommer", .summer, identifier: "base-summer")
                baseButton("Winter", .winter, identifier: "base-winter")
            }
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
            if controller.base == .winter {
                HStack {
                    Text("Hangneigung")
                        .font(.callout)
                    Slider(value: $controller.slopeOpacity, in: 0...1)
                        .accessibilityIdentifier("slope-opacity")
                        .accessibilityLabel("Hangneigung")
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .frame(maxWidth: 360)
    }

    private func baseButton(_ title: String, _ base: MapBase, identifier: String) -> some View {
        let selected = controller.base == base
        return Button {
            if !selected { controller.show(base) }
        } label: {
            Text(title)
                .font(.callout.weight(selected ? .semibold : .regular))
                .foregroundStyle(selected ? Color.white : Color.primary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
                .background(selected ? Color.accentColor : Color.clear, in: RoundedRectangle(cornerRadius: 10))
        }
        .accessibilityIdentifier(identifier)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
