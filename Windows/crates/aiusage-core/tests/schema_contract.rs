use aiusage_core::{phase_a_snapshot, product_surfaces, FeatureStatus};
use aiusage_fixtures::load_desktop_snapshot_fixture;

#[test]
fn phase_a_snapshot_covers_all_primary_surfaces() {
    let surfaces = product_surfaces();
    assert_eq!(surfaces.len(), 10);
    assert!(surfaces
        .iter()
        .all(|surface| surface.status == FeatureStatus::FoundationReady));
}

#[test]
fn fixture_round_trips_against_core_schema() {
    let fixture = load_desktop_snapshot_fixture("phase_a_snapshot.json")
        .expect("fixture should decode as DesktopSnapshot");
    assert_eq!(fixture, phase_a_snapshot());
}
