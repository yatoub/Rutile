use rutile::layout::{Direction, Orientation, SplitTree};

#[test]
fn split_creates_two_leaves_with_correct_orientation() {
    let mut tree = SplitTree::new_leaf(1);
    assert!(tree.split(1, Orientation::Horizontal, 2));

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
fn split_on_unknown_target_is_noop() {
    let mut tree = SplitTree::new_leaf(1);
    assert!(!tree.split(99, Orientation::Horizontal, 2));
    assert!(matches!(tree, SplitTree::Leaf(1)));
}

#[test]
fn close_collapses_to_sibling() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2);
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
    tree.split(1, Orientation::Horizontal, 2);
    tree.split(2, Orientation::Vertical, 3);

    assert_eq!(tree.leaves(), vec![1, 2, 3]);
}

#[test]
fn nested_splits_produce_correct_shape() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2);
    tree.split(1, Orientation::Vertical, 3);
    tree.split(2, Orientation::Vertical, 4);

    // Leaves should be: (1 top, 3 bottom) on the left, (2 top, 4 bottom) on the right.
    assert_eq!(tree.leaves(), vec![1, 3, 2, 4]);
}

#[test]
fn leaf_rects_grid_2x2() {
    let mut tree = SplitTree::new_leaf(1);
    tree.split(1, Orientation::Horizontal, 2);
    tree.split(1, Orientation::Vertical, 3);
    tree.split(2, Orientation::Vertical, 4);

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
    tree.split(1, Orientation::Horizontal, 2);
    tree.split(1, Orientation::Vertical, 3);
    tree.split(2, Orientation::Vertical, 4);

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
    tree.split(1, Orientation::Vertical, 2);
    tree.split(2, Orientation::Vertical, 3);

    assert_eq!(tree.neighbor(1, Direction::Down), Some(2));
    assert_eq!(tree.neighbor(2, Direction::Down), Some(3));
    assert_eq!(tree.neighbor(3, Direction::Up), Some(2));
    assert_eq!(tree.neighbor(3, Direction::Down), None);
}
