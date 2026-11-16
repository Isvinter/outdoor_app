#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};

use common::{open, position, temp_dir};
use core_app::{App, ImportOutcome, LegSpec, PlanRequest, RemoteRouteState};
use core_routing::{Profile, RouteOptions};
use uuid::Uuid;

fn request(points: &[(f64, f64)]) -> PlanRequest {
    PlanRequest {
        waypoints: points.iter().map(|&(c, r)| position(c, r)).collect(),
        legs: vec![
            LegSpec {
                profile: Profile::Manual,
                options: RouteOptions::default(),
            };
            points.len() - 1
        ],
        stage_ends: Vec::new(),
    }
}

/// The desktop and the phone, and the directory the desktop shares.
struct Pair {
    desktop: App,
    phone: App,
    share: PathBuf,
}

fn pair(name: &str) -> Pair {
    let desktop = open(&temp_dir(&format!("{name}-desktop")));
    let phone = open(&temp_dir(&format!("{name}-phone")));
    let share = temp_dir(&format!("{name}-share"));
    phone.set_catalog_source(share.to_str().unwrap()).unwrap();
    Pair {
        desktop,
        phone,
        share,
    }
}

impl Pair {
    /// Writes what the desktop shares now.
    fn publish(&self) {
        write_share(&self.desktop, &self.share);
    }

    fn plan_on_desktop(&self, points: &[(f64, f64)], name: &str) -> Uuid {
        self.desktop.plan_route(&request(points)).unwrap();
        self.desktop.save_planned_route(name).unwrap().id
    }

    fn states(&self) -> Vec<RemoteRouteState> {
        self.phone
            .remote_routes()
            .unwrap()
            .iter()
            .map(|r| r.state)
            .collect()
    }
}

fn write_share(desktop: &App, share: &Path) {
    let _ = std::fs::remove_dir_all(share.join("routes"));
    std::fs::create_dir_all(share.join("routes")).unwrap();
    let list = desktop.shared_routes().unwrap();
    std::fs::write(
        share.join("routes.json"),
        serde_json::to_string(&list).unwrap(),
    )
    .unwrap();
    for info in list {
        let route = desktop.shared_route(info.id).unwrap();
        std::fs::write(
            share.join(format!("routes/{}.json", info.id)),
            serde_json::to_string(&route).unwrap(),
        )
        .unwrap();
    }
}

const TWO: [(f64, f64); 2] = [(10.0, 32.0), (50.0, 32.0)];

#[test]
fn new_route_is_added_with_same_id() {
    let p = pair("ri-new");
    let id = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    assert_eq!(p.states(), [RemoteRouteState::New]);
    assert_eq!(
        p.phone.import_route(id).unwrap(),
        (ImportOutcome::Added, id)
    );
    assert_eq!(p.states(), [RemoteRouteState::UpToDate]);
    assert_eq!(p.phone.saved_routes().unwrap()[0].id, id);
}

#[test]
fn desktop_change_replaces_unchanged() {
    let p = pair("ri-replace");
    let id = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    p.phone.import_route(id).unwrap();
    p.desktop.rename_route(id, "Tour neu").unwrap();
    p.publish();
    assert_eq!(p.states(), [RemoteRouteState::DesktopNewer]);
    assert_eq!(
        p.phone.import_route(id).unwrap(),
        (ImportOutcome::Replaced, id)
    );
    assert_eq!(p.phone.saved_routes().unwrap()[0].name, "Tour neu");
    assert_eq!(p.states(), [RemoteRouteState::UpToDate]);
}

#[test]
fn iphone_change_makes_copy() {
    let p = pair("ri-copy");
    let id = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    p.phone.import_route(id).unwrap();
    p.phone.rename_route(id, "Mein Name").unwrap();
    assert_eq!(p.states(), [RemoteRouteState::ChangedHere]);
    let (outcome, other) = p.phone.import_route(id).unwrap();
    assert_eq!(outcome, ImportOutcome::Copied);
    assert_ne!(other, id);
    let routes = p.phone.saved_routes().unwrap();
    assert_eq!(routes.len(), 2);
    let copy = routes.iter().find(|r| r.id == other).unwrap();
    assert!(copy.name.ends_with(" (Desktop)"));
    assert_eq!(
        routes.iter().find(|r| r.id == id).unwrap().name,
        "Mein Name"
    );
}

#[test]
fn deleted_here_is_restored() {
    let p = pair("ri-restore");
    let id = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    p.phone.import_route(id).unwrap();
    p.phone.delete_route(id).unwrap();
    assert_eq!(p.states(), [RemoteRouteState::DeletedHere]);
    assert_eq!(
        p.phone.import_route(id).unwrap(),
        (ImportOutcome::Restored, id)
    );
    assert_eq!(p.phone.saved_routes().unwrap().len(), 1);
}

#[test]
fn import_replaces_loaded_route_in_memory() {
    let p = pair("ri-loaded");
    let id = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    p.phone.import_route(id).unwrap();
    p.phone.saved_route(id).unwrap();
    assert_eq!(p.phone.planned_route().unwrap().request.waypoints.len(), 2);

    p.desktop
        .plan_route(&request(&[(10.0, 32.0), (30.0, 20.0), (50.0, 32.0)]))
        .unwrap();
    p.desktop.save_planned_route("Tour").unwrap();
    p.publish();
    p.phone.import_route(id).unwrap();
    assert_eq!(p.phone.planned_route().unwrap().request.waypoints.len(), 3);
    assert_eq!(p.phone.loaded_route(), Some(id));

    p.phone.save_planned_route("Tour").unwrap();
    assert_eq!(p.phone.planned_route().unwrap().request.waypoints.len(), 3);
    p.phone.saved_route(id).unwrap();
    assert_eq!(p.phone.planned_route().unwrap().request.waypoints.len(), 3);
}

#[test]
fn mismatched_id_is_rejected() {
    let p = pair("ri-mismatch");
    let a = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    let b = Uuid::now_v7();
    let file = p.share.join(format!("routes/{a}.json"));
    let text = std::fs::read_to_string(&file)
        .unwrap()
        .replace(&a.to_string(), &b.to_string());
    std::fs::write(&file, text).unwrap();
    let error = p.phone.import_route(a).unwrap_err();
    assert_eq!(error.code(), "route_import");
    assert!(p.phone.saved_routes().unwrap().is_empty());
}

#[test]
fn broken_plan_is_rejected() {
    let p = pair("ri-broken");
    let a = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    let file = p.share.join(format!("routes/{a}.json"));
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    value["plan"] = serde_json::json!({"x": 1});
    std::fs::write(&file, value.to_string()).unwrap();
    assert_eq!(p.phone.import_route(a).unwrap_err().code(), "route_import");
    assert!(p.phone.saved_routes().unwrap().is_empty());
}

#[test]
fn missing_route_file_is_the_fetch_error() {
    let p = pair("ri-missing");
    let a = p.plan_on_desktop(&TWO, "Tour");
    p.publish();
    std::fs::remove_file(p.share.join(format!("routes/{a}.json"))).unwrap();
    assert_ne!(p.phone.import_route(a).unwrap_err().code(), "route_import");
}

#[test]
fn no_source_fails() {
    let phone = open(&temp_dir("ri-nosource"));
    assert_eq!(phone.remote_routes().unwrap_err().code(), "no_catalog");
    assert_eq!(
        phone.import_route(Uuid::now_v7()).unwrap_err().code(),
        "no_catalog"
    );
}
