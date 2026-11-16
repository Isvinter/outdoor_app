import SwiftUI

/// The regions on the iPhone (switch on and off, update, delete) and those
/// the desktop offers in the home network (load).
struct RegionsScreen: View {
    @ObservedObject var regions: RegionsController
    @State private var deleting: RegionSummary?
    @State private var typing = false
    @State private var address = ""

    var body: some View {
        NavigationStack {
            List {
                if let error = regions.error {
                    Text(error)
                        .font(.callout)
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("regions-error")
                        .onTapGesture { regions.error = nil }
                }
                Section("Auf dem iPhone") {
                    if regions.installed.isEmpty {
                        Text("Noch keine Region geladen")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    ForEach(regions.installed, id: \.id) { region in
                        installedRow(region)
                    }
                }
                Section("Vom Desktop") {
                    sourceRow
                    if let failure = regions.list?.catalogError {
                        unreachable(failure)
                    }
                    ForEach(regions.offered, id: \.id) { entry in
                        offeredRow(entry)
                    }
                }
            }
            // Only this and opening the screen read the catalog.
            .refreshable { await regions.read() }
            .navigationTitle("Regionen")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Fertig") { regions.showing = false }
                        .accessibilityIdentifier("regions-done")
                }
            }
        }
        .onAppear { regions.refresh() }
        .alert(
            "Region löschen?",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { region in
            Button("Löschen", role: .destructive) { regions.delete(region.id) }
            Button("Abbrechen", role: .cancel) {}
        } message: { region in
            Text("„\(region.name)“ wird vom iPhone entfernt (\(Format.bytes(region.bytes))). "
                + "Vom Desktop lässt sie sich wieder laden.")
        }
        .alert("Adresse des Desktops", isPresented: $typing) {
            TextField("192.168.178.20:8765", text: $address)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Übernehmen") { regions.setSource(address) }
            Button("Abbrechen", role: .cancel) {}
        } message: {
            Text("Sie steht am Desktop unter Karten → Im Heimnetz freigeben.")
        }
    }

    // MARK: On the iPhone

    private func installedRow(_ region: RegionSummary) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(region.name)
                    Text("\(Format.bytes(region.bytes)) · Stand \(Format.regionVersion(region.version))")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Toggle("Aktiv", isOn: Binding(
                    get: { region.enabled }, set: { regions.setEnabled(region.id, $0) }))
                    .labelsHidden()
                    .accessibilityIdentifier("region-enabled-\(region.id)")
                    .accessibilityLabel("\(region.name) aktiv")
            }
            if let download = regions.downloads[region.id] {
                progress(region.id, download)
            } else if regions.updateAvailable(region.id) {
                Button("Aktualisieren") { regions.install(region.id) }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("region-update-\(region.id)")
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            // Not with the destructive role: the row stays until the question is answered.
            Button("Löschen") { deleting = region }
                .tint(.red)
        }
        .contextMenu {
            Button("Löschen", role: .destructive) { deleting = region }
        }
    }

    // MARK: From the desktop

    @ViewBuilder private var sourceRow: some View {
        VStack(alignment: .leading, spacing: 8) {
            if regions.source.isEmpty {
                Text("Noch keine Adresse. Sie steht am Desktop unter Karten → Im Heimnetz freigeben.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            } else {
                HStack {
                    Text(regions.source)
                        .font(.callout.monospaced())
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                        .accessibilityIdentifier("catalog-source")
                    Spacer()
                    if regions.reading { ProgressView() }
                }
            }
            Button("Adresse eingeben") {
                address = regions.source
                typing = true
            }
            .buttonStyle(.bordered)
            .accessibilityIdentifier("source-type")
        }
    }

    private func unreachable(_ failure: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Desktop nicht erreichbar: \(failure)")
                .font(.callout)
                .accessibilityIdentifier("catalog-error")
            Text("Ist „Im Heimnetz freigeben“ am Desktop eingeschaltet, und sind beide Geräte im selben WLAN?")
                .font(.footnote)
                .foregroundStyle(.secondary)
            Button("Erneut versuchen") { regions.refresh() }
                .buttonStyle(.bordered)
                .disabled(regions.reading)
                .accessibilityIdentifier("catalog-retry")
        }
    }

    private func offeredRow(_ entry: RegionEntry) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(entry.name)
                    Text("\(Format.bytes(entry.size)) · Stand \(Format.regionVersion(entry.version))")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if regions.downloads[entry.id] == nil {
                    Button("Laden") { regions.install(entry.id) }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("region-load-\(entry.id)")
                }
            }
            if let download = regions.downloads[entry.id] {
                progress(entry.id, download)
            }
        }
    }

    /// The bar, the bytes and the button that stops the download.
    private func progress(_ id: String, _ download: RegionsController.Download) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                if download.total > 0 {
                    ProgressView(value: Double(download.done), total: Double(download.total))
                    Text("\(Format.bytes(download.done)) von \(Format.bytes(download.total))")
                        .font(.footnote)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                } else {
                    ProgressView()
                }
            }
            Button("Abbrechen") { regions.cancel(id) }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("region-cancel-\(id)")
        }
    }
}
