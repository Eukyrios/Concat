// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer Abdullah and Concat contributors

//! The workspace's arrangement: a tree of splits and views, walked flat
//! for Slint, which has no recursion.

use crate::ui::{DockDivider, DockSide, PaneKind, SeatBox};

pub const SEAT_MIN_W: f32 = 240.0;
pub const SEAT_MIN_H: f32 = 140.0;
/// The floor that holds where the one above cannot: enough of a seat left to
/// find its corner button with, which is the only way back from a layout that
/// has been squeezed too far.
pub const SEAT_MIN_GRAB: f32 = 64.0;
/// The gutter between the halves of a split, and the margin round the lot.
pub const SEAT_GAP: f32 = 8.0;

pub enum Dock {
    /// One view, filling its box.
    Leaf(PaneKind),
    /// Two nodes and the gutter between them. `columns` means side by side,
    /// which is a vertical gutter dragged left and right; `ratio` is the share
    /// of the usable extent — the box less the gutter — that the first takes.
    Split {
        columns: bool,
        ratio: f32,
        first: Box<Dock>,
        second: Box<Dock>,
    },
}

impl Dock {
    pub fn leaf(kind: PaneKind) -> Box<Dock> {
        Box::new(Dock::Leaf(kind))
    }

    /// The node a path names, following each step into the first branch or the
    /// second. A path that runs past a leaf stops there, which cannot happen
    /// for a path this module produced.
    pub fn at(&self, path: &[bool]) -> &Dock {
        let Some((step, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Dock::Split { first, second, .. } => {
                if *step {
                    second.at(rest)
                } else {
                    first.at(rest)
                }
            }
            leaf => leaf,
        }
    }

    pub fn at_mut(&mut self, path: &[bool]) -> &mut Dock {
        let Some((step, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Dock::Split { first, second, .. } => {
                if *step {
                    second.at_mut(rest)
                } else {
                    first.at_mut(rest)
                }
            }
            leaf => leaf,
        }
    }

    /// The path to the nth leaf, counted in the same walk `lay_out` uses — so
    /// the number a seat carries and the number a drop reports are the same
    /// number. Paths rather than indices for the editing below, because an
    /// index is invalidated by a change elsewhere in the tree and a path is
    /// only invalidated by a change on its own way down.
    pub fn leaf_path(&self, index: usize) -> Option<Vec<bool>> {
        fn walk(node: &Dock, want: usize, seen: &mut usize, path: &mut Vec<bool>) -> bool {
            match node {
                Dock::Leaf(_) => {
                    if *seen == want {
                        return true;
                    }
                    *seen += 1;
                    false
                }
                Dock::Split { first, second, .. } => {
                    path.push(false);
                    if walk(first, want, seen, path) {
                        return true;
                    }
                    path.pop();
                    path.push(true);
                    if walk(second, want, seen, path) {
                        return true;
                    }
                    path.pop();
                    false
                }
            }
        }
        let (mut seen, mut path) = (0, Vec::new());
        walk(self, index, &mut seen, &mut path).then_some(path)
    }

    /// The same for the nth split — parent before children, which is the order
    /// `lay_out` numbers the gutters in.
    pub fn split_path(&self, index: usize) -> Option<Vec<bool>> {
        fn walk(node: &Dock, want: usize, seen: &mut usize, path: &mut Vec<bool>) -> bool {
            let Dock::Split { first, second, .. } = node else {
                return false;
            };
            if *seen == want {
                return true;
            }
            *seen += 1;
            path.push(false);
            if walk(first, want, seen, path) {
                return true;
            }
            path.pop();
            path.push(true);
            if walk(second, want, seen, path) {
                return true;
            }
            path.pop();
            false
        }
        let (mut seen, mut path) = (0, Vec::new());
        walk(self, index, &mut seen, &mut path).then_some(path)
    }

    /// Whether a view of this kind is anywhere in the tree.
    pub fn contains(&self, kind: PaneKind) -> bool {
        match self {
            Dock::Leaf(held) => *held == kind,
            Dock::Split { first, second, .. } => first.contains(kind) || second.contains(kind),
        }
    }

    /// The ratio of the split that parts two views: the deepest one with
    /// both of them under it.
    pub fn ratio_over(&self, a: PaneKind, b: PaneKind) -> Option<f32> {
        let Dock::Split {
            ratio,
            first,
            second,
            ..
        } = self
        else {
            return None;
        };
        if first.contains(a) && first.contains(b) {
            first.ratio_over(a, b)
        } else if second.contains(a) && second.contains(b) {
            second.ratio_over(a, b)
        } else if self.contains(a) && self.contains(b) {
            Some(*ratio)
        } else {
            None
        }
    }

    pub fn kind_at(&self, index: usize) -> Option<PaneKind> {
        match self.at(&self.leaf_path(index)?) {
            Dock::Leaf(kind) => Some(*kind),
            _ => None,
        }
    }

    /// Turn one leaf into a split, with a new view on the named side of it.
    /// Half and half: the seat being split is the only thing that knows how
    /// much room there is, and half of it is the answer that needs no rule.
    pub fn split_leaf(&mut self, path: &[bool], kind: PaneKind, side: DockSide) {
        let node = self.at_mut(path);
        let existing = Box::new(std::mem::replace(node, Dock::Leaf(kind)));
        let (columns, first, second) = match side {
            DockSide::Left => (true, Dock::leaf(kind), existing),
            DockSide::Right => (true, existing, Dock::leaf(kind)),
            DockSide::Top => (false, Dock::leaf(kind), existing),
            _ => (false, existing, Dock::leaf(kind)),
        };
        *node = Dock::Split {
            columns,
            ratio: 0.5,
            first,
            second,
        };
    }

    /// Take a leaf out, and let its sibling take the space — the split it was
    /// half of stops existing. A path with nothing above it is the whole tree,
    /// which cannot be removed: a window with no seats has nowhere to put
    /// anything back.
    pub fn remove_leaf(&mut self, path: &[bool]) {
        let Some((last, above)) = path.split_last() else {
            return;
        };
        let parent = self.at_mut(above);
        let survivor = match parent {
            Dock::Split { first, second, .. } => {
                let keep = if *last { first } else { second };
                std::mem::replace(keep.as_mut(), Dock::Leaf(PaneKind::Media))
            }
            _ => return,
        };
        *parent = survivor;
    }
}

/// The tree walked flat: a box per view, a box per gutter.
#[derive(Default)]
pub struct DockLayout {
    pub seats: Vec<SeatBox>,
    pub dividers: Vec<DockDivider>,
    /// The usable extent of each split — its box less the gutter — in the same
    /// order as `dividers`. A divider drag is a delta in pixels and a ratio is
    /// a fraction, and this is what converts between them.
    pub extents: Vec<f32>,
}

pub fn lay_out(node: &Dock, (x, y, w, h): (f32, f32, f32, f32), out: &mut DockLayout) {
    match node {
        Dock::Leaf(kind) => out.seats.push(SeatBox {
            index: out.seats.len() as i32,
            kind: *kind,
            x,
            y,
            width: w.max(0.0),
            height: h.max(0.0),
        }),
        Dock::Split {
            columns,
            ratio,
            first,
            second,
        } => {
            // The slot is taken before the children are walked, so a gutter's
            // number is its split's position in a parent-first walk — which is
            // what `split_path` counts and what a drag reports back.
            let index = out.dividers.len();
            out.dividers.push(DockDivider::default());
            out.extents.push(0.0);

            let along = if *columns { w } else { h };
            let usable = (along - SEAT_GAP).max(0.0);
            let head = (usable * ratio).clamp(0.0, usable);
            out.extents[index] = usable;
            out.dividers[index] = if *columns {
                DockDivider {
                    index: index as i32,
                    columns: true,
                    x: x + head,
                    y,
                    width: SEAT_GAP,
                    height: h.max(0.0),
                }
            } else {
                DockDivider {
                    index: index as i32,
                    columns: false,
                    x,
                    y: y + head,
                    width: w.max(0.0),
                    height: SEAT_GAP,
                }
            };

            if *columns {
                lay_out(first, (x, y, head, h), out);
                lay_out(second, (x + head + SEAT_GAP, y, usable - head, h), out);
            } else {
                lay_out(first, (x, y, w, head), out);
                lay_out(second, (x, y + head + SEAT_GAP, w, usable - head), out);
            }
        }
    }
}

/// Which panels are open. Each kind is either open or shut, so there is never
/// more than one of any, and each has a place of its own in `build_dock`
/// rather than going wherever there happens to be room.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Panels {
    pub media: bool,
    pub preview: bool,
    pub inspector: bool,
    pub timeline: bool,
    /// The keys. A tab beside the timeline's own while the timeline is
    /// open, and in the timeline's place while it is not.
    pub keyframes: bool,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            media: true,
            preview: true,
            inspector: true,
            timeline: true,
            keyframes: false,
        }
    }
}

