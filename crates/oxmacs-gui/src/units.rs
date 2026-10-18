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
    clippy::cast_precision_loss,
    reason = "row offsets stay far below the f32 precision limit"
)]
pub fn signed_to_f32(value: isize) -> f32 {
    value as f32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "callers pass row offsets that fit comfortably in isize"
)]
pub fn floor_to_isize(value: f32) -> isize {
    if value.is_finite() {
        value.floor() as isize
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

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to the u16 range before the cast"
)]
pub fn portion(weight: f32) -> u16 {
    if weight.is_finite() {
        (weight * 10_000.0).round().clamp(1.0, f32::from(u16::MAX)) as u16
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_become_layout_portions() {
        assert_eq!(portion(0.5), 5_000);
        assert_eq!(portion(0.0), 1);
        assert_eq!(portion(f32::NAN), 1);
    }

    #[test]
    fn floor_rounds_down_to_whole_rows() {
        assert_eq!(floor_to_usize(3.0), 3);
        assert_eq!(floor_to_usize(3.99), 3);
    }

    #[test]
    fn signed_floors_round_toward_negative_infinity() {
        assert_eq!(floor_to_isize(1.5), 1);
        assert_eq!(floor_to_isize(-1.5), -2);
        assert_eq!(floor_to_isize(f32::NAN), 0);
        assert!((signed_to_f32(-3) + 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn floor_clamps_invalid_values_to_zero() {
        assert_eq!(floor_to_usize(-2.5), 0);
        assert_eq!(floor_to_usize(f32::NAN), 0);
    }
}
