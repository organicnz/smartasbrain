//! Tiny geometry helpers shared by game UIs.

use ratatui::layout::Rect;

/// True when the cell `(col, row)` lies inside `r`.
pub fn in_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Largest `width`×`height` [`Rect`] centered inside `area`, clamped to fit.
pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// A proportion-aware placement for a fixed-shape board canvas.
///
/// Each board cell may span multiple terminal columns *and* rows. Because
/// terminal glyphs are roughly twice as tall as they are wide, the scorer
/// compares `board_width` against `2 x board_height` and prefers the
/// candidate closest to a perfect square, tie-breaking toward more coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Canvas {
    /// Screen position of the top-left playable cell.
    pub x0: u16,
    pub y0: u16,
    /// Chosen cell width in terminal columns (never zero).
    pub cell_w: u16,
    /// Chosen cell height in terminal rows (never zero).
    pub cell_h: u16,
}

/// Columns occupied by `cols` cells of width `cell_w`, including single
/// spacer columns between cells wider than one.
#[must_use]
pub fn board_width(cols: u16, cell_w: u16) -> u16 {
    let spacers = cols
        .saturating_sub(1)
        .saturating_mul(cell_w.saturating_sub(1));
    cols.saturating_add(spacers)
}

/// Rows occupied by `rows` cells of height `cell_h`.
#[must_use]
pub fn board_height(rows: u16, cell_h: u16) -> u16 {
    let spacers = rows
        .saturating_sub(1)
        .saturating_mul(cell_h.saturating_sub(1));
    rows.saturating_add(spacers)
}

/// Visual footprint once the terminal's ~1:2 character aspect is
/// compensated: `w x (2h)`. Maximizing this fills the screen; ties fall
/// back to whichever block is closest to perfectly square.
fn visual_area(w: u16, h: u16) -> u32 {
    u32::from(w).saturating_mul(u32::from(h).saturating_mul(2))
}
fn imbalance(w: u16, h: u16) -> u16 {
    w.abs_diff(h.saturating_mul(2))
}

/// See [`Canvas`]. Candidates are `(cell_w, cell_h)` pairs; the best-fitting,
/// best-proportioned one wins. Returns None when nothing fits.
#[must_use]
pub fn fit_canvas(
    area: Rect,
    cols: u16,
    rows: u16,
    border: u16,
    options: &[CellSize],
) -> Option<Canvas> {
    let inner_w = area.width.checked_sub(2 * border)?;
    let inner_h = area.height.checked_sub(2 * border)?;
    let mut best: Option<(Canvas, (u32, u32))> = None;
    for &(cw, ch) in options {
        if cw == 0 || ch == 0 || cols == 0 || rows == 0 {
            continue;
        }
        let bw = board_width(cols, cw);
        let bh = board_height(rows, ch);
        if bw > inner_w || bh > inner_h {
            continue;
        }
        let canvas = Canvas {
            x0: area.x + border + (inner_w - bw) / 2,
            y0: area.y + border + (inner_h - bh) / 2,
            cell_w: cw,
            cell_h: ch,
        };
        let key = (visual_area(bw, bh), u32::from(imbalance(bw, bh)));
        let better = match best {
            None => true,
            Some((_, (ba, bd))) => {
                let (na, nd) = key;
                na > ba || (na == ba && nd < bd)
            }
        };
        if better {
            best = Some((canvas, key));
        }
    }
    best.map(|(c, _)| c)
}

/// Cell dimensions candidate.
pub type CellSize = (u16, u16);

/// Generates near-square cell candidates from roomiest to tightest.
///
/// A cell `(w, h)` qualifies when it compensates the terminal's ~1:2 glyph
/// aspect within one column (`|w - 2h| <= 1`). Boards pick the first entry
/// that fits, which is the largest possible rendition.
#[must_use]
pub fn proportional_ladder(
    cols: u16,
    rows: u16,
    max_cell_w: u16,
    max_cell_h: u16,
) -> Vec<CellSize> {
    let mut out = Vec::new();
    let hard_w = max_cell_w.max(1);
    let hard_h = max_cell_h.max(1);
    for h in 1..=hard_h {
        for w in 1..=hard_w {
            if w.abs_diff(h.saturating_mul(2)) <= 1 {
                out.push((w, h));
            }
        }
    }
    // Roomiest first: descending visual footprint.
    out.sort_by(|a, b| {
        let a_area = board_width(cols, a.0).saturating_mul(board_height(rows, a.1));
        let b_area = board_width(cols, b.0).saturating_mul(board_height(rows, b.1));
        b_area.cmp(&a_area)
    });
    out
}

