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

    /// Measures the hits by the way from the own position.
    private var routesToggle: some View {
        Button {
            search.toggleRoutes()
        } label: {
            Label("Weg", systemImage: "point.topleft.down.to.point.bottomright.curvepath")
                .labelStyle(.titleAndIcon)
                .font(.footnote.weight(search.routes ? .semibold : .regular))
                .foregroundStyle(search.routes ? Color.white : Color.primary)
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .background(search.routes ? Color.accentColor : Color(.secondarySystemFill), in: Capsule())
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .accessibilityIdentifier("search-routes")
        .accessibilityAddTraits(search.routes ? .isSelected : [])
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
