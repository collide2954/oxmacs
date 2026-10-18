#[expect(
    clippy::cast_precision_loss,
    reason = "screen coordinates stay far below the f32 precision limit"
)]
pub fn to_f32(value: usize) -> f32 {
    value as f32
}
