//! Incremental lane assignment for the commit graph.
//!
//! Commits arrive in topological order (children before parents). Each
//! column ("lane") waits for one commit id: the next parent on that line of
//! history. Lanes never move sideways, so a row can be drawn on its own and
//! the layout can continue page after page without recomputing earlier rows.
//!
//! A row is drawn in two halves around the commit's node:
//! - `incoming`: from the top edge at `column` into the node (children above);
//! - `outgoing`: from the node to the bottom edge at `column` (parents below);
//! - `passing`: straight through the whole row (lines waiting for a commit
//!   further down).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Lane {
    pub column: u16,
    /// Colour index; the UI maps it onto its palette (modulo its size).
    pub color: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GraphRow {
    /// Column of this commit's node.
    pub column: u16,
    pub color: u32,
    pub incoming: Vec<Lane>,
    pub outgoing: Vec<Lane>,
    pub passing: Vec<Lane>,
}

#[derive(Debug, Clone)]
struct Slot {
    waiting_for: String,
    color: u32,
}

#[derive(Debug, Clone, Default)]
pub struct GraphLayout {
    lanes: Vec<Option<Slot>>,
    next_color: u32,
}

impl GraphLayout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of columns currently in use (including gaps).
    pub fn width(&self) -> usize {
        self.lanes.len()
    }

    /// Places the next commit and returns its row.
    pub fn push(&mut self, oid: &str, parents: &[String]) -> GraphRow {
        let matching: Vec<usize> = self
            .lanes
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.as_ref().is_some_and(|s| s.waiting_for == oid))
            .map(|(i, _)| i)
            .collect();

        // The node takes the leftmost lane that was waiting for it, or opens
        // a new line (a branch tip) in the first free column.
        let (column, color) = match matching.first() {
            Some(&first) => (first, self.color_at(first)),
            None => (self.free_column(), self.new_color()),
        };
        let incoming = matching
            .iter()
            .map(|&i| lane(i, self.color_at(i)))
            .collect();
        for &i in &matching {
            self.lanes[i] = None;
        }
        let passing = self.occupied();

        let mut outgoing = Vec::with_capacity(parents.len());
        for (index, parent) in parents.iter().enumerate() {
            // The first parent always continues this commit's own line and
            // colour, even if another line also leads to it: the lines then
            // converge at the parent, which sits in the leftmost of them.
            // That keeps long-lived lines (e.g. main) in place.
            let (target, line_color) = if index == 0 {
                (column, color)
            } else if let Some(existing) = self.position_of(parent) {
                // A merge parent another line already leads to: join it.
                outgoing.push(lane(existing, self.color_at(existing)));
                continue;
            } else {
                (self.free_column(), self.new_color())
            };
            self.occupy(
                target,
                Slot {
                    waiting_for: parent.clone(),
                    color: line_color,
                },
            );
            outgoing.push(lane(target, line_color));
        }

        while self.lanes.last().is_some_and(Option::is_none) {
            self.lanes.pop();
        }

        GraphRow {
            column: to_u16(column),
            color,
            incoming,
            outgoing,
            passing,
        }
    }

    fn color_at(&self, column: usize) -> u32 {
        self.lanes[column].as_ref().map_or(0, |s| s.color)
    }

    fn position_of(&self, oid: &str) -> Option<usize> {
        self.lanes
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|s| s.waiting_for == oid))
    }

    fn free_column(&self) -> usize {
        self.lanes
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.lanes.len())
    }

    fn occupy(&mut self, column: usize, slot: Slot) {
        if column >= self.lanes.len() {
            self.lanes.resize(column + 1, None);
        }
        self.lanes[column] = Some(slot);
    }

    fn occupied(&self) -> Vec<Lane> {
        self.lanes
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|s| lane(i, s.color)))
            .collect()
    }

    fn new_color(&mut self) -> u32 {
        let color = self.next_color;
        self.next_color = self.next_color.wrapping_add(1);
        color
    }
}

