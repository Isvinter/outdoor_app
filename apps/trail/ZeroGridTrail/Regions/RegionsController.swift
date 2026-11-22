import Foundation

/// German texts for the region manager.
enum RegionTexts {
    /// The text for a failure of the core; the core's own message for a code
    /// without a translation.
    static func error(_ error: Error) -> String {
        guard case CoreError.Failed(let code, let message) = error else { return error.localizedDescription }
        switch code {
        case "region_download": return "Laden fehlgeschlagen – läuft die Freigabe am Desktop noch? (\(message))"
        case "region_checksum": return "Die geladene Datei ist beschädigt – bitte erneut laden"
        case "region_busy": return "Die Region wird bereits geladen"
        case "region_unknown": return "Der Desktop bietet diese Region nicht mehr an"
        case "region_not_installed": return "Die Region ist nicht installiert"
        case "region_manifest": return "Die Beschreibung der Region ist ungültig"
        case "region_io": return "Kartendaten können nicht gelesen oder geschrieben werden (\(message))"
        case "no_catalog":
            return "Noch keine Adresse des Desktops – unter „Regionen“ den QR-Code scannen oder die Adresse eingeben"
        case "route_import": return "Die Route vom Desktop kann nicht gelesen werden"
        default: return PlanTexts.error(code: code, fallback: message)
        }
    }
}

/// The regions on the iPhone and those the desktop offers, as the screen sees
/// them. The core compares the catalog, keeps the states and does the work;
/// this says which change the user asked for and shows the outcome.
@MainActor
final class RegionsController: ObservableObject {
    /// How far a download is, in bytes; `total` is 0 until the core has said.
    struct Download: Equatable {
        var done: UInt64 = 0
        var total: UInt64 = 0

        var percent: Int {
            total > 0 ? Int(min(done * 100 / total, 100)) : 0
        }
    }

    private let host: CoreHost
    private let engine: ValhallaMobileEngine
    private let map: MapController
    private let plan: PlanController

    /// The regions on the iPhone, active or not.
    @Published private(set) var installed: [RegionSummary]
    /// What the last reading of the catalog gave; nil before the first.
    @Published private(set) var list: RegionList?
    /// The address of the desktop's share, as the core stored it; empty if none.
    @Published private(set) var source: String
    /// The catalog is being read.
    @Published private(set) var reading = false
    /// The running downloads, by region.
    @Published private(set) var downloads: [String: Download] = [:]
    /// A failure worth telling the user.
    @Published var error: String?
    /// The screen "Regionen" is open.
    @Published var showing = false

    /// Deleting and switching, and every reading of the installed regions,
    /// one after the other: what the screen shows last is what was read last.
    private let changes = DispatchQueue(label: "de.zeibig.zerogrid.trail.regions", qos: .userInitiated)
    /// Counts the readings of the catalog, so that a slow one cannot replace a newer one.
    private var reads = 0
    /// The download started last and where its first report stood; for the self-test.
    private var lastDownload: (id: String, startPercent: Int?)?

    init(
        host: CoreHost, engine: ValhallaMobileEngine, map: MapController, plan: PlanController,
        installed: [RegionSummary]
    ) {
        self.host = host
        self.engine = engine
        self.map = map
        self.plan = plan
        self.installed = installed
        source = (try? host.core.catalogSource()) ?? ""
        host.watchRegionProgress { [weak self] region, done, total in
            DispatchQueue.main.async { self?.progress(region, done: done, total: total) }
        }
    }

    // MARK: What the screen shows

    /// The regions of the catalog that are not on the iPhone.
    var offered: [RegionEntry] {
        (list?.regions ?? []).filter { entry in !installed.contains { $0.id == entry.id } }
    }

    /// Whether the desktop has a newer version of an installed region.
    func updateAvailable(_ id: String) -> Bool {
        list?.regions.first { $0.id == id }?.state == "update_available"
    }

    // MARK: The catalog

    /// Reads the catalog, off the main thread. Only the screen "Regionen"
    /// asks for it: when it opens, when it is pulled down, after a download.
    func refresh() {
        Task { await read() }
    }

    func read() async {
        reads += 1
        let number = reads
        reading = true
        let core = host.core
        let result = await Task.detached(priority: .userInitiated) { Result { try core.regions() } }.value
        guard number == reads else { return }
        reading = false
        switch result {
        case .success(let list): show(list)
        case .failure(let error): self.error = RegionTexts.error(error)
        }
    }

    private func show(_ list: RegionList) {
        self.list = list
        source = list.source
    }

