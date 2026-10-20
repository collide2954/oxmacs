use std::ops::Range;

pub const GAP: f32 = 12.0;
pub const MARGIN: f32 = 16.0;
pub const MIN_SCALE: f32 = 0.25;
pub const MAX_SCALE: f32 = 8.0;
const PIXELS_PER_POINT: f32 = 96.0 / 72.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    FitWidth,
    FitPage,
    Scale(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub top: f32,
    pub size: Size,
}

impl Placed {
    fn bottom(&self) -> f32 {
        self.top + self.size.height
    }
}

pub fn scale(zoom: Zoom, page: Size, viewport: Size) -> f32 {
    let across = (viewport.width - 2.0 * MARGIN).max(1.0) / page.width.max(1.0);
    match zoom {
        Zoom::FitWidth => across,
        Zoom::FitPage => across.min((viewport.height - 2.0 * GAP).max(1.0) / page.height.max(1.0)),
        Zoom::Scale(scale) => scale * PIXELS_PER_POINT,
    }
}

pub fn place(pages: &[Size], zoom: Zoom, viewport: Size) -> (Vec<Placed>, f32) {
    let mut top = GAP;
    let placed = pages
        .iter()
        .map(|page| {
            let scale = scale(zoom, *page, viewport);
            let size = Size {
                width: page.width * scale,
                height: page.height * scale,
            };
            let placed = Placed { top, size };
            top += size.height + GAP;
            placed
        })
        .collect();
    (placed, top)
}

pub fn visible(placed: &[Placed], offset: f32, height: f32) -> Range<usize> {
    let first = placed
        .iter()
        .position(|page| page.bottom() > offset)
        .unwrap_or(placed.len());
    let end = placed
        .iter()
        .position(|page| page.top >= offset + height)
        .unwrap_or(placed.len());
    first..end.max(first)
}

pub fn page_at(placed: &[Placed], offset: f32, height: f32) -> usize {
    let line = offset + height / 3.0;
    placed
        .iter()
        .filter(|page| page.top <= line)
        .count()
        .saturating_sub(1)
}

pub fn zoomed(current: f32, factor: f32) -> Zoom {
    let scale = (current / PIXELS_PER_POINT * factor).clamp(MIN_SCALE, MAX_SCALE);
    Zoom::Scale((scale * 100.0).round() / 100.0)
}
#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: Size = Size {
        width: 612.0,
        height: 792.0,
    };
    const WIDE: Size = Size {
        width: 792.0,
        height: 612.0,
    };
    const WINDOW: Size = Size {
        width: 644.0,
        height: 500.0,
    };

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn fit_width_fills_the_window_between_the_margins() {
        assert!(close(scale(Zoom::FitWidth, LETTER, WINDOW), 1.0));
    }

    #[test]
    fn fit_page_shows_the_whole_page() {
        let scale = scale(Zoom::FitPage, LETTER, WINDOW);

        assert!(close(LETTER.height * scale, WINDOW.height - 2.0 * GAP));
    }

    #[test]
    fn a_fixed_scale_counts_points_as_pixels_at_96_dpi() {
        assert!(close(scale(Zoom::Scale(1.0), LETTER, WINDOW), 96.0 / 72.0));
        assert!(close(scale(Zoom::Scale(2.0), LETTER, WINDOW), 192.0 / 72.0));
    }

    #[test]
    fn pages_stack_with_gaps_each_at_its_own_scale() {
        let (placed, total) = place(&[LETTER, WIDE], Zoom::FitWidth, WINDOW);

        assert_eq!(placed.len(), 2);
        assert!(close(placed[0].top, GAP));
        assert!(close(placed[0].size.width, 612.0));
        assert!(close(placed[0].size.height, 792.0));
        assert!(close(placed[1].top, GAP + 792.0 + GAP));
        assert!(close(placed[1].size.width, 612.0));
        assert!(close(placed[1].size.height, 612.0 * 612.0 / 792.0));
        assert!(close(total, placed[1].top + placed[1].size.height + GAP));
    }

    #[test]
    fn visible_pages_overlap_the_viewport() {
        let (placed, _) = place(&[LETTER; 5], Zoom::FitWidth, WINDOW);

        assert_eq!(visible(&placed, 0.0, 500.0), 0..1);
        assert_eq!(visible(&placed, 700.0, 500.0), 0..2);
        assert_eq!(visible(&placed, 2000.0, 500.0), 2..4);
        assert_eq!(visible(&placed, 100_000.0, 500.0), 5..5);
        assert_eq!(visible(&[], 0.0, 500.0), 0..0);
    }

    #[test]
    fn the_current_page_is_the_one_under_the_top_third() {
        let (placed, _) = place(&[LETTER; 5], Zoom::FitWidth, WINDOW);

        assert_eq!(page_at(&placed, 0.0, 600.0), 0);
        assert_eq!(page_at(&placed, 700.0, 600.0), 1);
        assert_eq!(page_at(&placed, 100_000.0, 600.0), 4);
        assert_eq!(page_at(&[], 0.0, 600.0), 0);
    }

    #[test]
    fn zooming_starts_from_the_scale_shown_and_stays_in_bounds() {
        assert_eq!(zoomed(96.0 / 72.0, 1.25), Zoom::Scale(1.25));
        assert_eq!(zoomed(96.0 / 72.0, 0.8), Zoom::Scale(0.8));
        assert_eq!(zoomed(7.0 * 96.0 / 72.0, 2.0), Zoom::Scale(MAX_SCALE));
        assert_eq!(zoomed(0.2 * 96.0 / 72.0, 0.5), Zoom::Scale(MIN_SCALE));
    }
}
