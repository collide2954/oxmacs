#[expect(
    clippy::cast_precision_loss,
    reason = "screen coordinates stay far below the f32 precision limit"
)]
pub fn to_f32(value: usize) -> f32 {
    value as f32
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is finite, positive and floored before the cast"
)]
pub fn floor_to_usize(value: f32) -> usize {
    if value.is_finite() && value > 0.0 {
        value.floor() as usize
    } else {
        0
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "callers pass whole numbers of rows that fit comfortably in isize"
)]
pub fn trunc_to_isize(value: f32) -> isize {
    if value.is_finite() {
        value.trunc() as isize
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_rounds_down_to_whole_rows() {
        assert_eq!(floor_to_usize(3.0), 3);
        assert_eq!(floor_to_usize(3.99), 3);
    }

    #[test]
    fn floor_clamps_invalid_values_to_zero() {
        assert_eq!(floor_to_usize(-2.5), 0);
        assert_eq!(floor_to_usize(f32::NAN), 0);
    }
}