impl Panels {
    const NAMES: [&'static str; 5] = ["media", "preview", "inspector", "timeline", "keyframes"];

    /// The panels a saved list names; none saved is the default arrangement,
    /// and a list that names nothing known is too.
    pub fn from_names(names: Option<&[String]>) -> Self {
        let Some(names) = names else {
            return Self::default();
        };
        let has = |name: &str| names.iter().any(|n| n == name);
        let panels = Self {
            media: has("media"),
            preview: has("preview"),
            inspector: has("inspector"),
            timeline: has("timeline"),
            keyframes: has("keyframes"),
        };
        if panels.open() == 0 {
            Self::default()
        } else {
            panels
        }
    }

    pub fn names(&self) -> Vec<String> {
        Self::NAMES
            .iter()
            .zip(self.flags())
            .filter(|(_, on)| *on)
            .map(|(name, _)| (*name).to_owned())
            .collect()
    }

    fn open(&self) -> usize {
        self.flags().into_iter().filter(|on| *on).count()
    }

    /// Whether a kind is showing.
    pub fn shows(&self, kind: PaneKind) -> bool {
        match kind {
            PaneKind::Media => self.media,
            PaneKind::Preview => self.preview,
            PaneKind::Inspector => self.inspector,
            PaneKind::Timeline => self.timeline,
            _ => self.keyframes,
        }
    }

    /// Which panels are up, in `PaneKind` order: library, preview,
    /// inspector, timeline, keyframes.
    pub fn flags(&self) -> [bool; 5] {
        [
            self.media,
            self.preview,
            self.inspector,
            self.timeline,
            self.keyframes,
        ]
    }

    /// Opens a shut panel and shuts an open one. The last one stays: a
    /// window with nothing in it has no button left to bring anything back
    /// with.
    pub fn flip(&mut self, kind: PaneKind) {
        let mut next = *self;
        let slot = match kind {
            PaneKind::Media => &mut next.media,
            PaneKind::Preview => &mut next.preview,
            PaneKind::Inspector => &mut next.inspector,
            PaneKind::Timeline => &mut next.timeline,
            _ => &mut next.keyframes,
        };
        *slot = !*slot;
        if next.open() > 0 {
            *self = next;
        }
    }

    /// Opens a panel if it is shut.
    pub fn show(&mut self, kind: PaneKind) {
        if !self.shows(kind) {
            self.flip(kind);
        }
    }
}

/// How the three splits of `build_dock` are divided: the library against the
/// monitor, those two against the timeline, and all of it against the
/// inspector. Read back off the dock before it is rebuilt, so a panel
/// switched on or off does not undo the gutters the person has dragged.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Ratios {
    pub top: f32,
    pub left: f32,
    pub root: f32,
}

