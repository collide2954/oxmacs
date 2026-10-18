use iced::Animation;
use iced::animation::Easing;
use iced::time::{Duration, Instant};

use mx_core::display::ScreenLine;

use crate::units::{signed_to_f32, to_f32};

pub const BLINKS: u32 = 10;
const FADE: Duration = Duration::from_millis(60);
const GLIDE: Duration = Duration::from_millis(70);
const SCROLL: Duration = Duration::from_millis(120);
pub const THEME_FADE: Duration = Duration::from_millis(200);
pub const REVEAL: Duration = Duration::from_millis(120);

struct Phase {
    visible: bool,
    elapsed: Duration,
}

fn blink_phase(idle: Duration, interval: Duration) -> Option<Phase> {
    if interval.is_zero() {
        return None;
    }
    let since = idle.checked_sub(interval)?;
    let index = u32::try_from(since.as_nanos() / interval.as_nanos()).ok()?;
    if index >= BLINKS * 2 {
        return None;
    }
    Some(Phase {
        visible: index % 2 == 1,
        elapsed: since.saturating_sub(interval * index),
    })
}

pub fn cursor_opacity(idle: Duration, interval: Duration) -> f32 {
    let Some(phase) = blink_phase(idle, interval) else {
        return 1.0;
    };
    let fade = FADE.min(interval);
    let progress = (phase.elapsed.as_secs_f32() / fade.as_secs_f32()).min(1.0);
    let eased = Easing::EaseInOut.value(progress);
    if phase.visible { eased } else { 1.0 - eased }
}

pub fn next_blink_frame(idle: Duration, interval: Duration) -> Option<Duration> {
    if interval.is_zero() {
        return None;
    }
    if let Some(remaining) = interval.checked_sub(idle).filter(|left| !left.is_zero()) {
        return Some(remaining);
    }
    let phase = blink_phase(idle, interval)?;
    if phase.elapsed < FADE {
        Some(Duration::ZERO)
    } else {
        Some(interval.saturating_sub(phase.elapsed))
    }
}

#[derive(Debug, Clone)]
pub struct Glide {
    target: (usize, usize),
    x: Animation<f32>,
    y: Animation<f32>,
}

impl Glide {
    pub fn new(at: (usize, usize)) -> Self {
        Self {
            target: at,
            x: glide(at.0),
            y: glide(at.1),
        }
    }

    pub fn retarget(&mut self, target: (usize, usize), now: Instant, glide: bool) {
        if target == self.target {
            return;
        }
        let lines = target.1.abs_diff(self.target.1);
        let step = lines == 1 || (lines == 0 && target.0.abs_diff(self.target.0) <= 1);
        if !glide || step || self.is_animating(now) {
            *self = Self::new(target);
            return;
        }
        self.target = target;
        self.x.go_mut(to_f32(target.0), now);
        self.y.go_mut(to_f32(target.1), now);
    }

    pub fn position(&self, now: Instant) -> (f32, f32) {
        (
            self.x.interpolate_with(|x| x, now),
            self.y.interpolate_with(|y| y, now),
        )
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.x.is_animating(now) || self.y.is_animating(now)
    }
}

fn glide(value: usize) -> Animation<f32> {
    Animation::new(to_f32(value))
        .duration(GLIDE)
        .easing(Easing::EaseOutCubic)
}

#[derive(Debug, Clone)]
pub struct Scroll {
    target: ScreenLine,
    shift: Animation<f32>,
}

impl Scroll {
    pub fn new(top: ScreenLine) -> Self {
        Self {
            target: top,
            shift: scroll(0.0),
        }
    }

    pub fn target(&self) -> ScreenLine {
        self.target
    }

    pub fn retarget(
        &mut self,
        top: ScreenLine,
        moved: Option<isize>,
        rows: usize,
        now: Instant,
        enabled: bool,
    ) -> bool {
        if top == self.target {
            return false;
        }
        let screen = to_f32(rows);
        let direction = if top > self.target { screen } else { -screen };
        self.target = top;
        if !enabled {
            self.shift = scroll(0.0);
            return true;
        }
        let moved = moved.map_or(direction, signed_to_f32);
        self.shift = scroll((self.shift(now) + moved).clamp(-screen, screen));
        self.shift.go_mut(0.0, now);
        true
    }

    pub fn shift(&self, now: Instant) -> f32 {
        self.shift.interpolate_with(|shift| shift, now)
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.shift.is_animating(now)
    }
}

