pub type PaneId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

#[derive(Debug, Clone)]
pub enum SplitTree {
    Leaf(PaneId),
    Split {
        orientation: Orientation,
        ratio: f32,
        left: Box<SplitTree>,
        right: Box<SplitTree>,
    },
}

impl SplitTree {
    pub fn new_leaf(id: PaneId) -> Self {
        SplitTree::Leaf(id)
    }

    /// Splits the leaf identified by `target`, inserting `new_id` as the
    /// second child in the given orientation. Returns true if `target` was
    /// found and split.
    pub fn split(&mut self, target: PaneId, orientation: Orientation, new_id: PaneId) -> bool {
        match self {
            SplitTree::Leaf(id) if *id == target => {
                let old = SplitTree::Leaf(*id);
                *self = SplitTree::Split {
                    orientation,
                    ratio: 0.5,
                    left: Box::new(old),
                    right: Box::new(SplitTree::Leaf(new_id)),
                };
                true
            }
            SplitTree::Leaf(_) => false,
            SplitTree::Split { left, right, .. } => {
                left.split(target, orientation, new_id) || right.split(target, orientation, new_id)
            }
        }
    }

    /// Removes the leaf identified by `target`. The parent `Split` node is
    /// replaced by whichever child remains, which is exactly the
    /// "rebalance" the guideline describes. Returns false (no-op) if
    /// `target` is the only leaf in the tree, or if it wasn't found.
    pub fn close(&mut self, target: PaneId) -> bool {
        if self.is_leaf_only() {
            return false;
        }
        Self::close_inner(self, target)
    }

    fn close_inner(node: &mut SplitTree, target: PaneId) -> bool {
        match node {
            SplitTree::Leaf(_) => false,
            SplitTree::Split { left, right, .. } => {
                let left_is_target = matches!(left.as_ref(), SplitTree::Leaf(id) if *id == target);
                let right_is_target =
                    matches!(right.as_ref(), SplitTree::Leaf(id) if *id == target);

                if left_is_target {
                    *node = (**right).clone();
                    true
                } else if right_is_target {
                    *node = (**left).clone();
                    true
                } else {
                    Self::close_inner(left, target) || Self::close_inner(right, target)
                }
            }
        }
    }

    pub fn find(&self, id: PaneId) -> Option<&SplitTree> {
        match self {
            SplitTree::Leaf(leaf_id) if *leaf_id == id => Some(self),
            SplitTree::Leaf(_) => None,
            SplitTree::Split { left, right, .. } => left.find(id).or_else(|| right.find(id)),
        }
    }

    /// In-order traversal of leaves, left-to-right / top-to-bottom.
    pub fn leaves(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<PaneId>) {
        match self {
            SplitTree::Leaf(id) => out.push(*id),
            SplitTree::Split { left, right, .. } => {
                left.collect_leaves(out);
                right.collect_leaves(out);
            }
        }
    }

    pub fn is_leaf_only(&self) -> bool {
        matches!(self, SplitTree::Leaf(_))
    }

    /// Computes a normalized [0,1] rectangle for every leaf, purely from the
    /// tree shape (no GTK involved) — used to drive directional navigation.
    pub fn leaf_rects(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        self.collect_rects(
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            &mut out,
        );
        out
    }

    fn collect_rects(&self, rect: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match self {
            SplitTree::Leaf(id) => out.push((*id, rect)),
            SplitTree::Split {
                orientation,
                ratio,
                left,
                right,
            } => match orientation {
                Orientation::Horizontal => {
                    let left_w = rect.w * ratio;
                    left.collect_rects(
                        Rect {
                            x: rect.x,
                            y: rect.y,
                            w: left_w,
                            h: rect.h,
                        },
                        out,
                    );
                    right.collect_rects(
                        Rect {
                            x: rect.x + left_w,
                            y: rect.y,
                            w: rect.w - left_w,
                            h: rect.h,
                        },
                        out,
                    );
                }
                Orientation::Vertical => {
                    let top_h = rect.h * ratio;
                    left.collect_rects(
                        Rect {
                            x: rect.x,
                            y: rect.y,
                            w: rect.w,
                            h: top_h,
                        },
                        out,
                    );
                    right.collect_rects(
                        Rect {
                            x: rect.x,
                            y: rect.y + top_h,
                            w: rect.w,
                            h: rect.h - top_h,
                        },
                        out,
                    );
                }
            },
        }
    }

    /// Finds the closest leaf to `from` in the given screen direction, based
    /// on the geometry computed by `leaf_rects`. Returns `None` if there is
    /// no leaf in that direction (e.g. already at the edge).
    pub fn neighbor(&self, from: PaneId, direction: Direction) -> Option<PaneId> {
        let rects = self.leaf_rects();
        let origin = rects.iter().find(|(id, _)| *id == from)?.1;
        let (ox, oy) = origin.center();

        rects
            .iter()
            .filter(|(id, _)| *id != from)
            .filter_map(|(id, rect)| {
                let (cx, cy) = rect.center();
                let in_direction = match direction {
                    Direction::Up => cy < oy,
                    Direction::Down => cy > oy,
                    Direction::Left => cx < ox,
                    Direction::Right => cx > ox,
                };
                if !in_direction {
                    return None;
                }
                let dist = match direction {
                    Direction::Up | Direction::Down => (cy - oy).abs() + (cx - ox).abs() * 0.1,
                    Direction::Left | Direction::Right => (cx - ox).abs() + (cy - oy).abs() * 0.1,
                };
                Some((*id, dist))
            })
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(id, _)| id)
    }
}
