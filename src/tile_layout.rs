//! Where each pinned tile sits in its group's grid.
//!
//! Windows 10 groups are six small cells wide. Tiles are placed first-fit in
//! the user's order — the packer never reorders, so a tile stays where the
//! user dropped it, and a gap it cannot fill stays a gap. Adapted from
//! cosmic-control-center-applet's packer, for three sizes and 0-based output.

use crate::config::TileSize;

pub const COLUMNS: u16 = 6;

/// (columns, rows) a tile of `size` covers.
pub fn footprint(size: TileSize) -> (u16, u16) {
    match size {
        TileSize::Small => (1, 1),
        TileSize::Medium => (2, 2),
        TileSize::Wide => (4, 2),
    }
}

/// A tile's top-left cell and extent, all 0-based cell units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub col: u16,
    pub row: u16,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// One per input size, in the same order.
    pub tiles: Vec<Placement>,
    /// Rows the grid needs, so the caller can size its box.
    pub rows: u16,
}

pub fn pack(sizes: &[TileSize]) -> Pack {
    let mut occupied: Vec<[bool; COLUMNS as usize]> = Vec::new();
    let mut tiles = Vec::with_capacity(sizes.len());
    for &size in sizes {
        let (w, h) = footprint(size);
        let (w, h) = (w as usize, h as usize);
        // Terminates: every footprint fits an empty row band, and the grid
        // grows one row per pass.
        let mut row = 0usize;
        let col = loop {
            while occupied.len() < row + h {
                occupied.push([false; COLUMNS as usize]);
            }
            let fits = |c: usize| (0..w).all(|dc| (0..h).all(|dr| !occupied[row + dr][c + dc]));
            if let Some(c) = (0..=(COLUMNS as usize - w)).find(|&c| fits(c)) {
                break c;
            }
            row += 1;
        };
        for dc in 0..w {
            for dr in 0..h {
                occupied[row + dr][col + dc] = true;
            }
        }
        tiles.push(Placement {
            col: col as u16,
            row: row as u16,
            cols: w as u16,
            rows: h as u16,
        });
    }
    let rows = occupied
        .iter()
        .rposition(|r| r.iter().any(|&c| c))
        .map_or(0, |i| i as u16 + 1);
    Pack { tiles, rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TileSize::*;

    fn p(col: u16, row: u16, s: TileSize) -> Placement {
        let (cols, rows) = footprint(s);
        Placement {
            col,
            row,
            cols,
            rows,
        }
    }

    #[test]
    fn three_mediums_fill_a_row() {
        let pk = pack(&[Medium, Medium, Medium]);
        assert_eq!(
            pk.tiles,
            [p(0, 0, Medium), p(2, 0, Medium), p(4, 0, Medium)]
        );
        assert_eq!(pk.rows, 2);
    }

    #[test]
    fn smalls_tuck_into_the_gap_beside_a_wide() {
        let pk = pack(&[Wide, Small, Small, Small, Small]);
        assert_eq!(
            pk.tiles,
            [
                p(0, 0, Wide),
                p(4, 0, Small),
                p(5, 0, Small),
                p(4, 1, Small),
                p(5, 1, Small)
            ]
        );
        assert_eq!(pk.rows, 2);
    }

    #[test]
    fn a_wide_that_does_not_fit_starts_a_new_band() {
        let pk = pack(&[Medium, Medium, Wide]);
        assert_eq!(pk.tiles[2], p(0, 2, Wide));
        assert_eq!(pk.rows, 4);
    }

    #[test]
    fn empty_is_zero_rows() {
        assert_eq!(pack(&[]).rows, 0);
    }

    #[test]
    fn no_two_tiles_overlap() {
        let sizes = [
            Small, Wide, Medium, Small, Medium, Wide, Small, Small, Medium,
        ];
        let pk = pack(&sizes);
        let mut seen = std::collections::HashSet::new();
        for t in &pk.tiles {
            for c in t.col..t.col + t.cols {
                for r in t.row..t.row + t.rows {
                    assert!(seen.insert((c, r)), "overlap at {c},{r}");
                    assert!(c < COLUMNS);
                }
            }
        }
    }
}
