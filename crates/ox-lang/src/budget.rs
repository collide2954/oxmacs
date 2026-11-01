use crate::error::{Error, ErrorKind};

pub const MOST_BYTES: u64 = 1 << 29;
pub const MOST_SIDE: u64 = 32_768;
pub const MOST_PIXELS: u64 = MOST_BYTES / 4;

pub fn fits(count: u64, each: u64) -> bool {
    count
        .checked_mul(each)
        .is_some_and(|total| total <= MOST_BYTES)
}

pub fn fits_area(width: u64, height: u64) -> bool {
    width <= MOST_SIDE && height <= MOST_SIDE && width.saturating_mul(height) <= MOST_PIXELS
}

pub fn too_big(name: &str, count: u64, what: &str) -> Error {
    Error::new(format!(
        "{name} would make {count} {what}, more than the {} MB one value can take",
        MOST_BYTES >> 20
    ))
    .of(ErrorKind::Range)
}

pub fn checked(name: &str, count: u64, each: u64, what: &str) -> Result<usize, Error> {
    if fits(count, each) {
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    } else {
        Err(too_big(name, count, what))
    }
}

pub fn widest(height_per_width: f64) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "the limits are far below 2^52")]
    let (pixels, side) = (MOST_PIXELS as f64, MOST_SIDE as f64);
    let tall = height_per_width.max(f64::MIN_POSITIVE);
    (pixels / tall).sqrt().min(side / tall).min(side)
}
