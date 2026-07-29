use rutile::layout::{Direction, Orientation, SplitTree};

#[test]
fn remap_ids_produces_fresh_unique_ids_and_preserves_shape() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    tree.split(1, Orientation::Vertical, 3, 103);

    let mut next = 1000;
    let (remapped, pane_map) = tree.remap_ids(&mut || {
        let id = next;
        next += 1;
        id
    });

    // Same shape (leaf order, orientation), just renumbered.
    assert_eq!(remapped.leaves().len(), 3);
    let old_leaves = tree.leaves();
    let new_leaves = remapped.leaves();
    assert_eq!(old_leaves.len(), new_leaves.len());
    for (old, new) in old_leaves.iter().zip(new_leaves.iter()) {
        assert_eq!(pane_map[old], *new);
    }

    // Every id handed out (leaves + internal split nodes) is unique and
    // came from the counter, not reused from the original tree.
    let mut all_new_ids: Vec<u64> = new_leaves.clone();
    fn collect_split_ids(tree: &SplitTree, out: &mut Vec<u64>) {
        if let SplitTree::Split {
            id, left, right, ..
        } = tree
        {
            out.push(*id);
            collect_split_ids(left, out);
            collect_split_ids(right, out);
        }
    }
    collect_split_ids(&remapped, &mut all_new_ids);
    assert!(all_new_ids.iter().all(|id| *id >= 1000));
    let unique: std::collections::HashSet<_> = all_new_ids.iter().collect();
    assert_eq!(unique.len(), all_new_ids.len());
}

#[test]
fn tree_round_trips_through_toml() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    tree.set_ratio(102, 0.3);

    let serialized = toml::to_string(&tree).expect("SplitTree must serialize");
    let deserialized: SplitTree = toml::from_str(&serialized).expect("must deserialize back");

    assert_eq!(deserialized.leaves(), tree.leaves());
    let rects: std::collections::HashMap<_, _> = deserialized.leaf_rects().into_iter().collect();
    let original_rects: std::collections::HashMap<_, _> = tree.leaf_rects().into_iter().collect();
    assert_eq!(rects[&1].w, original_rects[&1].w);
}

#[test]
fn split_creates_two_leaves_with_correct_orientation() {
    let mut tree = SplitTree::new_leaf(1);
    assert!(tree.split(1, Orientation::Horizontal, 2, 102));

    match &tree {
        SplitTree::Split {
            orientation,
            left,
            right,
            ..
        } => {
            assert_eq!(*orientation, Orientation::Horizontal);
            assert!(matches!(**left, SplitTree::Leaf(1)));
            assert!(matches!(**right, SplitTree::Leaf(2)));
        }
        _ => panic!("expected a Split node"),
    }
}

#[test]
fn set_ratio_updates_the_targeted_split_only() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    tree.split(1, Orientation::Vertical, 3, 103);

    assert!(tree.set_ratio(103, 0.75));
    assert!(!tree.set_ratio(999, 0.3));

    let rects: std::collections::HashMap<_, _> = tree.leaf_rects().into_iter().collect();
    // The inner (103) split moved to 0.75; the outer (102) split, untouched,
    // stays at the default 0.5 — so leaf 1 (top of the inner split) now
    // occupies a taller slice of the left half than leaf 3.
    assert!(rects[&1].h > rects[&3].h);
}

#[test]
fn set_ratio_clamps_to_avoid_collapsing_a_pane() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);

    tree.set_ratio(102, 5.0);
    let rects: std::collections::HashMap<_, _> = tree.leaf_rects().into_iter().collect();
    assert!(rects[&2].w > 0.0);
}

#[test]
fn split_on_unknown_target_is_noop() {
    let mut tree = SplitTree::new_leaf(1);
    assert!(!tree.split(99, Orientation::Horizontal, 2, 102));
    assert!(matches!(tree, SplitTree::Leaf(1)));
}

#[test]
fn close_collapses_to_sibling() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    assert!(tree.close(2));
    assert!(matches!(tree, SplitTree::Leaf(1)));
}

