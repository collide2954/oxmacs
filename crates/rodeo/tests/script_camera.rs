use crate::support::evaluated;

fn camera(source: &str) -> String {
    evaluated("(rodeo camera)", source)
}

#[test]
fn cameras_are_plain_maps_that_show_the_world() {
    assert_eq!(
        camera(
            "(let [cam (camera/new [320 180])] [cam.position cam.zoom (camera/to-screen cam [0 0]) (camera/view cam)])"
        ),
        "[[160.0 90.0] 1.0 [0.0 0.0] [0.0 0.0 320.0 180.0]]"
    );
    assert_eq!(
        camera("(camera/to-screen (assoc (camera/new [320 180]) :zoom 2) [170 90])"),
        "[180.0 90.0]"
    );
    assert_eq!(
        camera(
            "(let [cam (assoc (camera/new [320 180]) :zoom 2)] (camera/to-world cam (camera/to-screen cam [12.5 300])))"
        ),
        "[12.5 300.0]"
    );
}

#[test]
fn cameras_follow_within_their_bounds() {
    assert_eq!(
        camera("(get (camera/follow (camera/new [320 180]) [260 90] 0.5) :position)"),
        "[210.0 90.0]"
    );
    assert_eq!(
        camera(
            "(get (camera/follow (assoc (camera/new [320 180]) :bounds [0 0 640 360]) [-50 1000] 1) :position)"
        ),
        "[160.0 270.0]"
    );
    assert_eq!(
        camera(
            "(get (camera/clamp (assoc (camera/new [320 180]) :bounds [0 0 640 360] :position [0 0])) :position)"
        ),
        "[160.0 90.0]"
    );
}

#[test]
fn cameras_shake_and_settle() {
    assert_eq!(
        camera(
            "(let [cam (camera/tick (camera/shake (camera/new [320 180]) 1) (/ 1 60))] (not= (camera/offset cam) [0.0 0.0]))"
        ),
        "true"
    );
    assert_eq!(
        camera(
            "(let [cam (reduce (fn [cam _] (camera/tick cam (/ 1 60))) (camera/shake (camera/new [320 180]) 1) (range 120))] [cam.trauma (camera/offset cam)])"
        ),
        "[0.0 [0.0 0.0]]"
    );
}
