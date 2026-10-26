use ox_library::animation::{Animation, Direction, Frame};

fn frames_at(animation: &Animation, times: &[f64]) -> Vec<Option<usize>> {
    times
        .iter()
        .map(|seconds| animation.frame_at(*seconds))
        .collect()
}

#[test]
fn forward_animations_loop_through_their_frames() {
    let animation = Animation::uniform([10, 11, 12], 0.1);
    assert_eq!(
        frames_at(&animation, &[0.0, 0.15, 0.25, 0.35]),
        [Some(10), Some(11), Some(12), Some(10)]
    );
    assert!(!animation.finished(100.0));
}

#[test]
fn reverse_and_ping_pong_change_the_order() {
    let mut animation = Animation::uniform([10, 11, 12], 0.1);
    animation.direction = Direction::Reverse;
    assert_eq!(
        frames_at(&animation, &[0.05, 0.15, 0.25, 0.35]),
        [Some(12), Some(11), Some(10), Some(12)]
    );
    let mut bounce = Animation::uniform([0, 1, 2, 3], 0.1);
    bounce.direction = Direction::PingPong;
    assert_eq!(
        frames_at(&bounce, &[0.05, 0.35, 0.45, 0.55, 0.65, 0.75]),
        [Some(0), Some(3), Some(2), Some(1), Some(0), Some(1)]
    );
    bounce.direction = Direction::PingPongReverse;
    assert_eq!(
        frames_at(&bounce, &[0.05, 0.35, 0.45, 0.65]),
        [Some(3), Some(0), Some(1), Some(3)]
    );
}

#[test]
fn repeats_end_on_the_last_frame() {
    let mut animation = Animation::uniform([10, 11, 12], 0.1);
    animation.repeat = Some(1);
    assert_eq!(animation.frame_at(0.35), Some(12));
    assert!(!animation.finished(0.25));
    assert!(animation.finished(0.35));
    assert!(
        animation
            .duration()
            .is_some_and(|seconds| (seconds - 0.3).abs() < 1e-9)
    );
    assert_eq!(Animation::uniform([1], 0.1).duration(), None);
    let mut bounce = Animation::uniform([0, 1, 2, 3], 0.1);
    bounce.direction = Direction::PingPong;
    bounce.repeat = Some(2);
    assert_eq!(
        frames_at(&bounce, &[0.55, 0.65, 5.0]),
        [Some(1), Some(0), Some(0)]
    );
    assert!(bounce.finished(0.71));
}

#[test]
fn frames_can_last_different_times() {
    let animation = Animation::new(vec![
        Frame {
            region: 0,
            seconds: 0.1,
        },
        Frame {
            region: 1,
            seconds: 0.3,
        },
    ]);
    assert_eq!(
        frames_at(&animation, &[0.05, 0.2, 0.39, 0.41]),
        [Some(0), Some(1), Some(1), Some(0)]
    );
    assert_eq!(Animation::new(Vec::new()).frame_at(1.0), None);
}