fn scroll(shift: f32) -> Animation<f32> {
    Animation::new(shift)
        .duration(SCROLL)
        .easing(Easing::EaseOutCubic)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    const INTERVAL: Duration = Duration::from_millis(400);

    #[test]
    fn cursor_is_solid_until_the_blink_delay() {
        assert!(close(cursor_opacity(Duration::ZERO, INTERVAL), 1.0));
        assert!(close(cursor_opacity(MS * 399, INTERVAL), 1.0));
    }

    #[test]
    fn cursor_fades_out_then_stays_hidden() {
        assert!(close(cursor_opacity(MS * 400, INTERVAL), 1.0));
        let fading = cursor_opacity(MS * 430, INTERVAL);
        assert!(fading > 0.0 && fading < 1.0, "{fading}");
        assert!(close(cursor_opacity(MS * 500, INTERVAL), 0.0));
    }

    #[test]
    fn cursor_fades_back_in_for_the_second_half() {
        let fading = cursor_opacity(MS * 830, INTERVAL);
        assert!(fading > 0.0 && fading < 1.0, "{fading}");
        assert!(close(cursor_opacity(MS * 900, INTERVAL), 1.0));
    }

    #[test]
    fn cursor_stops_blinking_after_ten_blinks() {
        let done = MS * 8_400 + MS * 100;
        assert!(close(cursor_opacity(done, INTERVAL), 1.0));
        assert_eq!(next_blink_frame(done, INTERVAL), None);
    }

    #[test]
    fn steady_phases_wait_until_the_next_phase() {
        assert_eq!(next_blink_frame(MS * 100, INTERVAL), Some(MS * 300));
        assert_eq!(next_blink_frame(MS * 500, INTERVAL), Some(MS * 300));
    }

    #[test]
    fn fades_need_the_next_frame_immediately() {
        assert_eq!(next_blink_frame(MS * 430, INTERVAL), Some(Duration::ZERO));
    }

    #[test]
    fn zero_interval_disables_blinking() {
        assert!(close(cursor_opacity(MS * 5_000, Duration::ZERO), 1.0));
        assert_eq!(next_blink_frame(MS * 5_000, Duration::ZERO), None);
        assert_eq!(next_blink_frame(Duration::ZERO, Duration::ZERO), None);
    }

    fn at(glide: &Glide, now: Instant, expected: (f32, f32)) -> bool {
        let (x, y) = glide.position(now);
        close(x, expected.0) && close(y, expected.1)
    }

    #[test]
    fn glide_reaches_its_target_after_the_duration() {
        let start = Instant::now();
        let mut glide = Glide::new((0, 0));

        glide.retarget((10, 4), start, true);

        assert!(glide.is_animating(start + MS * 10));
        let (x, y) = glide.position(start + MS * 30);
        assert!(x > 0.0 && x < 10.0 && y > 0.0 && y < 4.0, "{x} {y}");
        assert!(at(&glide, start + MS * 500, (10.0, 4.0)));
        assert!(!glide.is_animating(start + MS * 500));
    }

    #[test]
    fn single_cell_steps_jump_without_gliding() {
        let start = Instant::now();
        let mut glide = Glide::new((3, 2));

        glide.retarget((4, 2), start, true);

        assert!(at(&glide, start, (4.0, 2.0)));
        assert!(!glide.is_animating(start));
    }

    #[test]
    fn single_line_steps_jump_without_gliding() {
        let start = Instant::now();
        let mut glide = Glide::new((30, 2));

        glide.retarget((0, 3), start, true);

        assert!(at(&glide, start, (0.0, 3.0)));
        assert!(!glide.is_animating(start));
    }

    #[test]
    fn a_new_target_during_a_glide_jumps_there() {
        let start = Instant::now();
        let mut glide = Glide::new((0, 0));
        glide.retarget((10, 0), start, true);

        glide.retarget((20, 0), start + MS * 10, true);

        assert!(at(&glide, start + MS * 10, (20.0, 0.0)));
        assert!(!glide.is_animating(start + MS * 10));
    }

    fn line(line: usize) -> ScreenLine {
        ScreenLine { line, row: 0 }
    }

    #[test]
    fn scrolling_reports_when_its_target_changes() {
        let start = Instant::now();
        let mut scroll = Scroll::new(line(0));

        assert!(!scroll.retarget(line(0), Some(0), 30, start, true));
        assert!(scroll.retarget(line(5), Some(5), 30, start, true));
    }

    #[test]
    fn scrolling_shifts_the_text_back_into_place() {
        let start = Instant::now();
        let mut scroll = Scroll::new(line(0));

        scroll.retarget(line(10), Some(10), 30, start, true);

        assert!(close(scroll.shift(start), 10.0));
        let midway = scroll.shift(start + MS * 40);
        assert!(midway > 0.0 && midway < 10.0, "{midway}");
        assert!(close(scroll.shift(start + MS * 500), 0.0));
        assert!(!scroll.is_animating(start + MS * 500));
    }

    #[test]
    fn long_scrolls_start_one_screen_away() {
        let start = Instant::now();
        let mut scroll = Scroll::new(line(0));

        scroll.retarget(line(1_000), None, 20, start, true);
        assert!(close(scroll.shift(start), 20.0));

        scroll.retarget(line(0), None, 20, start + MS * 500, true);
        assert!(close(scroll.shift(start + MS * 500), -20.0));
    }

    #[test]
    fn disabled_scroll_jumps_immediately() {
        let start = Instant::now();
        let mut scroll = Scroll::new(line(0));

        scroll.retarget(line(10), Some(10), 30, start, false);

        assert!(close(scroll.shift(start), 0.0));
        assert_eq!(scroll.target(), line(10));
    }
    #[test]
    fn disabled_animation_jumps_immediately() {
        let start = Instant::now();
        let mut glide = Glide::new((0, 0));

        glide.retarget((10, 4), start, false);

        assert!(at(&glide, start, (10.0, 4.0)));
    }
}