/// Convenience wrapper: single-row cells at the given widths (legacy shape).
#[must_use]
pub fn fit_canvas_rows(
    area: Rect,
    cols: u16,
    rows: u16,
    border: u16,
    cell_options: &[u16],
) -> Option<Canvas> {
    let owned: Vec<CellSize> = cell_options.iter().map(|&w| (w, 1)).collect();
    fit_canvas(area, cols, rows, border, &owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_rect_bounds_are_exclusive_on_far_edges() {
        let r = Rect::new(2, 3, 4, 2);
        assert!(in_rect(r, 2, 3));
        assert!(in_rect(r, 5, 4));
        assert!(!in_rect(r, 6, 3));
        assert!(!in_rect(r, 2, 5));
    }

    #[test]
    fn canvas_prefers_widest_cells_then_falls_back() {
        let area = Rect::new(0, 0, 60, 20);
        let c = fit_canvas_rows(area, 9, 9, 1, &[2, 1]).expect("wide fits");
        assert_eq!((c.cell_w, c.cell_h), (2, 1));
        assert_eq!(c.x0, 1 + (58 - 17) / 2);

        // Narrow terminal: only single-width cells squeeze in.
        let c = fit_canvas_rows(Rect::new(0, 0, 11, 11), 9, 9, 1, &[2, 1]).expect("compact fits");
        assert_eq!((c.cell_w, c.cell_h), (1, 1));
        assert_eq!(c.x0, 1);
        assert!(fit_canvas_rows(Rect::new(0, 0, 5, 5), 9, 9, 1, &[2, 1]).is_none());
    }

    #[test]
    fn board_width_accounts_for_spacers() {
        assert_eq!(board_width(9, 2), 17);
        assert_eq!(board_width(9, 1), 9);
        assert_eq!(board_width(7, 3), 19);
    }

    #[test]
    fn tall_cells_fill_big_screens_and_stay_square() {
        // 80x40 with double-height cells: 9x9 board spans 17x17 -> square.
        let c = fit_canvas(Rect::new(0, 0, 80, 40), 9, 9, 1, &[(2, 2), (2, 1), (1, 1)])
            .expect("big screen fits");
        assert_eq!((c.cell_w, c.cell_h), (2, 2));

        // Same screen without tall cells allowed degrades gracefully.
        let c = fit_canvas(Rect::new(0, 0, 80, 40), 9, 9, 1, &[(2, 1)]).expect("flat fits");
        assert_eq!((c.cell_w, c.cell_h), (2, 1));
    }

    #[test]
    fn ladder_orders_big_to_small_and_stays_proportioned() {
        let ladder = proportional_ladder(9, 9, 13, 6);
        assert_eq!(ladder.first(), Some(&(13, 6)), "roomiest first");
        assert!(ladder.contains(&(2, 1)));
        assert!(ladder.contains(&(1, 1)));
        // Every entry compensates the 1:2 aspect within one column.
        for (w, h) in &ladder {
            assert!(w.abs_diff(h.saturating_mul(2)) <= 1);
        }
    }

    #[test]
    fn proportion_scoring_prefers_the_squarest_option() {
        // Wide-short terminal: flat cells beat tall ones on squareness even
        // though both fit.
        let c = fit_canvas(Rect::new(0, 0, 60, 14), 9, 9, 1, &[(2, 2), (2, 1)]).expect("fits");
        assert_eq!((c.cell_w, c.cell_h), (2, 1));
    }

    #[test]
    fn centered_rect_clamps_to_area() {
        let area = Rect::new(0, 0, 20, 10);
        assert_eq!(centered_rect(area, 30, 4), Rect::new(0, 3, 20, 4));
        assert_eq!(centered_rect(area, 10, 4), Rect::new(5, 3, 10, 4));
    }
}
