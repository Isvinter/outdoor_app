//! Editing a route plan: the rules of the desktop's route panel
//! (`apps/base/ui/src/route.ts`), in the core for the mobile apps.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_app::{LegError, LegSpec, PlanRequest};
use core_routing::{LatLon, Profile, RouteOptions};

fn at(i: usize) -> LatLon {
    LatLon {
        lat: 47.6 + i as f64 * 0.01,
        lon: 11.5 + i as f64 * 0.01,
    }
}

fn leg(profile: Profile) -> LegSpec {
    LegSpec {
        profile,
        options: RouteOptions::default(),
    }
}

/// Points 0..n, legs in `profiles` (one less than points).
fn plan(profiles: &[Profile], stage_ends: &[usize]) -> PlanRequest {
    PlanRequest {
        waypoints: (0..=profiles.len()).map(at).collect(),
        legs: profiles.iter().copied().map(leg).collect(),
        stage_ends: stage_ends.to_vec(),
    }
}

fn profiles(request: &PlanRequest) -> Vec<Profile> {
    request.legs.iter().map(|l| l.profile).collect()
}

fn valid(request: &PlanRequest) {
    if request.waypoints.len() >= 2 {
        request.validate().unwrap();
    } else {
        assert!(request.legs.is_empty() && request.stage_ends.is_empty());
    }
}

use Profile::{Car, Hike, Manual, Mtb};

#[test]
fn add_appends_leg_from_form() {
    let empty = PlanRequest::empty();
    assert!(empty.waypoints.is_empty() && empty.legs.is_empty());
    let one = empty.add_waypoint(at(0), leg(Hike));
    assert_eq!((one.waypoints.len(), one.legs.len()), (1, 0));
    let two = one.add_waypoint(at(1), leg(Mtb));
    let three = two.add_waypoint(at(2), leg(Car));
    assert_eq!(three.waypoints, vec![at(0), at(1), at(2)]);
    assert_eq!(profiles(&three), vec![Mtb, Car]);
    assert_eq!(one.waypoints.len(), 1, "the original is not changed");
    for r in [&one, &two, &three] {
        valid(r);
    }
}

#[test]
fn remove_inner_joins_with_leg_before() {
    let four = plan(&[Hike, Mtb, Car], &[]);
    let removed = four.remove_waypoint(2);
    assert_eq!(removed.waypoints, vec![at(0), at(1), at(3)]);
    // The leg before the point (mtb) now joins its neighbours.
    assert_eq!(profiles(&removed), vec![Hike, Mtb]);
    assert_eq!(profiles(&four.remove_waypoint(1)), vec![Hike, Car]);
    assert_eq!(profiles(&four.remove_waypoint(3)), vec![Hike, Mtb]);
    // The first point takes its leg along.
    assert_eq!(profiles(&four.remove_waypoint(0)), vec![Mtb, Car]);
    assert_eq!(four.remove_waypoint(4), four, "out of range");
    for i in 0..4 {
        valid(&four.remove_waypoint(i));
    }
}

#[test]
fn removing_down_to_nothing() {
    let two = plan(&[Hike], &[]);
    let one = two.remove_waypoint(1);
    assert_eq!((one.waypoints, one.legs.len()), (vec![at(0)], 0));
    let none = two.remove_waypoint(0).remove_waypoint(0);
    assert_eq!(none, PlanRequest::empty());
}

#[test]
fn remove_shifts_stage_ends() {
    let five = plan(&[Hike, Hike, Hike, Hike], &[1, 3]);
    assert_eq!(five.remove_waypoint(1).stage_ends, vec![2]);
    assert_eq!(five.remove_waypoint(2).stage_ends, vec![1, 2]);
    valid(&five.remove_waypoint(1));
    valid(&five.remove_waypoint(2));
}

#[test]
fn stage_ends_dropped_when_point_becomes_last() {
    let three = plan(&[Hike, Hike], &[1]);
    assert_eq!(three.remove_waypoint(2).stage_ends, Vec::<usize>::new());
    valid(&three.remove_waypoint(2));
}

#[test]
fn stage_end_only_inner() {
    let five = plan(&[Hike, Hike, Hike, Hike], &[]);
    assert_eq!(five.toggle_stage_end(0), five);
    assert_eq!(five.toggle_stage_end(4), five);
    assert_eq!(five.toggle_stage_end(9), five);
    let marked = five.toggle_stage_end(3).toggle_stage_end(1);
    assert_eq!(marked.stage_ends, vec![1, 3]);
    valid(&marked);
    assert_eq!(marked.toggle_stage_end(3).toggle_stage_end(1), five);
}

#[test]
fn move_changes_one_point() {
    let three = plan(&[Hike, Mtb], &[1]);
    let moved = three.move_waypoint(1, at(7));
    assert_eq!(moved.waypoints, vec![at(0), at(7), at(2)]);
    assert_eq!(
        (profiles(&moved), moved.stage_ends.clone()),
        (vec![Hike, Mtb], vec![1])
    );
    assert_eq!(three.move_waypoint(5, at(7)), three);
}

#[test]
fn set_last_leg_changes_only_last() {
    let three = plan(&[Hike, Mtb], &[]);
    assert_eq!(
        profiles(&three.set_last_leg(leg(Manual))),
        vec![Hike, Manual]
    );
    let one = PlanRequest::empty().add_waypoint(at(0), leg(Hike));
    assert_eq!(one.set_last_leg(leg(Manual)), one);
}

#[test]
fn set_leg_changes_only_that_leg() {
    let request = plan(&[Profile::Hike, Profile::Mtb, Profile::Car], &[1]);
    let next = request.set_leg(1, leg(Profile::Manual));
    assert_eq!(
        profiles(&next),
        [Profile::Hike, Profile::Manual, Profile::Car]
    );
    assert_eq!(next.waypoints, request.waypoints);
    assert_eq!(next.stage_ends, request.stage_ends);
    valid(&next);
}

#[test]
fn set_leg_outside_the_route_changes_nothing() {
    let request = plan(&[Profile::Hike], &[]);
    assert_eq!(request.set_leg(5, leg(Profile::Manual)), request);
}

#[test]
fn append_keeps_order_and_existing_legs() {
    let one_point = PlanRequest {
        waypoints: vec![at(0)],
        legs: Vec::new(),
        stage_ends: Vec::new(),
    };
    let next = one_point.add_waypoint(at(9), leg(Profile::Car));
    assert_eq!(next.waypoints, [at(0), at(9)]);
    assert_eq!(profiles(&next), [Profile::Car]);

    let request = plan(&[Profile::Hike, Profile::Mtb], &[1]);
    let next = request.add_waypoint(at(9), leg(Profile::Car));
    assert_eq!(next.waypoints, [at(0), at(1), at(2), at(9)]);
    assert_eq!(profiles(&next), [Profile::Hike, Profile::Mtb, Profile::Car]);
    assert_eq!(next.stage_ends, [1]);
}

#[test]
fn straight_line_only_for_missing_data_or_no_route() {
    let error = |code: &str| LegError {
        code: code.to_string(),
        message: String::new(),
    };
    assert!(error("routing_unavailable").can_connect_straight());
    assert!(error("route_not_found").can_connect_straight());
    assert!(!error("routing_incompatible").can_connect_straight());
    assert!(!error("routing").can_connect_straight());
}
