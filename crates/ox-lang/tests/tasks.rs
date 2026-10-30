use std::time::Duration;

use ox_lang::error::ErrorKind;
use ox_lang::task::TaskId;
use ox_lang::value::Value;
use ox_lang::{Error, Isolate};

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
fn swaps_from_tasks_that_share_time_never_lose_updates() {
    let mut isolate = Isolate::new();
    let bump = isolate
        .eval(
            "(def total (atom 0))
             (def (slow n k) (if (< k 1) (+ n 1) (slow n (- k 1))))
             (fn [] (swap! total slow 300) (swap! total slow 300) (swap! total slow 300))",
        )
        .unwrap();
    for _ in 0..20 {
        isolate.spawn(bump, &[]).unwrap();
    }

    for _ in 0..100_000 {
        if isolate.run(Duration::from_micros(50)).pending == 0 {
            break;
        }
    }

    let total = isolate.eval("@total").unwrap();
    assert_eq!(isolate.write(total), "60");
}

fn interleaving(atomic: bool) -> String {
    let mut isolate = Isolate::new();
    let (open, close) = if atomic {
        ("(atomically", ")")
    } else {
        ("(do", ")")
    };
    let long = isolate
        .eval(&format!(
            "(def log (atom []))
             (def (spin n) (if (< n 1) nil (spin (- n 1))))
             (fn [] {open} (reset! log (conj @log :start)) (spin 200000) (reset! log (conj @log :end)){close})"
        ))
        .unwrap();
    let short = isolate
        .eval("(fn [] (reset! log (conj @log :other)))")
        .unwrap();
    isolate.spawn(long, &[]).unwrap();
    isolate.spawn(short, &[]).unwrap();
    for _ in 0..100_000 {
        if isolate.run(Duration::from_micros(50)).pending == 0 {
            break;
        }
    }
    let entries = isolate.eval("@log").unwrap();
    isolate.write(entries)
}

#[test]
fn atomic_blocks_finish_before_another_task_runs() {
    assert_eq!(interleaving(false), "[:start :other :end]");
    assert_eq!(interleaving(true), "[:start :end :other]");
}

