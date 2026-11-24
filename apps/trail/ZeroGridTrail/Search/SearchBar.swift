import CoreLocation
import SwiftUI

/// The search above the map: the field, the quick choices, the list of hits and
/// the card of the chosen one. The map stays visible below it.
struct SearchBar: View {
    @ObservedObject var search: SearchController
    @ObservedObject var flow: RouteHereFlow
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            field
            if search.showsCategories {
                categories
            }
            if search.showsList {
                list
            }
            if search.selected == nil {
                ForEach(search.notes, id: \.self) { note in
                    Text(note)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("search-note")
                }
            }
            if let hit = search.selected {
                ResultCard(hit: hit, search: search, action: cardAction(hit))
            }
            if flow.step != .idle {
                RouteHereBanner(flow: flow)
            }
        }
        .padding(10)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .onChange(of: focused) { focused in
            if focused { search.open = true }
        }
        .onChange(of: search.selected) { selected in
            if selected != nil { focused = false }
        }
    }

    private func cardAction(_ hit: SearchHit) -> String? {
        guard flow.step.picksStart else { return "Route hierher" }
        return flow.isTarget(CLLocationCoordinate2D(latitude: hit.lat, longitude: hit.lon)) ? nil : "Hier starten"
    }

    private var field: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            TextField("Ort, Straße oder Name", text: $search.text)
                .focused($focused)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
                .onSubmit { focused = false }
                .accessibilityIdentifier("search-field")
            if search.open || !search.text.isEmpty || search.results != nil {
                Button {
                    focused = false
                    search.close()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                        .frame(minWidth: 32, minHeight: 32)
                }
                .accessibilityIdentifier("search-close")
                .accessibilityLabel("Suche schließen")
            }
        }
    }

    private var categories: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(SearchTexts.categories, id: \.self) { key in
                    let chosen = search.category == key
                    Button {
                        focused = false
                        search.chooseCategory(key)
                    } label: {
                        Text(SearchTexts.category(key))
                            .font(.footnote.weight(chosen ? .semibold : .regular))
                            .foregroundStyle(chosen ? Color.white : Color.primary)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 7)
                            .background(
                                chosen ? Color.accentColor : Color(.secondarySystemFill), in: Capsule())
                    }
                    .accessibilityIdentifier("search-category-\(key)")
                    .accessibilityAddTraits(chosen ? .isSelected : [])
                }
            }
        }
    }

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(Array((search.results?.hits ?? []).enumerated()), id: \.offset) { index, hit in
                    Button {
                        search.select(hit)
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(SearchTexts.title(hit))
                                .foregroundStyle(Color.primary)
                            Text(SearchTexts.details(hit, distance: Format.distance))
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .accessibilityIdentifier("search-hit-\(index)")
                    Divider()
                }
            }
        }
        .scrollDismissesKeyboard(.immediately)
        // The map has to stay visible below the list.
        .frame(maxHeight: 220)
    }
}

/// What "Route hierher" asks at the moment.
private struct RouteHereBanner: View {
    @ObservedObject var flow: RouteHereFlow

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(text)
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("route-here-text")
            switch flow.step {
            case .choosing:
                button("Neue Route", identifier: "route-here-new", primary: true) { flow.chooseNewRoute() }
                button("Als nächstes Ziel anhängen", identifier: "route-here-append") { flow.chooseAppend() }
            case .waiting:
                button("Start selbst wählen", identifier: "route-here-choose-start", primary: true) {
                    flow.chooseStartMyself()
                }
            case .pickStart(_, true):
                button("Mein Standort", identifier: "route-here-my-position", primary: true) {
                    flow.chooseMyPosition()
                }
            default:
                EmptyView()
            }
            button("Abbrechen", identifier: "route-here-cancel") { flow.cancel() }
        }
    }

    private var text: String {
        switch flow.step {
        case .choosing(let unsaved):
            return unsaved ? "Nicht gespeichert: Bei „Neue Route“ geht die alte verloren." : "Route hierher"
        case .waiting:
            return "Standort wird gesucht …"
        case .pickStart(let otherMap, _):
            return otherMap
                ? "Dein Standort liegt nicht auf derselben Karte wie das Ziel. Tippe auf die Karte oder suche oben, wo die Route beginnen soll."
                : "Wo soll die Route beginnen? Tippe auf die Karte oder suche oben einen Ort."
        case .idle:
            return ""
        }
    }

    @ViewBuilder private func button(
        _ title: String, identifier: String, primary: Bool = false, action: @escaping () -> Void
    ) -> some View {
        if primary {
            Button(title, action: action)
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier(identifier)
        } else {
            Button(title, action: action)
                .buttonStyle(.bordered)
                .accessibilityIdentifier(identifier)
        }
    }
}
