use iced::Animation;
use iced::animation::Easing;
use iced::time::{Duration, Instant};

use crate::units::to_f32;

pub const BLINKS: u32 = 10;
const FADE: Duration = Duration::from_millis(60);
const GLIDE: Duration = Duration::from_millis(70);
const SCROLL: Duration = Duration::from_millis(120);

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

    pub fn retarget(&mut self, target: (usize, usize), now: Instant, enabled: bool) {
        if target == self.target {
            return;
        }
        let single_step = target.1 == self.target.1 && target.0.abs_diff(self.target.0) <= 1;
        if !enabled || single_step {
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
    target: usize,
    animation: Animation<f32>,
}

impl Scroll {
    pub fn new(top: usize) -> Self {
        Self {
            target: top,
            animation: scroll(to_f32(top)),
        }
    }

    pub fn retarget(&mut self, target: usize, rows: usize, now: Instant, enabled: bool) {
        if target == self.target {
            return;
        }
        if !enabled {
            *self = Self::new(target);
            return;
        }
        let current = self.position(now);
        let goal = to_f32(target);
        let screen = to_f32(rows);
        if (current - goal).abs() > screen {
            let start = if current < goal {
                goal - screen
            } else {
                goal + screen
            };
            self.animation = scroll(start);
        }
        self.animation.go_mut(goal, now);
        self.target = target;
    }

    pub fn position(&self, now: Instant) -> f32 {
        self.animation.interpolate_with(|top| top, now)
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.animation.is_animating(now)
    }
}

fn scroll(top: f32) -> Animation<f32> {
    Animation::new(top)
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
    fn scroll_eases_toward_the_new_top() {
        let start = Instant::now();
        let mut scroll = Scroll::new(0);

        scroll.retarget(10, 30, start, true);

        let midway = scroll.position(start + MS * 40);
        assert!(midway > 0.0 && midway < 10.0, "{midway}");
        assert!(close(scroll.position(start + MS * 500), 10.0));
        assert!(!scroll.is_animating(start + MS * 500));
    }

    #[test]
    fn long_scrolls_start_one_screen_from_the_target() {
        let start = Instant::now();
        let mut scroll = Scroll::new(0);

        scroll.retarget(1_000, 20, start, true);
        assert!(close(scroll.position(start), 980.0));

        scroll.retarget(0, 20, start + MS * 500, true);
        assert!(close(scroll.position(start + MS * 500), 20.0));
    }

    #[test]
    fn disabled_scroll_jumps_immediately() {
        let start = Instant::now();
        let mut scroll = Scroll::new(0);

        scroll.retarget(10, 30, start, false);

        assert!(close(scroll.position(start), 10.0));
    }

    #[test]
    fn disabled_animation_jumps_immediately() {
        let start = Instant::now();
        let mut glide = Glide::new((0, 0));

        glide.retarget((10, 4), start, false);

        assert!(at(&glide, start, (10.0, 4.0)));
    }
}
