//! Where the keyboard highlight is, and where a key moves it.
//!
//! Three zones hold one highlight between them: the app list, the pinned
//! tiles and the side rail. Pure arithmetic over indices and grid cells —
//! no widgets, no messages — so every rule here is a unit test rather than
//! something only a human pressing keys can check.

use crate::config::TileSize;
use crate::tile_layout::{self, Placement};

/// The three places a highlight can live, in Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// The app list in the middle column, one row at a time.
    List,
    /// The pinned tiles: a variable-width grid, so all four arrows work.
    Tiles,
    /// The thin left strip of account, default-app, Settings and Power
    /// buttons.
    Rail,
}

/// Tab walks the zones in this order and wraps; Shift+Tab walks it backwards.
const ORDER: [Zone; 3] = [Zone::List, Zone::Tiles, Zone::Rail];

/// The one highlight: which zone, and how far into that zone's own order.
/// For the list and the rail that is the n'th thing the keyboard can land
/// on; for the tiles it is the n'th tile as the column draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    pub zone: Zone,
    pub index: usize,
}

impl Spot {
    fn at(zone: Zone) -> Self {
        Self { zone, index: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// How many things each zone has to land on right now. A zone with none is
/// not drawn (a search is on, the letter grid has replaced the list, the
/// right column is showing something other than tiles) and Tab skips it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub list: usize,
    pub tiles: usize,
    pub rail: usize,
}

impl Counts {
    pub fn of(self, zone: Zone) -> usize {
        match zone {
            Zone::List => self.list,
            Zone::Tiles => self.tiles,
            Zone::Rail => self.rail,
        }
    }

    fn all_empty(self) -> bool {
        ORDER.iter().all(|&z| self.of(z) == 0)
    }
}

/// Tab (`back` false) or Shift+Tab: the next zone that has anything in it.
///
/// From no highlight at all — the search box has the keyboard — Tab lands on
/// the first filled zone, which is what puts the highlight on screen in the
/// first place. Landing on a zone starts at its first item, except when the
/// ring comes back round to the zone it started in: there the user's place
/// is kept rather than thrown away.
pub fn tab(from: Option<Spot>, back: bool, counts: Counts) -> Option<Spot> {
    if counts.all_empty() {
        return None;
    }
    let Some(spot) = from else {
        // Forward starts looking at the head of the ring, backward at its
        // tail, so a first Shift+Tab reaches the rail rather than the list.
        let order: Vec<Zone> = if back {
            ORDER.iter().rev().copied().collect()
        } else {
            ORDER.to_vec()
        };
        return order.into_iter().find(|&z| counts.of(z) > 0).map(Spot::at);
    };
    let here = ORDER.iter().position(|&z| z == spot.zone).unwrap_or(0);
    for step in 1..=ORDER.len() {
        let next = if back {
            (here + ORDER.len() - step % ORDER.len()) % ORDER.len()
        } else {
            (here + step) % ORDER.len()
        };
        let zone = ORDER[next];
        if counts.of(zone) == 0 {
            continue;
        }
        if zone == spot.zone {
            return Some(spot);
        }
        return Some(Spot::at(zone));
    }
    None
}

/// Bring a highlight back into range after the zone under it has changed
/// size: the mouse closed a folder, unpinned a tile, or the background
/// reload replaced the lists. The last item when the zone has shrunk past
/// the index, and no highlight at all when the zone is empty — never a
/// silent reset to the first item, which cost several dead keypresses
/// before one press moved one visible step again.
pub fn clamped(spot: Spot, counts: Counts) -> Option<Spot> {
    let len = counts.of(spot.zone);
    let last = len.checked_sub(1)?;
    Some(Spot {
        zone: spot.zone,
        index: spot.index.min(last),
    })
}

/// A step through a single column — the list or the rail. Clamped at both
/// ends, like the arrow keys through search results: holding Down parks on
/// the last row instead of wrapping round to the first.
pub fn step(index: usize, len: usize, dir: Dir) -> usize {
    if len == 0 {
        return 0;
    }
    match dir {
        Dir::Up => index.saturating_sub(1),
        Dir::Down => (index + 1).min(len - 1),
        // A column has no sideways.
        Dir::Left | Dir::Right => index.min(len - 1),
    }
}

/// Every pinned tile as one grid, groups stacked in the order they are
/// drawn. Each group is packed by [`tile_layout::pack`] — the same packer
/// the tiles are laid out with, so the cell the keyboard moves through is
/// the cell on screen — and then pushed down by the rows above it, which is
/// what lets Down leave a group for the one below.
pub fn tile_grid(groups: &[Vec<TileSize>], columns: u16) -> Vec<Placement> {
    let mut out = Vec::new();
    let mut top = 0u16;
    for sizes in groups {
        let packed = tile_layout::pack(sizes, columns);
        out.extend(packed.tiles.iter().map(|p| Placement {
            row: p.row + top,
            ..*p
        }));
        top += packed.rows;
    }
    out
}

/// How many columns two cells share. Nothing in a variable-width grid lines
/// up by index, so a vertical move picks the neighbour it overlaps most —
/// Down from a Wide tile lands under the part of it the user is looking at,
/// not back at column 0.
fn overlap(a: &Placement, b: &Placement) -> u16 {
    let left = a.col.max(b.col);
    let right = (a.col + a.cols).min(b.col + b.cols);
    right.saturating_sub(left)
}

fn shares_a_band(a: &Placement, b: &Placement) -> bool {
    a.row < b.row + b.rows && b.row < a.row + a.rows
}

/// Move the highlight through the tile grid. A move with nothing to land on
/// — off the edge of the grid, or past the last group — keeps the tile it
/// is on, so an arrow key never silently loses the highlight.
pub fn tile_step(cells: &[Placement], index: usize, dir: Dir) -> usize {
    let Some(cur) = cells.get(index) else {
        return 0;
    };
    let pick = |iter: &mut dyn Iterator<Item = (usize, &Placement)>, dir: Dir| -> Option<usize> {
        match dir {
            // Nearest cell to the side, on a row band this one touches.
            Dir::Right => iter.min_by_key(|(_, c)| (c.col, c.row)).map(|(i, _)| i),
            Dir::Left => iter
                .max_by_key(|(_, c)| (c.col + c.cols, std::cmp::Reverse(c.row)))
                .map(|(i, _)| i),
            // Nearest row band, then the best column overlap in it.
            Dir::Down => iter
                .min_by_key(|(_, c)| (c.row, std::cmp::Reverse(overlap(cur, c)), c.col))
                .map(|(i, _)| i),
            Dir::Up => iter
                .max_by_key(|(_, c)| (c.row + c.rows, overlap(cur, c), std::cmp::Reverse(c.col)))
                .map(|(i, _)| i),
        }
    };
    let mut candidates = cells
        .iter()
        .enumerate()
        .filter(|(i, c)| *i != index && reachable(cur, c, dir));
    pick(&mut candidates, dir).unwrap_or(index)
}

fn reachable(cur: &Placement, c: &Placement, dir: Dir) -> bool {
    match dir {
        Dir::Right => c.col >= cur.col + cur.cols && shares_a_band(cur, c),
        Dir::Left => c.col + c.cols <= cur.col && shares_a_band(cur, c),
        Dir::Down => c.row >= cur.row + cur.rows,
        Dir::Up => c.row + c.rows <= cur.row,
    }
}

/// Where the highlight goes when an arrow is pressed. One call for all three
/// zones, so `app.rs` never has to know which of them is a grid.
pub fn moved(spot: Spot, dir: Dir, counts: Counts, tiles: &[Placement]) -> Spot {
    let index = match spot.zone {
        Zone::Tiles => tile_step(tiles, spot.index, dir),
        zone => step(spot.index, counts.of(zone), dir),
    };
    Spot {
        zone: spot.zone,
        index,
    }
}

/// The scroll offset that brings a row at `y`, `h` tall, into a viewport
/// `view` tall currently scrolled to `offset`; `None` when it is already
/// showing. Only ever the smallest move that works — a row one step below
/// the fold comes up by one row, so arrowing down walks the list instead of
/// re-centring it under the cursor.
pub fn reveal(y: f32, h: f32, offset: f32, view: f32) -> Option<f32> {
    if y < offset {
        return Some(y.max(0.0));
    }
    if y + h > offset + view {
        return Some((y + h - view).max(0.0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use TileSize::*;

    fn counts(list: usize, tiles: usize, rail: usize) -> Counts {
        Counts { list, tiles, rail }
    }

    fn spot(zone: Zone, index: usize) -> Option<Spot> {
        Some(Spot { zone, index })
    }

    #[test]
    fn tab_cycles_list_then_tiles_then_rail_and_wraps() {
        let c = counts(10, 4, 7);
        assert_eq!(tab(None, false, c), spot(Zone::List, 0));
        assert_eq!(tab(spot(Zone::List, 3), false, c), spot(Zone::Tiles, 0));
        assert_eq!(tab(spot(Zone::Tiles, 1), false, c), spot(Zone::Rail, 0));
        assert_eq!(tab(spot(Zone::Rail, 2), false, c), spot(Zone::List, 0));
    }

    #[test]
    fn shift_tab_walks_the_same_ring_backwards() {
        let c = counts(10, 4, 7);
        assert_eq!(tab(None, true, c), spot(Zone::Rail, 0));
        assert_eq!(tab(spot(Zone::List, 3), true, c), spot(Zone::Rail, 0));
        assert_eq!(tab(spot(Zone::Rail, 2), true, c), spot(Zone::Tiles, 0));
        assert_eq!(tab(spot(Zone::Tiles, 1), true, c), spot(Zone::List, 0));
    }

    #[test]
    fn tab_skips_a_zone_with_nothing_in_it() {
        // No tiles: the right column is showing Favourites.
        let c = counts(10, 0, 7);
        assert_eq!(tab(spot(Zone::List, 0), false, c), spot(Zone::Rail, 0));
        assert_eq!(tab(spot(Zone::Rail, 0), true, c), spot(Zone::List, 0));
        // Nothing but the rail: Tab from nothing still finds it.
        assert_eq!(tab(None, false, counts(0, 0, 7)), spot(Zone::Rail, 0));
    }

    #[test]
    fn tab_keeps_the_place_when_it_comes_back_to_the_same_zone() {
        // The list is the only filled zone, so the ring returns to it; the
        // row the user had reached is not thrown away.
        let c = counts(10, 0, 0);
        assert_eq!(tab(spot(Zone::List, 6), false, c), spot(Zone::List, 6));
        assert_eq!(tab(spot(Zone::List, 6), true, c), spot(Zone::List, 6));
    }

    #[test]
    fn tab_does_nothing_when_no_zone_has_anything() {
        assert_eq!(tab(None, false, Counts::default()), None);
        assert_eq!(tab(spot(Zone::List, 2), false, Counts::default()), None);
    }

    #[test]
    fn a_highlight_past_the_end_falls_back_to_the_last_item() {
        let c = counts(4, 2, 6);
        // Still in range: left exactly where it was.
        assert_eq!(
            clamped(
                Spot {
                    zone: Zone::List,
                    index: 3
                },
                c
            ),
            spot(Zone::List, 3)
        );
        // The list shrank under it: the last row, not the first, so the
        // next Up key moves one visible step.
        assert_eq!(
            clamped(
                Spot {
                    zone: Zone::List,
                    index: 9
                },
                c
            ),
            spot(Zone::List, 3)
        );
        assert_eq!(
            clamped(
                Spot {
                    zone: Zone::Tiles,
                    index: 7
                },
                c
            ),
            spot(Zone::Tiles, 1)
        );
    }

    #[test]
    fn a_zone_that_has_emptied_loses_the_highlight() {
        let c = counts(0, 0, 6);
        assert_eq!(
            clamped(
                Spot {
                    zone: Zone::List,
                    index: 2
                },
                c
            ),
            None
        );
        assert_eq!(
            clamped(
                Spot {
                    zone: Zone::Rail,
                    index: 2
                },
                c
            ),
            spot(Zone::Rail, 2)
        );
    }

    #[test]
    fn a_column_clamps_at_both_ends_and_ignores_sideways() {
        assert_eq!(step(0, 5, Dir::Up), 0);
        assert_eq!(step(0, 5, Dir::Down), 1);
        assert_eq!(step(4, 5, Dir::Down), 4);
        assert_eq!(step(3, 5, Dir::Left), 3);
        assert_eq!(step(3, 5, Dir::Right), 3);
        // An empty column, or a stale index from a longer one.
        assert_eq!(step(3, 0, Dir::Down), 0);
        assert_eq!(step(9, 5, Dir::Down), 4);
    }

    #[test]
    fn tile_groups_stack_into_one_grid() {
        let grid = tile_grid(&[vec![Medium, Medium], vec![Small]], tile_layout::COLUMNS);
        assert_eq!(grid.len(), 3);
        assert_eq!((grid[0].col, grid[0].row), (0, 0));
        assert_eq!((grid[1].col, grid[1].row), (2, 0));
        // The second group starts below the two rows the first one took.
        assert_eq!((grid[2].col, grid[2].row), (0, 2));
    }

    #[test]
    fn sideways_moves_to_the_next_tile_on_the_same_band() {
        let grid = tile_grid(&[vec![Medium, Medium, Medium]], tile_layout::COLUMNS);
        assert_eq!(tile_step(&grid, 0, Dir::Right), 1);
        assert_eq!(tile_step(&grid, 1, Dir::Right), 2);
        assert_eq!(tile_step(&grid, 2, Dir::Left), 1);
        // Off the edge keeps the tile rather than losing the highlight.
        assert_eq!(tile_step(&grid, 2, Dir::Right), 2);
        assert_eq!(tile_step(&grid, 0, Dir::Left), 0);
    }

    #[test]
    fn down_from_a_wide_tile_keeps_the_column_under_the_cursor() {
        // Wide across cells 0-3 on rows 0-1, two Smalls tucked at 4 and 5,
        // a Medium filling 4-5 on rows 1-2, then two Mediums on rows 2-3.
        let grid = tile_grid(
            &[vec![Wide, Small, Small, Medium, Medium, Medium]],
            tile_layout::COLUMNS,
        );
        assert_eq!((grid[1].col, grid[1].row), (4, 0));
        assert_eq!((grid[3].col, grid[3].row), (4, 1));
        assert_eq!((grid[4].col, grid[4].row), (0, 2));
        // The smalls sit inside the Wide's own row band, so Down from the
        // Wide skips them for the first tile actually below it.
        assert_eq!(tile_step(&grid, 0, Dir::Down), 4);
        // From the small at column 5, the Medium tucked right under it.
        assert_eq!(tile_step(&grid, 2, Dir::Down), 3);
        // And back up from that Medium to the small over its left half.
        assert_eq!(tile_step(&grid, 3, Dir::Up), 1);
        // Up from the bottom-left Medium reaches the Wide above it.
        assert_eq!(tile_step(&grid, 4, Dir::Up), 0);
    }

    #[test]
    fn vertical_moves_cross_from_one_group_to_the_next() {
        let grid = tile_grid(&[vec![Medium, Medium], vec![Medium]], tile_layout::COLUMNS);
        assert_eq!(tile_step(&grid, 0, Dir::Down), 2);
        assert_eq!(tile_step(&grid, 2, Dir::Up), 0);
        // Nothing below the last group, nothing above the first.
        assert_eq!(tile_step(&grid, 2, Dir::Down), 2);
        assert_eq!(tile_step(&grid, 0, Dir::Up), 0);
        // Sideways never crosses a group: the second group's lone tile has
        // no neighbour on its band.
        assert_eq!(tile_step(&grid, 2, Dir::Right), 2);
    }

    #[test]
    fn an_empty_grid_or_a_stale_index_is_harmless() {
        assert_eq!(tile_step(&[], 0, Dir::Down), 0);
        let grid = tile_grid(&[vec![Small]], tile_layout::COLUMNS);
        assert_eq!(tile_step(&grid, 7, Dir::Up), 0);
    }

    #[test]
    fn moved_sends_a_column_zone_up_and_down_and_the_grid_sideways() {
        let c = counts(5, 3, 4);
        let grid = tile_grid(&[vec![Medium, Medium, Medium]], tile_layout::COLUMNS);
        let s = Spot {
            zone: Zone::List,
            index: 1,
        };
        assert_eq!(moved(s, Dir::Down, c, &grid).index, 2);
        assert_eq!(moved(s, Dir::Right, c, &grid).index, 1);
        let t = Spot {
            zone: Zone::Tiles,
            index: 0,
        };
        assert_eq!(moved(t, Dir::Right, c, &grid).index, 1);
        assert_eq!(moved(t, Dir::Right, c, &grid).zone, Zone::Tiles);
    }

    #[test]
    fn a_row_is_scrolled_to_only_when_it_is_out_of_view() {
        // Already showing, top and bottom edge alike.
        assert_eq!(reveal(100.0, 36.0, 80.0, 400.0), None);
        assert_eq!(reveal(80.0, 36.0, 80.0, 400.0), None);
        assert_eq!(reveal(444.0, 36.0, 80.0, 400.0), None);
        // Above the fold: the row's own top becomes the offset.
        assert_eq!(reveal(40.0, 36.0, 80.0, 400.0), Some(40.0));
        // Below it: by exactly the overshoot, so the list walks one row.
        assert_eq!(reveal(480.0, 36.0, 80.0, 400.0), Some(116.0));
        // Never past the top of the content.
        assert_eq!(reveal(-5.0, 36.0, 80.0, 400.0), Some(0.0));
    }
}
