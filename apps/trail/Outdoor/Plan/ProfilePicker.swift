import SwiftUI

/// The profile for the last and all new legs. A profile that cannot be
/// computed is greyed out.
struct ProfileMenu: View {
    @ObservedObject var plan: PlanController

    var body: some View {
        Menu {
            ForEach(plan.profiles, id: \.profile) { choice in
                let name = PlanTexts.profile(choice.profile)
                Button {
                    plan.choose(profile: choice.profile)
                } label: {
                    if choice.profile == plan.profile {
                        Label(name, systemImage: "checkmark")
                    } else {
                        Text(choice.available ? name : "\(name) (nicht verfügbar)")
                    }
                }
                .disabled(!choice.available)
            }
        } label: {
            HStack(spacing: 4) {
                Circle()
                    .fill(Color(UIColor(hex: plan.profiles.first { $0.profile == plan.profile }?.color ?? "#000000")))
                    .frame(width: 10, height: 10)
                Text(PlanTexts.profile(plan.profile))
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.caption)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: 40)
            .background(Color(.secondarySystemFill), in: RoundedRectangle(cornerRadius: 10))
        }
        .accessibilityIdentifier("plan-profile")
        .accessibilityLabel("Sportart")
        .accessibilityValue(PlanTexts.profile(plan.profile))
    }
}

/// One slider per setting of the chosen profile, as the core describes them.
struct ProfileOptions: View {
    @ObservedObject var plan: PlanController

    var body: some View {
        VStack(spacing: 6) {
            ForEach(plan.optionSpecs, id: \.key) { spec in
                OptionSlider(spec: spec, committed: plan.value(of: spec)) { plan.set(option: spec.key, to: $0) }
                    // A new profile or a loaded route starts the slider afresh.
                    .id("\(plan.profile)-\(spec.key)-\(plan.value(of: spec))")
            }
        }
    }
}

/// Shows the value while the finger moves; the route is computed when it lifts.
private struct OptionSlider: View {
    let spec: OptionInfo
    let commit: (Double) -> Void
    @State private var value: Double

    init(spec: OptionInfo, committed: Double, commit: @escaping (Double) -> Void) {
        self.spec = spec
        self.commit = commit
        _value = State(initialValue: committed)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(PlanTexts.option(spec.key))
                Spacer()
                Text(Format.option(spec.key, value))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .font(.callout)
            Slider(value: $value, in: spec.min...spec.max, step: spec.step) { editing in
                if !editing { commit(value) }
            }
            .accessibilityIdentifier("option-\(spec.key)")
            .accessibilityLabel(PlanTexts.option(spec.key))
            .accessibilityValue(Format.option(spec.key, value))
        }
    }
}
