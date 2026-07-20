use rutile::terminal::broadcast::{BroadcastGroup, BroadcastManager};

#[test]
fn none_group_targets_nothing() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(1, 11);
    mgr.set_group(BroadcastGroup::None);

    assert!(mgr.targets(10, 1).is_empty());
}

#[test]
fn session_group_targets_only_same_session_excluding_origin() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(1, 11);
    mgr.register_pane(2, 20);
    mgr.set_group(BroadcastGroup::Session);

    let mut targets = mgr.targets(10, 1);
    targets.sort();
    assert_eq!(targets, vec![11]);
}

#[test]
fn session_group_never_reaches_other_sessions() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(2, 20);
    mgr.set_group(BroadcastGroup::Session);

    assert!(mgr.targets(10, 1).is_empty());
    assert!(mgr.targets(20, 2).is_empty());
}

#[test]
fn switching_group_at_runtime_changes_targets() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(1, 11);

    mgr.set_group(BroadcastGroup::None);
    assert!(mgr.targets(10, 1).is_empty());

    mgr.set_group(BroadcastGroup::Session);
    assert_eq!(mgr.targets(10, 1), vec![11]);
}

#[test]
fn unregister_pane_removes_it_from_targets() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(1, 11);
    mgr.set_group(BroadcastGroup::Session);

    mgr.unregister_pane(1, 11);
    assert!(mgr.targets(10, 1).is_empty());
}

#[test]
fn excluded_pane_is_not_a_target() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(1, 11);
    mgr.register_pane(1, 12);
    mgr.set_group(BroadcastGroup::Session);

    mgr.toggle_excluded(11);

    let mut targets = mgr.targets(10, 1);
    targets.sort();
    assert_eq!(targets, vec![12]);
}

#[test]
fn excluded_origin_broadcasts_nothing() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(1, 11);
    mgr.set_group(BroadcastGroup::Session);

    mgr.toggle_excluded(10);
    assert!(mgr.targets(10, 1).is_empty());
}

#[test]
fn toggle_excluded_flips_state_and_can_be_re_included() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.register_pane(1, 11);
    mgr.set_group(BroadcastGroup::Session);

    assert!(mgr.toggle_excluded(11));
    assert!(mgr.is_excluded(11));
    assert!(mgr.targets(10, 1).is_empty());

    assert!(!mgr.toggle_excluded(11));
    assert!(!mgr.is_excluded(11));
    assert_eq!(mgr.targets(10, 1), vec![11]);
}

#[test]
fn unregister_pane_clears_its_exclusion() {
    let mut mgr = BroadcastManager::new();
    mgr.register_pane(1, 10);
    mgr.toggle_excluded(10);
    assert!(mgr.is_excluded(10));

    mgr.unregister_pane(1, 10);
    assert!(!mgr.is_excluded(10));
}