#[test]
fn close_last_leaf_is_noop() {
    let mut tree = SplitTree::new_leaf(1);
    assert!(!tree.close(1));
    assert!(matches!(tree, SplitTree::Leaf(1)));
}

#[test]
fn leaves_respects_left_to_right_order() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    tree.split(2, Orientation::Vertical, 3, 103);

    assert_eq!(tree.leaves(), vec![1, 2, 3]);
}

#[test]
fn nested_splits_produce_correct_shape() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    tree.split(1, Orientation::Vertical, 3, 103);
    tree.split(2, Orientation::Vertical, 4, 104);

    // Leaves should be: (1 top, 3 bottom) on the left, (2 top, 4 bottom) on the right.
    assert_eq!(tree.leaves(), vec![1, 3, 2, 4]);
}

#[test]
fn leaf_rects_grid_2x2() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    tree.split(1, Orientation::Vertical, 3, 103);
    tree.split(2, Orientation::Vertical, 4, 104);

    let rects: std::collections::HashMap<_, _> = tree.leaf_rects().into_iter().collect();

    // 1 = top-left, 3 = bottom-left, 2 = top-right, 4 = bottom-right.
    assert!(rects[&1].x < 0.5 && rects[&1].y < 0.5);
    assert!(rects[&3].x < 0.5 && rects[&3].y >= 0.5);
    assert!(rects[&2].x >= 0.5 && rects[&2].y < 0.5);
    assert!(rects[&4].x >= 0.5 && rects[&4].y >= 0.5);
}

#[test]
fn neighbor_grid_2x2_directions() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102);
    tree.split(1, Orientation::Vertical, 3, 103);
    tree.split(2, Orientation::Vertical, 4, 104);

    // Layout: 1 (top-left), 2 (top-right), 3 (bottom-left), 4 (bottom-right).
    assert_eq!(tree.neighbor(1, Direction::Right), Some(2));
    assert_eq!(tree.neighbor(1, Direction::Down), Some(3));
    assert_eq!(tree.neighbor(4, Direction::Left), Some(3));
    assert_eq!(tree.neighbor(4, Direction::Up), Some(2));
    assert_eq!(tree.neighbor(1, Direction::Up), None);
    assert_eq!(tree.neighbor(1, Direction::Left), None);
}

#[test]
fn neighbor_column_of_three() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Vertical, 2, 102);
    tree.split(2, Orientation::Vertical, 3, 103);

    assert_eq!(tree.neighbor(1, Direction::Down), Some(2));
    assert_eq!(tree.neighbor(2, Direction::Down), Some(3));
    assert_eq!(tree.neighbor(3, Direction::Up), Some(2));
    assert_eq!(tree.neighbor(3, Direction::Down), None);
}

#[test]
fn find_resizable_ancestor_matches_nearest_matching_orientation() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2, 102); // 1 | 2, horizontal split id 102
    tree.split(2, Orientation::Vertical, 3, 103); // 2 over 3, vertical split id 103

    // Resizing 1 horizontally hits split 102 directly; 1 is the left child.
    let (split_id, is_left, ratio) = tree
        .find_resizable_ancestor(1, Orientation::Horizontal)
        .unwrap();
    assert_eq!(split_id, 102);
    assert!(is_left);
    assert_eq!(ratio, 0.5);

    // Resizing 1 vertically: no vertical ancestor exists at all.
    assert_eq!(tree.find_resizable_ancestor(1, Orientation::Vertical), None);

    // Resizing 3 vertically hits the nearer split 103, not the outer 102.
    let (split_id, is_left, _) = tree
        .find_resizable_ancestor(3, Orientation::Vertical)
        .unwrap();
    assert_eq!(split_id, 103);
    assert!(!is_left);

    // Resizing 3 horizontally falls through to the outer split 102, where 3
    // (via its parent 2) lives on the right.
    let (split_id, is_left, _) = tree
        .find_resizable_ancestor(3, Orientation::Horizontal)
        .unwrap();
    assert_eq!(split_id, 102);
    assert!(!is_left);
}
