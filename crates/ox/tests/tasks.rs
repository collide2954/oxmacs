use std::time::Duration;

use ox::Isolate;
use ox::task::TaskId;
use ox::value::Value;

fn finish(isolate: &mut Isolate, task: TaskId) -> (Result<String, String>, usize) {
    for rounds in 1..10_000 {
        let report = isolate.run(Duration::from_millis(2));
        if let Some((id, result)) = report.finished.into_iter().next() {
            assert_eq!(id, task);
            let shown = match result {
                Ok(value) => Ok(isolate.write(value)),
                Err(error) => Err(error.message),
            };
            return (shown, rounds);
        }
    }
    panic!("the task never finished");
}

#[test]
fn tasks_pause_when_their_budget_runs_out_and_resume_later() {
    let mut isolate = Isolate::new();
    let spin = isolate
        .eval("(def (spin n) (if (< n 1) :done (spin (- n 1)))) spin")
        .unwrap();

    let task = isolate.spawn(spin, &[Value::Int(200_000)]).unwrap();
    let first = isolate.run(Duration::from_micros(200));

    assert_eq!(first.pending, 1);
    let (result, rounds) = finish(&mut isolate, task);
    assert_eq!(result, Ok(":done".to_owned()));
    assert!(rounds > 1);
}

#[test]
fn several_tasks_share_the_time() {
    let mut isolate = Isolate::new();
    let count = isolate
        .eval("(def (count-to n acc) (if (< n 1) acc (count-to (- n 1) (+ acc 1)))) count-to")
        .unwrap();

    let first = isolate
        .spawn(count, &[Value::Int(50_000), Value::Int(0)])
        .unwrap();
    let second = isolate
        .spawn(count, &[Value::Int(10), Value::Int(0)])
        .unwrap();
    let mut order = Vec::new();
    for _ in 0..10_000 {
        if order.len() == 2 {
            break;
        }
        let report = isolate.run(Duration::from_micros(300));
        for (id, result) in report.finished {
            order.push((id, isolate.write(result.unwrap())));
        }
    }

    assert_eq!(
        order,
        [(second, "10".to_owned()), (first, "50000".to_owned())]
    );
}

#[test]
fn an_interrupt_stops_a_runaway_task_through_catch_but_not_finally() {
    let mut isolate = Isolate::new();
    let stubborn = isolate
        .eval("(def log (atom [])) (def (forever) (forever)) (def (stubborn) (try (forever) (catch _ e :caught) (finally (swap! log conj :cleanup)))) stubborn")
        .unwrap();
    let interrupter = isolate.interrupter();

    let task = isolate.spawn(stubborn, &[]).unwrap();
    assert_eq!(isolate.run(Duration::from_millis(1)).pending, 1);
    interrupter.interrupt();

    assert_eq!(finish(&mut isolate, task).0, Err("Interrupted".to_owned()));
    let log = isolate.eval("@log").unwrap();
    assert_eq!(isolate.write(log), "[:cleanup]");
}

#[test]
fn a_cancelled_task_ends_with_an_interrupt() {
    let mut isolate = Isolate::new();
    let forever = isolate.eval("(def (forever) (forever)) forever").unwrap();

    let task = isolate.spawn(forever, &[]).unwrap();
    assert_eq!(isolate.run(Duration::from_millis(1)).pending, 1);
    assert!(isolate.cancel(task));

    assert_eq!(finish(&mut isolate, task).0, Err("Interrupted".to_owned()));
    assert!(!isolate.cancel(task));
}

#[test]
fn errors_in_tasks_are_reported() {
    let mut isolate = Isolate::new();
    let divide = isolate.eval("(fn [] (/ 1 0))").unwrap();

    let task = isolate.spawn(divide, &[]).unwrap();

    assert_eq!(
        finish(&mut isolate, task).0,
        Err("Division by zero".to_owned())
    );
}
