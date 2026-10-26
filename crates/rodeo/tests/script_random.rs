use crate::support::evaluated;

fn random(source: &str) -> String {
    evaluated("(rodeo random)", source)
}

#[test]
fn generators_are_atoms_of_plain_state() {
    assert_eq!(
        random("(let [rng (random/new 0)] [(atom? rng) (count @rng)])"),
        "[true 4]"
    );
    assert_eq!(
        random(
            "(let [a (random/new 42) b (random/new 42)] (= [(random/float a) (random/float a)] [(random/float b) (random/float b)]))"
        ),
        "true"
    );
    assert_eq!(
        random(
            "(let [rng (random/new 7) saved @rng first (random/float rng)] (reset! rng saved) (= first (random/float rng)))"
        ),
        "true"
    );
}

#[test]
fn draws_stay_in_their_ranges() {
    assert_eq!(
        random(
            "(let [rng (random/new 3)] (every? (fn [_] (let [n (random/int rng 2 5)] (and (>= n 2) (< n 5)))) (range 200)))"
        ),
        "true"
    );
    assert_eq!(
        random("(let [rng (random/new 3)] (every? (fn [_] (< (random/int rng 6) 6)) (range 200)))"),
        "true"
    );
    assert_eq!(
        random("(let [rng (random/new 3) x (random/range rng 2 4)] (and (>= x 2) (< x 4)))"),
        "true"
    );
    assert_eq!(random("(random/chance? (random/new 1) 1)"), "true");
    assert_eq!(random("(random/chance? (random/new 1) 0)"), "false");
}

#[test]
fn picks_shuffles_and_noise() {
    assert_eq!(
        random("(contains? #{:a :b :c} (random/pick (random/new 5) [:a :b :c]))"),
        "true"
    );
    assert_eq!(random("(random/pick (random/new 5) [])"), "nil");
    assert_eq!(
        random("(sort (random/shuffle (random/new 5) [3 1 2]))"),
        "[1 2 3]"
    );
    assert_eq!(random("(= (random/noise 1 2) (random/noise 1 2))"), "true");
    assert_eq!(
        random("(random/float (atom 3))"),
        "error: random/float expects a generator from random/new, got #<atom 3>"
    );
}