fn lane(column: usize, color: u32) -> Lane {
    Lane {
        column: to_u16(column),
        color,
    }
}

/// Real histories stay far below 65 535 simultaneous lines; saturate rather
/// than wrap if one ever does not.
fn to_u16(column: usize) -> u16 {
    u16::try_from(column).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Lays out `(oid, parents)` pairs given in topological order.
    fn layout(commits: &[(&str, &[&str])]) -> Vec<GraphRow> {
        let mut graph = GraphLayout::new();
        commits
            .iter()
            .map(|(oid, parents)| {
                let parents: Vec<String> = parents.iter().map(|p| p.to_string()).collect();
                graph.push(oid, &parents)
            })
            .collect()
    }

    fn cols(lanes: &[Lane]) -> Vec<u16> {
        lanes.iter().map(|l| l.column).collect()
    }

    /// Every line leaving the bottom of a row enters the top of the next one,
    /// at the same column and with the same colour.
    pub(crate) fn assert_continuous(rows: &[GraphRow]) {
        for (i, pair) in rows.windows(2).enumerate() {
            let bottom: BTreeMap<u16, u32> = pair[0]
                .outgoing
                .iter()
                .chain(&pair[0].passing)
                .map(|l| (l.column, l.color))
                .collect();
            let top: BTreeMap<u16, u32> = pair[1]
                .incoming
                .iter()
                .chain(&pair[1].passing)
                .map(|l| (l.column, l.color))
                .collect();
            assert_eq!(bottom, top, "discontinuity between rows {i} and {}", i + 1);
        }
    }

    #[test]
    fn linear_history_stays_in_one_column() {
        let rows = layout(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
        assert!(rows.iter().all(|r| r.column == 0 && r.color == 0));
        assert!(rows[0].incoming.is_empty(), "branch tip");
        assert_eq!(cols(&rows[1].incoming), [0]);
        assert!(rows[2].outgoing.is_empty(), "root");
        assert!(rows.iter().all(|r| r.passing.is_empty()));
        assert_continuous(&rows);
    }

    #[test]
    fn branch_and_merge() {
        // m merges b (first parent) and d; d-c branched off a.
        let rows = layout(&[
            ("m", &["b", "d"]),
            ("d", &["c"]),
            ("c", &["a"]),
            ("b", &["a"]),
            ("a", &[]),
        ]);
        assert_eq!(cols(&rows[0].outgoing), [0, 1]);
        assert_eq!((rows[1].column, rows[1].color), (1, 1));
        assert_eq!(cols(&rows[1].passing), [0], "main line passes d");
        assert_eq!(rows[3].column, 0);
        assert_eq!(cols(&rows[3].outgoing), [0], "main keeps its own line");
        assert_eq!(rows[4].column, 0, "the root stays on the main line");
        assert_eq!(cols(&rows[4].incoming), [0, 1], "both lines converge at a");
        assert_continuous(&rows);
    }

    #[test]
    fn merge_parent_joins_an_existing_line() {
        // m merges x, which the line below b is already waiting for.
        let rows = layout(&[("b", &["x"]), ("m", &["a", "x"]), ("x", &["a"]), ("a", &[])]);
        assert_eq!(rows[1].column, 1);
        assert_eq!(
            cols(&rows[1].outgoing),
            [1, 0],
            "second parent joins column 0"
        );
        assert_continuous(&rows);
    }

    #[test]
    fn branch_point_collects_every_child() {
        // Two tips whose lines both end at a.
        let rows = layout(&[("x", &["a"]), ("y", &["a"]), ("a", &[])]);
        assert_eq!((rows[0].column, rows[1].column), (0, 1));
        assert_ne!(rows[0].color, rows[1].color);
        assert_eq!(rows[2].column, 0);
        assert_eq!(cols(&rows[2].incoming), [0, 1]);
        assert_continuous(&rows);
    }

    #[test]
    fn octopus_merge_opens_a_line_per_parent() {
        let rows = layout(&[
            ("o", &["a", "b", "c"]),
            ("c", &["r"]),
            ("b", &["r"]),
            ("a", &["r"]),
            ("r", &[]),
        ]);
        assert_eq!(cols(&rows[0].outgoing), [0, 1, 2]);
        let colors: Vec<u32> = rows[0].outgoing.iter().map(|l| l.color).collect();
        assert_eq!(colors, [0, 1, 2]);
        assert_eq!(cols(&rows[4].incoming), [0, 1, 2]);
        assert_continuous(&rows);
    }

    #[test]
    fn freed_columns_are_reused() {
        let rows = layout(&[
            ("m", &["a", "x"]),
            ("x", &["a"]),
            ("a", &["z"]),
            ("t", &["z"]), // new tip after column 1 was freed
            ("z", &[]),
        ]);
        assert_eq!(rows[1].column, 1);
        assert_eq!(rows[3].column, 1, "reuses the freed column");
        assert_continuous(&rows);
    }

    #[test]
    fn unrelated_histories_get_separate_lines() {
        let rows = layout(&[("b", &["a"]), ("y", &["x"]), ("a", &[]), ("x", &[])]);
        assert_eq!(rows[1].column, 1);
        assert_eq!(rows[3].column, 1);
        assert_ne!(rows[0].color, rows[1].color);
        assert_continuous(&rows);
    }

    #[test]
    fn missing_parents_keep_their_line_open() {
        // Shallow clone or a page boundary: "gone" never arrives.
        let mut graph = GraphLayout::new();
        graph.push("b", &["gone".to_string()]);
        let row = graph.push("t", &[]);
        assert_eq!(row.column, 1);
        assert_eq!(cols(&row.passing), [0]);
        assert_eq!(graph.width(), 1);
    }

    #[test]
    fn pages_continue_exactly_like_one_pass() {
        let commits: [(&str, &[&str]); 5] = [
            ("m", &["b", "d"]),
            ("d", &["c"]),
            ("c", &["a"]),
            ("b", &["a"]),
            ("a", &[]),
        ];
        let all = layout(&commits);
        let mut graph = GraphLayout::new();
        let mut paged = Vec::new();
        for page in commits.chunks(2) {
            for (oid, parents) in page {
                let parents: Vec<String> = parents.iter().map(|p| p.to_string()).collect();
                paged.push(graph.push(oid, &parents));
            }
        }
        assert_eq!(paged, all);
    }

    /// Tiny deterministic PRNG (xorshift) so the property test needs no crate.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % bound as u64) as usize
        }
    }

    #[test]
    fn random_histories_are_continuous_and_close_every_line() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..300 {
            let n = 2 + rng.next(60);
            // Index order is topological: parents always have higher indices.
            let commits: Vec<(String, Vec<String>)> = (0..n)
                .map(|i| {
                    let later = n - i - 1;
                    let parent_count = if later == 0 {
                        0
                    } else {
                        rng.next(4).min(later)
                    };
                    let mut parents: Vec<String> = Vec::new();
                    while parents.len() < parent_count {
                        let p = format!("c{}", i + 1 + rng.next(later));
                        if !parents.contains(&p) {
                            parents.push(p);
                        }
                    }
                    (format!("c{i}"), parents)
                })
                .collect();

            let mut graph = GraphLayout::new();
            let rows: Vec<GraphRow> = commits
                .iter()
                .map(|(oid, parents)| graph.push(oid, parents))
                .collect();
            assert_continuous(&rows);
            for (row, (_, parents)) in rows.iter().zip(&commits) {
                assert_eq!(row.outgoing.len(), parents.len());
                if let Some(min) = row.incoming.iter().map(|l| l.column).min() {
                    assert_eq!(row.column, min, "node sits on its leftmost incoming line");
                }
                assert!(
                    row.passing.iter().all(|l| l.column != row.column),
                    "no line passes through a node"
                );
            }
            assert_eq!(graph.width(), 0, "every line reaches its commit");
        }
    }
}
