import CoreLocation
import SwiftUI

/// The search above the map: the field, the quick choices, the list of hits and
/// the card of the chosen one. The map stays visible below it.
struct SearchBar: View {
    @ObservedObject var search: SearchController
    @ObservedObject var flow: RouteHereFlow
    @ObservedObject var location: LocationService
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
                ResultCard(hit: hit, search: search, flow: flow, location: location)
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
                routesToggle
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

    /// How the hits are measured: as the crow flies or by the way from the own position.
    private var routesToggle: some View {
        HStack(spacing: 2) {
            distanceSegment("Luft", icon: "line.diagonal", label: "Luftlinie", on: false, id: "search-air")
            distanceSegment(
                "Weg", icon: "point.topleft.down.to.point.bottomright.curvepath", label: "Weg", on: true,
                id: "search-routes")
        }
        .padding(2)
        .background(Color(.secondarySystemFill), in: Capsule())
        .fixedSize()
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Entfernung")
    }

    private func distanceSegment(_ title: String, icon: String, label: String, on: Bool, id: String) -> some View {
        let chosen = search.routes == on
        return Button {
            search.setRoutes(on)
        } label: {
            Label(title, systemImage: icon)
                .labelStyle(.titleAndIcon)
                .font(.caption.weight(chosen ? .semibold : .regular))
                .foregroundStyle(chosen ? Color.white : Color.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background(chosen ? Color.accentColor : Color.clear, in: Capsule())
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .accessibilityIdentifier(id)
        .accessibilityLabel(label)
        .accessibilityAddTraits(chosen ? .isSelected : [])
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
                let hits = search.results?.hits ?? []
                ForEach(Array(hits.enumerated()), id: \.offset) { index, hit in
                    if hit.outside && (index == 0 || !hits[index - 1].outside) {
                        Text("Außerhalb des Bereichs")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .padding(.top, 10)
                            .padding(.bottom, 2)
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("search-outside")
                    }
                    Button {
                        search.select(hit)
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(SearchTexts.title(hit))
                                .foregroundStyle(Color.primary)
                            Text(SearchTexts.details(hit))
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
