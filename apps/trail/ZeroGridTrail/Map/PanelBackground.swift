import SwiftUI

extension View {
    /// The background of a panel over the map. In the dark look it is solid black with a
    /// thin edge: on OLED black costs no power, while the blur of a material does.
    func panelBackground<S: Shape>(in shape: S) -> some View {
        modifier(PanelBackground(shape: shape))
    }
}

private struct PanelBackground<S: Shape>: ViewModifier {
    let shape: S
    @Environment(\.colorScheme) private var colorScheme

    func body(content: Content) -> some View {
        if colorScheme == .dark {
            content
                .background(Color.black, in: shape)
                .overlay(shape.stroke(Color.white.opacity(0.35), lineWidth: 1))
        } else {
            content.background(.regularMaterial, in: shape)
        }
    }
}