#[test]
fn atomic_blocks_cannot_wait_and_end_cleanly_when_they_fail() {
    let mut isolate = Isolate::new();
    let waits = isolate
        .eval("(fn [] (try (atomically (await (promise))) (catch error e e.message)))")
        .unwrap();
    let task = isolate.spawn(waits, &[]).unwrap();

    let (result, _) = finish(&mut isolate, task);

    assert_eq!(
        result,
        Ok(r#""atomically blocks can't wait; await outside the block""#.to_owned())
    );
    assert_eq!(interleaving(false), "[:start :other :end]");
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
fn tasks_wait_for_promises_the_host_resolves() {
    let mut isolate = Isolate::new();
    let promise = isolate.promise().unwrap();
    let held = isolate.root(promise);
    let wait = isolate.eval("(fn [p] (+ 1 (await p)))").unwrap();

    let task = isolate.spawn(wait, &[promise]).unwrap();
    assert_eq!(isolate.run(Duration::from_millis(2)).pending, 1);
    assert_eq!(isolate.run(Duration::from_millis(2)).pending, 1);
    assert!(isolate.resolve(held.get(), Value::Int(41)));

    assert_eq!(finish(&mut isolate, task).0, Ok("42".to_owned()));
    assert!(!isolate.resolve(held.get(), Value::Int(0)));
}

#[test]
fn rejected_promises_raise_in_the_tasks_that_wait() {
    let mut isolate = Isolate::new();
    let promise = isolate.promise().unwrap();
    let held = isolate.root(promise);
    let wait = isolate
        .eval("(fn [p] (try (await p) (catch error e e.message)))")
        .unwrap();

    let task = isolate.spawn(wait, &[promise]).unwrap();
    isolate.run(Duration::from_millis(1));
    assert!(isolate.reject(held.get(), &Error::new("offline")));

    assert_eq!(finish(&mut isolate, task).0, Ok("\"offline\"".to_owned()));
}

#[test]
fn a_quit_passes_error_handlers_and_keeps_its_kind_when_nothing_catches_it() {
    let mut isolate = Isolate::new();
    let quit = Error::new("Quit").of(ErrorKind::Quit);
    let caught = isolate.promise().unwrap();
    let held = isolate.root(caught);
    let wait = isolate
        .eval("(fn [p] (try (try (await p) (catch error e :error)) (catch quit e e.message)))")
        .unwrap();
    let task = isolate.spawn(wait, &[caught]).unwrap();
    isolate.run(Duration::from_millis(1));
    assert!(isolate.reject(held.get(), &quit));
    assert_eq!(finish(&mut isolate, task).0, Ok("\"Quit\"".to_owned()));

    let uncaught = isolate.promise().unwrap();
    let held = isolate.root(uncaught);
    let wait = isolate.eval("(fn [p] (await p))").unwrap();
    isolate.spawn(wait, &[uncaught]).unwrap();
    isolate.run(Duration::from_millis(1));
    assert!(isolate.reject(held.get(), &quit));
    let failed = loop {
        let report = isolate.run(Duration::from_millis(2));
        if let Some((_, result)) = report.finished.into_iter().next() {
            break result.err();
        }
    };

    assert_eq!(
        failed.map(|error| (error.kind, error.message)),
        Some((ErrorKind::Quit, "Quit".to_owned()))
    );
}

#[test]
fn a_waiting_task_can_be_cancelled() {
    let mut isolate = Isolate::new();
    let promise = isolate.promise().unwrap();
    let _held = isolate.root(promise);
    let wait = isolate.eval("(fn [p] (await p))").unwrap();

    let task = isolate.spawn(wait, &[promise]).unwrap();
    isolate.run(Duration::from_millis(1));
    assert!(isolate.cancel(task));

    assert_eq!(finish(&mut isolate, task).0, Err("Interrupted".to_owned()));
}

#[test]
fn host_tasks_that_finish_while_eval_waits_are_still_reported() {
    let mut isolate = Isolate::new();
    let quick = isolate.eval("(fn [] :quick)").unwrap();
    let task = isolate.spawn(quick, &[]).unwrap();

    let waited = isolate.eval("(await (spawn :inner))").unwrap();
    assert_eq!(isolate.write(waited), ":inner");

    let report = isolate.run(Duration::from_millis(1));
    let finished: Vec<(TaskId, String)> = report
        .finished
        .into_iter()
        .map(|(id, result)| (id, isolate.write(result.unwrap())))
        .collect();
    assert_eq!(finished, [(task, ":quick".to_owned())]);
}

#[test]
fn failures_nothing_waits_for_are_reported_as_unhandled() {
    let mut isolate = Isolate::new();
    isolate
        .eval("(spawn (raise (error \"lost\"))) (def awaited (spawn (raise (error \"seen\"))))")
        .unwrap();
    isolate
        .eval("(try (await awaited) (catch error e nil))")
        .unwrap();

    let report = isolate.run(Duration::from_millis(1));
    let unhandled: Vec<String> = report
        .unhandled
        .into_iter()
        .map(|error| error.message)
        .collect();
    assert_eq!(unhandled, ["lost"]);
}

#[test]
fn suspended_generators_and_waiting_tasks_survive_collection() {
    let mut isolate = Isolate::new();
    isolate
        .eval("(def g (gen (let [s (str \"kept\" \"!\")] (yield 1) (yield s)))) (next g)")
        .unwrap();
    let promise = isolate.promise().unwrap();
    let held = isolate.root(promise);
    let wait = isolate
        .eval("(fn [p] (let [v [(str \"a\" \"b\")]] (await p) v))")
        .unwrap();
    let task = isolate.spawn(wait, &[promise]).unwrap();
    isolate.run(Duration::from_millis(1));

    isolate.collect();
    isolate
        .eval("(def junk (map (fn [i] [i (str i)]) (range 2000))) (def junk nil)")
        .unwrap();
    isolate.collect();

    let next = isolate.eval("(next g)").unwrap();
    assert_eq!(isolate.write(next), "\"kept!\"");
    isolate.resolve(held.get(), Value::Nil);
    assert_eq!(finish(&mut isolate, task).0, Ok("[\"ab\"]".to_owned()));
}

#[test]
fn callers_of_a_generator_survive_collections_while_it_runs() {
    let mut isolate = Isolate::new();

    let kept = isolate
        .eval("(def (churn n) (if (< n 1) :done (do [n n] (str n) (churn (- n 1))))) (let [s (str \"x\" \"y\") g (gen (churn 150000) (yield 1))] (next g) s)")
        .unwrap();

    assert_eq!(isolate.write(kept), "\"xy\"");
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

#[test]
fn values_nested_too_deeply_to_send_are_refused_instead_of_crashing() {
    let mut isolate = Isolate::new();

    let error = isolate
        .eval("(let [deep (reduce (fn [acc _] [acc]) nil (range 5000)) c (chan 1)] (send c deep))")
        .unwrap_err();

    assert!(
        error
            .message
            .ends_with("is nested more than 256 levels deep to be sent through a channel"),
        "{}",
        error.message
    );
}

#[test]
fn synchronous_work_stops_at_its_budget() {
    let mut isolate = Isolate::new();
    isolate.eval("(def (spin) (spin))").unwrap();
    let spin = isolate.eval("spin").unwrap();

    let error = isolate
        .within(Duration::from_millis(50), |isolate| isolate.call(spin, &[]))
        .unwrap_err();

    assert_eq!(error.message, "This took longer than 50 ms and was stopped");
    let started = std::time::Instant::now();
    let slept = isolate
        .within(Duration::from_millis(50), |isolate| {
            isolate.eval("(sleep 600000)")
        })
        .unwrap_err();
    assert_eq!(slept.message, "This took longer than 50 ms and was stopped");
    assert!(started.elapsed() < Duration::from_secs(60));
    let sum = isolate.eval("(+ 1 2)").unwrap();
    assert_eq!(isolate.write(sum), "3");
}

#[test]
fn natives_know_which_task_is_calling_them() {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut isolate = Isolate::new();
    let record = std::rc::Rc::clone(&seen);
    isolate
        .native("ox", "whose")
        .arity(0, Some(0))
        .function(move |heap, _| {
            record.borrow_mut().push(heap.running_task());
            Ok(Value::Nil)
        });
    let both = isolate.eval("(fn [] (whose) (sleep 1) (whose))").unwrap();

    isolate.eval("(whose)").unwrap();
    let first = isolate.spawn(both, &[]).unwrap();
    let second = isolate.spawn(both, &[]).unwrap();
    while isolate.pending() > 0 {
        isolate.run(Duration::from_millis(5));
    }

    let mut seen = seen.borrow().clone();
    assert_eq!(seen.remove(0), None);
    seen.sort_by_key(|task| task.map(|task| task == second));
    assert_eq!(seen, [Some(first), Some(first), Some(second), Some(second)]);
}
