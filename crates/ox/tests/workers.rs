use std::fs;
use std::sync::Arc;
use std::time::Duration;

use ox::Isolate;
use ox::value::Value;

const JOBS: &str = r#"
(defrecord hit [file line])
(deferror job-error [n] "Job {n} failed")
(def (square n) (* n n))
(def (echo x) x)
(def (find) (hit "a.ox" 3))
(def (divide) (/ 1 0))
(def (job-fail n) (raise (job-error n)))
(def (produce out n)
  ((fn step [i] (when (< i n) (send out i) (step (+ i 1)))) 0)
  (close out)
  :sent)
(def (relay in out)
  ((fn step [] (let [x (receive in)] (unless (done? in) (send out (* x 10)) (step)))))
  (close out))
(def (abandon) (spawn ((fn forever [] (forever)))) :returned)
(def (wait-on p) (await p))
(def (snapshot-length s) (snapshot-text-length s))
"#;

fn with_jobs() -> (tempfile::TempDir, Isolate) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("jobs.ox"), JOBS).unwrap();
    let mut isolate = Isolate::new();
    isolate.add_root(dir.path());
    (dir, isolate)
}

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    match isolate.eval(source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

#[test]
fn a_worker_runs_a_module_function_and_its_promise_gets_the_result() {
    let (_dir, mut isolate) = with_jobs();

    assert_eq!(
        evaluated(&mut isolate, "(await (worker 'jobs 'square 7))"),
        "49"
    );
    assert_eq!(
        evaluated(
            &mut isolate,
            "(map await (map (fn [n] (worker 'jobs 'square n)) [1 2 3]))"
        ),
        "[1 4 9]"
    );
}

#[test]
fn data_arrives_as_it_was_sent() {
    let (_dir, mut isolate) = with_jobs();

    assert_eq!(
        evaluated(
            &mut isolate,
            r#"(await (worker 'jobs 'echo {a: [1 2.5 "s" :k #{nil \x}] b: (list 'x 99999999999999999999) c: -0.0}))"#
        ),
        r#"{a: [1 2.5 "s" :k #{nil \x}] b: (x 99999999999999999999) c: -0.0}"#
    );
}

#[test]
fn records_arrive_as_the_receivers_type_when_it_has_one() {
    let (_dir, mut isolate) = with_jobs();

    assert_eq!(
        evaluated(
            &mut isolate,
            "(let [h (await (worker 'jobs 'find))] [h h.file])"
        ),
        r#"[#hit{file: "a.ox" line: 3} "a.ox"]"#
    );
    assert_eq!(
        evaluated(
            &mut isolate,
            r#"(module (import (jobs))) (= (await (worker 'jobs 'find)) (jobs/hit "a.ox" 3))"#
        ),
        "true"
    );
}

#[test]
fn errors_in_a_worker_are_raised_by_await() {
    let (_dir, mut isolate) = with_jobs();

    assert_eq!(
        evaluated(
            &mut isolate,
            "(try (await (worker 'jobs 'divide)) (catch zero-division-error e e.message))"
        ),
        r#""Division by zero""#
    );
    assert_eq!(
        evaluated(
            &mut isolate,
            "(try (await (worker 'jobs 'job-fail 4)) (catch error e e.message))"
        ),
        r#""Job 4 failed""#
    );
    assert_eq!(
        evaluated(&mut isolate, "(await (worker 'nowhere 'run))"),
        "error: No module named nowhere"
    );
    assert_eq!(
        evaluated(&mut isolate, "(await (worker 'jobs 'missing))"),
        "error: jobs has no function named missing"
    );
}

#[test]
fn only_data_can_be_sent_to_a_worker() {
    let (_dir, mut isolate) = with_jobs();

    assert_eq!(
        evaluated(&mut isolate, "(worker 'jobs 'echo (fn [] 1))"),
        "error: #<fn> can't be sent to a worker"
    );
    assert_eq!(
        evaluated(&mut isolate, "(worker 'jobs 'wait-on (promise))"),
        "error: #<promise> can't be sent to a worker"
    );
}

#[test]
fn channels_stream_values_between_isolates() {
    let (_dir, mut isolate) = with_jobs();

    assert_eq!(
        evaluated(
            &mut isolate,
            "(let [ch (chan 2) done (worker 'jobs 'produce ch 50)] [((fn drain [out] (let [x (receive ch)] (if (done? ch) out (drain (+ out x))))) 0) (await done)])"
        ),
        "[1225 :sent]"
    );
    assert_eq!(
        evaluated(
            &mut isolate,
            "(let [in (chan) out (chan) relay (worker 'jobs 'relay in out)] (send in 1) (send in 2) (close in) [(receive out) (receive out) (do (receive out) (done? out))])"
        ),
        "[10 20 true]"
    );
}

#[test]
fn a_worker_ends_when_its_function_returns() {
    let (_dir, mut isolate) = with_jobs();

    assert_eq!(
        evaluated(&mut isolate, "(await (worker 'jobs 'abandon))"),
        ":returned"
    );
}

#[test]
fn the_host_runs_workers_from_its_frame_loop() {
    let (_dir, mut isolate) = with_jobs();
    let started = isolate
        .eval("(def result (atom nil)) (fn [] (reset! result (await (worker 'jobs 'square 12))))")
        .unwrap();
    isolate.spawn(started, &[]).unwrap();

    for _ in 0..5_000 {
        let report = isolate.run(Duration::from_millis(1));
        if report.pending == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(evaluated(&mut isolate, "@result"), "144");
}

fn snapshot_natives(isolate: &mut Isolate) {
    isolate.define_native("snapshot-text-length", 1, Some(1), |heap, arguments| {
        let text = heap
            .foreign_of::<String>(arguments[0])
            .ok_or_else(|| ox::Error::new("expected a snapshot"))?;
        Ok(Value::Int(i64::try_from(text.len()).unwrap_or(0)))
    });
}

#[test]
fn shared_rust_values_cross_to_workers_and_keep_their_identity() {
    let (_dir, mut isolate) = with_jobs();
    isolate.set_worker_setup(snapshot_natives);
    let snapshot = isolate
        .shared("snapshot", Arc::new("hello world".to_owned()))
        .unwrap();
    isolate.define("snapshot", snapshot);

    assert_eq!(
        evaluated(
            &mut isolate,
            "(await (worker 'jobs 'snapshot-length snapshot))"
        ),
        "11"
    );
    assert_eq!(
        evaluated(
            &mut isolate,
            "(= snapshot (await (worker 'jobs 'echo snapshot)))"
        ),
        "true"
    );
    assert_eq!(evaluated(&mut isolate, "snapshot"), "#<snapshot>");
}
