import SwiftUI

/// The bar above the map: which map, light or dark, and how strongly the slope
/// shading and the relief show.
struct MapToolbar: View {
    @ObservedObject var controller: MapController

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 8) {
                HStack(spacing: 0) {
                    baseButton("Sommer", .summer, identifier: "base-summer")
                    baseButton("Winter", .winter, identifier: "base-winter")
                }
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
                HStack(spacing: 0) {
                    lookButton("Hell", dark: false, identifier: "look-light")
                    lookButton("Dunkel", dark: true, identifier: "look-dark")
                }
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
            }
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
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
            }
            if controller.dark {
                HStack {
                    Text("Relief")
                        .font(.callout)
                    Slider(value: $controller.reliefStrength, in: 0...1)
                        .accessibilityIdentifier("relief-strength")
                        .accessibilityLabel("Relief")
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .frame(maxWidth: 360)
    }

    private func baseButton(_ title: String, _ base: MapBase, identifier: String) -> some View {
        let selected = controller.base == base
        return segment(title, selected: selected, identifier: identifier) {
            if !selected { controller.show(base) }
        }
    }

    private func lookButton(_ title: String, dark: Bool, identifier: String) -> some View {
        segment(title, selected: controller.dark == dark, identifier: identifier) {
            controller.setDark(dark)
        }
    }

    private func segment(
        _ title: String, selected: Bool, identifier: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
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