impl Default for Ratios {
    fn default() -> Self {
        Self {
            top: 0.32,
            left: 0.6,
            root: 0.78,
        }
    }
}

impl Ratios {
    /// The ratios the dock has now; a split that is not in it - its panels
    /// are shut - keeps what was held.
    pub fn read(self, dock: &Dock) -> Ratios {
        let upper = if dock.contains(PaneKind::Media) {
            PaneKind::Media
        } else {
            PaneKind::Preview
        };
        let lower = if dock.contains(PaneKind::Timeline) {
            PaneKind::Timeline
        } else {
            PaneKind::Keyframes
        };
        let anchor = if dock.contains(PaneKind::Media) {
            PaneKind::Media
        } else if dock.contains(PaneKind::Preview) {
            PaneKind::Preview
        } else {
            lower
        };
        Ratios {
            top: dock
                .ratio_over(PaneKind::Media, PaneKind::Preview)
                .unwrap_or(self.top),
            left: dock
                .ratio_over(upper, lower)
                .unwrap_or(self.left),
            root: dock
                .ratio_over(anchor, PaneKind::Inspector)
                .unwrap_or(self.root),
        }
    }
}

/// The arrangement for a set of open panels, each in its place: the library
/// and the monitor across the top, the timeline under them, and the
/// inspector a column down the whole right side. Shutting one lets what is
/// left take its room; the proportions are the ones the editor opens with.
pub fn build_dock(panels: &Panels, ratios: &Ratios) -> Dock {
    fn pair(columns: bool, ratio: f32, first: Option<Dock>, second: Option<Dock>) -> Option<Dock> {
        match (first, second) {
            (Some(first), Some(second)) => Some(Dock::Split {
                columns,
                ratio,
                first: Box::new(first),
                second: Box::new(second),
            }),
            (one, other) => one.or(other),
        }
    }
    let leaf = |on: bool, kind| on.then_some(Dock::Leaf(kind));
    let top = pair(
        true,
        ratios.top,
        leaf(panels.media, PaneKind::Media),
        leaf(panels.preview, PaneKind::Preview),
    );
    // The keys are a tab of the timeline when it is open and take its place
    // when it is not, so shutting the timeline does not shut them.
    let lower = if panels.timeline {
        leaf(true, PaneKind::Timeline)
    } else {
        leaf(panels.keyframes, PaneKind::Keyframes)
    };
    let left = pair(false, ratios.left, top, lower);
    pair(
        true,
        ratios.root,
        left,
        leaf(panels.inspector, PaneKind::Inspector),
    )
    .unwrap_or(Dock::Leaf(PaneKind::Preview))
}

