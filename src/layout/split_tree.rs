use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub type PaneId = u64;
pub type SplitId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SplitTree {
    Leaf(PaneId),
    Split {
        id: SplitId,
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
    /// second child in the given orientation. `split_id` identifies the new
    /// `Split` node itself (distinct namespace from `PaneId`, allocated by
    /// the caller) so that `set_ratio` can later target it by id instead of
    /// by tree position, which would shift on every unrelated mutation.
    /// Returns true if `target` was found and split.
    pub fn split(
        &mut self,
        target: PaneId,
        orientation: Orientation,
        new_id: PaneId,
        split_id: SplitId,
    ) -> bool {
        match self {
            SplitTree::Leaf(id) if *id == target => {
                let old = SplitTree::Leaf(*id);
                *self = SplitTree::Split {
                    id: split_id,
                    orientation,
                    ratio: 0.5,
                    left: Box::new(old),
                    right: Box::new(SplitTree::Leaf(new_id)),
                };
                true
            }
            SplitTree::Leaf(_) => false,
            SplitTree::Split { left, right, .. } => {
                left.split(target, orientation, new_id, split_id)
                    || right.split(target, orientation, new_id, split_id)
            }
        }
    }

    /// Finds the `Split` node with the given id and overwrites its ratio,
    /// clamped to a sane range so a drag to the very edge can't collapse a
    /// pane to zero size. Returns false if no such split exists.
    pub fn set_ratio(&mut self, target: SplitId, ratio: f32) -> bool {
        match self {
            SplitTree::Leaf(_) => false,
            SplitTree::Split {
                id,
                ratio: r,
                left,
                right,
                ..
            } => {
                if *id == target {
                    *r = ratio.clamp(0.05, 0.95);
                    true
                } else {
                    left.set_ratio(target, ratio) || right.set_ratio(target, ratio)
                }
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
                ..
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

    /// Reassigns every `PaneId`/`SplitId` in this tree using `next_id`
    /// (e.g. a process-local counter). A tree deserialized from a saved
    /// session was built by a *previous* process, so its ids can collide
    /// with ones already in use by this one — this produces an equivalent
    /// tree with fresh ids, plus the old-id -> new-id map for `PaneId`s
    /// specifically, since callers (`session::persist`) need it to look up
    /// per-pane metadata that was saved keyed by the old id.
    pub fn remap_ids(
        &self,
        next_id: &mut impl FnMut() -> u64,
    ) -> (SplitTree, HashMap<PaneId, PaneId>) {
        let mut pane_map = HashMap::new();
        let remapped = self.remap_ids_inner(next_id, &mut pane_map);
        (remapped, pane_map)
    }

    fn remap_ids_inner(
        &self,
        next_id: &mut impl FnMut() -> u64,
        pane_map: &mut HashMap<PaneId, PaneId>,
    ) -> SplitTree {
        match self {
            SplitTree::Leaf(old_id) => {
                let new_id = next_id();
                pane_map.insert(*old_id, new_id);
                SplitTree::Leaf(new_id)
            }
            SplitTree::Split {
                orientation,
                ratio,
                left,
                right,
                ..
            } => SplitTree::Split {
                id: next_id(),
                orientation: *orientation,
                ratio: *ratio,
                left: Box::new(left.remap_ids_inner(next_id, pane_map)),
                right: Box::new(right.remap_ids_inner(next_id, pane_map)),
            },
        }
    }
}