    /// Stores the address of the desktop's share (the core puts it into
    /// shape) and reads the catalog there.
    func setSource(_ text: String) {
        do {
            source = try host.core.setCatalogSource(source: text)
            list = nil
            refresh()
        } catch {
            self.error = RegionTexts.error(error)
        }
    }

    // MARK: Changes

    /// Downloads a region and installs it; the same for an update.
    func install(_ id: String) {
        guard downloads[id] == nil else { return }
        downloads[id] = Download()
        lastDownload = (id, nil)
        error = nil
        let core = host.core
        let engine = engine
        let tiles = host.routingTiles(region: id)
        Task.detached(priority: .userInitiated) { [weak self] in
            let result = Result { try core.installRegion(id: id) }
            // An update put new routing data where the open instance reads.
            // At once, before the catalog is read: that may take long, and the
            // user may plan meanwhile. (The core asks for the same when it
            // opens the region's router anew; this also frees the old file.)
            engine.forget(tilesPath: tiles)
            // The states of the catalog changed with it; the desktop was just there.
            let list = (try? result.get()) == nil ? nil : try? core.regions()
            await self?.finished(id, result, list: list)
        }
    }

    private func finished(_ id: String, _ result: Result<RegionSummary, Error>, list: RegionList?) {
        // Reports of the download are on their way to the main thread before this.
        downloads[id] = nil
        switch result {
        case .success:
            reload { [weak self] in
                if let list { self?.show(list) }
            }
        case .failure(CoreError.Failed("region_cancelled", _)):
            // The user stopped it; what was loaded stays for the next time.
            break
        case .failure(let error):
            self.error = RegionTexts.error(error)
        }
    }

    private func progress(_ region: String, done: UInt64, total: UInt64) {
        guard downloads[region] != nil else { return }
        let download = Download(done: done, total: total)
        downloads[region] = download
        if lastDownload?.id == region, lastDownload?.startPercent == nil, total > 0 {
            lastDownload?.startPercent = download.percent
        }
    }

    /// Stops a download; what is there stays, and the next one goes on from it.
    func cancel(_ id: String) {
        host.core.cancelRegionDownload(id: id)
    }

    func delete(_ id: String) {
        change({ core in try core.deleteRegion(id: id) }, then: { [weak self] in
            guard let self else { return }
            self.engine.forget(tilesPath: self.host.routingTiles(region: id))
        })
    }

    /// A region that is not active stays on the iPhone, but the map, the
    /// elevations and the routing leave it out.
    func setEnabled(_ id: String, _ enabled: Bool) {
        if let index = installed.firstIndex(where: { $0.id == id }) {
            // The switch follows the finger; the core's answer confirms it.
            installed[index].enabled = enabled
        }
        change({ core in try core.setRegionEnabled(id: id, enabled: enabled) }, then: {})
    }

    private func change(_ work: @escaping @Sendable (CoreApp) throws -> Void, then: @escaping @MainActor () -> Void) {
        let core = host.core
        error = nil
        changes.async { [weak self] in
            let result = Result { try work(core) }
            DispatchQueue.main.async {
                guard let self else { return }
                if case .failure(let error) = result {
                    self.error = RegionTexts.error(error)
                } else {
                    then()
                }
                self.reload {}
            }
        }
    }

    /// Reads what is installed now (never the catalog) and makes the map and
    /// the planned route follow.
    private func reload(then: @escaping @MainActor () -> Void) {
        let core = host.core
        changes.async { [weak self] in
            let result = Result { try core.overview() }
            DispatchQueue.main.async {
                guard let self else { return }
                switch result {
                case .success(let overview):
                    self.installed = overview.regions
                    self.map.reloadRegions(bounds: overview.bounds)
                    self.plan.regionsChanged()
                case .failure(let error):
                    self.error = RegionTexts.error(error)
                }
                then()
            }
        }
    }

    // MARK: Self-test

    /// What the UI tests read about the regions, as `key=value` pairs.
    var status: [String] {
        var parts = ["regions=\(installed.map { "\($0.id):\($0.enabled ? 1 : 0)" }.joined(separator: ","))"]
        if let list {
            parts.append("catalog=\(list.regions.count)")
            parts.append("catalog_error=\(list.catalogError == nil ? 0 : 1)")
        }
        let running = downloads.keys.sorted().first
        parts.append("downloading=\(running ?? "-")")
        if let running, let download = downloads[running] {
            parts.append("download_pct=\(download.percent)")
        }
        if let start = lastDownload?.startPercent {
            parts.append("download_start_pct=\(start)")
        }
        return parts
    }
}