/// The dock a narrow window shows: the monitor over the timeline, and
/// nothing beside either. The library and the inspector are a switch away
/// on each seat - see `PaneSwitcher` - which on a phone is how they are
/// reached in every editor: one at a time, in the space the picture had.
pub fn compact_dock() -> Dock {
    Dock::Split {
        columns: false,
        ratio: 0.5,
        first: Dock::leaf(PaneKind::Preview),
        second: Dock::leaf(PaneKind::Timeline),
    }
}

/// The top edge of a row, measured down the stack from the first lane.
pub fn row_top(heights: &[f32], row: i32) -> f32 {
    heights.iter().take(row.max(0) as usize).sum()
}

/// The row a point down the stack falls in, clamped to the stack.
pub fn row_at(heights: &[f32], y: f32) -> i32 {
    let mut top = 0.0;
    for (row, height) in heights.iter().enumerate() {
        top += height;
        if y < top {
            return row as i32;
        }
    }
    (heights.len() as i32 - 1).max(0)
}

/// The row whose top edge is nearest a point down the stack.
///
/// What a *move* wants, and not the same question as `row_at`: dragging a clip
/// halfway into the lane below should put it there, which is the rounding the
/// old `delta / lane-height` did for free back when every lane was one height.
pub fn nearest_row(heights: &[f32], y: f32) -> i32 {
    let (mut top, mut best, mut best_distance) = (0.0_f32, 0_i32, f32::MAX);
    for (row, height) in heights.iter().enumerate() {
        let distance = (top - y).abs();
        if distance < best_distance {
            best_distance = distance;
            best = row as i32;
        }
        top += height;
    }
    best
}
