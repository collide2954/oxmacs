use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, ProcessInfo, ProcessStatus};

fn made(made: Result<Value, ox_lang::heap::HeapFull>) -> Result<Value, Error> {
    made.map_err(|full| Error::new(full.to_string()))
}

fn millis(time: jiff::Timestamp) -> Value {
    Value::Int(time.as_millisecond())
}

fn info(heap: &mut Heap, editor: &Editor, process: &ProcessInfo) -> Result<Value, Error> {
    let (status, exit) = match process.status {
        ProcessStatus::Starting => ("starting", Value::Nil),
        ProcessStatus::Running => ("running", Value::Nil),
        ProcessStatus::Exited(code) => (
            "exited",
            code.map_or(Value::Nil, |code| Value::Int(i64::from(code))),
        ),
        ProcessStatus::Failed => ("failed", Value::Nil),
    };
    let text = |heap: &mut Heap, text: &str| made(heap.string(text));
    let kind = text(heap, &process.kind)?;
    let name = text(heap, &process.name)?;
    let program = text(heap, &process.program)?;
    let args = process
        .args
        .iter()
        .map(|arg| text(heap, arg))
        .collect::<Result<Vec<Value>, Error>>()?;
    let args = made(heap.vector_from(&args))?;
    let directory = match &process.directory {
        Some(directory) => text(heap, &directory.display().to_string())?,
        None => Value::Nil,
    };
    let mut env = Vec::new();
    for (key, value) in &process.env {
        env.push(text(heap, key)?);
        env.push(text(heap, value)?);
    }
    let env = heap.map_from(&env)?;
    let cpu: Vec<Value> = process
        .cpu
        .iter()
        .map(|cpu| Value::Float(f64::from(*cpu)))
        .collect();
    let cpu = made(heap.vector_from(&cpu))?;
    let memory: Vec<Value> = process
        .memory
        .iter()
        .map(|bytes| Value::Int(i64::try_from(*bytes).unwrap_or(i64::MAX)))
        .collect();
    let memory = made(heap.vector_from(&memory))?;
    let buffer = match process.buffer.and_then(|id| editor.buffer_by_id(id)) {
        Some(buffer) => text(heap, buffer.name())?,
        None => Value::Nil,
    };
    heap.map_from(&[
        Value::keyword("id"),
        Value::Int(i64::try_from(process.id).unwrap_or(i64::MAX)),
        Value::keyword("kind"),
        kind,
        Value::keyword("name"),
        name,
        Value::keyword("program"),
        program,
        Value::keyword("args"),
        args,
        Value::keyword("directory"),
        directory,
        Value::keyword("env"),
        env,
        Value::keyword("status"),
        Value::keyword(status),
        Value::keyword("exit"),
        exit,
        Value::keyword("pid"),
        process
            .pid
            .map_or(Value::Nil, |pid| Value::Int(i64::from(pid))),
        Value::keyword("started"),
        millis(process.started),
        Value::keyword("ended"),
        process.ended.map_or(Value::Nil, millis),
        Value::keyword("cpu"),
        cpu,
        Value::keyword("memory"),
        memory,
        Value::keyword("buffer"),
        buffer,
    ])
}

fn id_of(heap: &Heap, name: &str, value: Value) -> Result<u64, Error> {
    match value {
        Value::Int(id) => u64::try_from(id).ok(),
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(format!(
            "{name} expects a process id, got {}",
            print::brief(heap, value)
        ))
    })
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("processes", 0, Some(0), |heap, editor, _| {
        let found = editor
            .processes()
            .iter()
            .map(|process| info(heap, editor, process))
            .collect::<Result<Vec<Value>, Error>>()?;
        made(heap.vector_from(&found))
    });
    for (name, force) in [("stop-process", false), ("kill-process", true)] {
        isolate.define_host_native::<Editor>(name, 1, Some(1), move |heap, editor, arguments| {
            let id = id_of(heap, name, arguments[0])?;
            editor.stop_process(id, force).map_err(Error::new)?;
            Ok(Value::Nil)
        });
    }
    isolate.define_host_native::<Editor>("visit-process", 1, Some(1), |heap, editor, arguments| {
        let id = id_of(heap, "visit-process", arguments[0])?;
        editor.visit_process(id).map_err(Error::new)?;
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>(
        "watch-process-usage",
        1,
        Some(1),
        |_, editor, arguments| {
            editor.watch_process_usage(arguments[0].truthy());
            Ok(Value::Nil)
        },
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use oxmacs_core::editor::{Editor, Effect, JobSpec};

    use crate::Ox;
    use crate::testing::settle;

    fn build() -> JobSpec {
        JobSpec {
            program: "cargo".to_owned(),
            args: vec!["build".to_owned()],
            directory: PathBuf::from("work"),
            env: Vec::new(),
            label: "compile",
            name: "cargo build".to_owned(),
            buffer: None,
            interactive: false,
        }
    }

    #[test]
    fn plugins_read_stop_and_follow_processes() {
        let home = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[]);
        ox.start(&mut editor, Some(home.path()));
        let job = editor.start_job(build(), |_, _, _| {});
        editor.job_started(job, 77);
        ox.eval(
            &mut editor,
            "(def seen (atom 0)) (on-processes-change :count (fn [] (swap! seen inc)))",
        )
        .unwrap();

        editor.processes_tick(&[(77, 3.5, 2048)]);
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "(> @seen 0)").unwrap(), "true");
        assert_eq!(
            ox.eval(
                &mut editor,
                "(let [p (first (processes))] [p.kind p.name p.program p.args p.status p.exit p.pid p.cpu p.memory p.buffer (int? p.started) p.ended])"
            )
            .unwrap(),
            r#"["compile" "cargo build" "cargo" ["build"] :running nil 77 [3.5] [2048] nil true nil]"#
        );
        editor.take_effects();
        ox.eval(
            &mut editor,
            "(let [id (get (first (processes)) :id)] (stop-process id) (kill-process id))",
        )
        .unwrap();
        assert_eq!(
            editor.take_effects(),
            [Effect::Terminate(job), Effect::Kill(job)]
        );
        ox.eval(&mut editor, "(watch-process-usage true)").unwrap();
        assert!(editor.wants_process_usage());
        assert!(
            ox.eval(&mut editor, "(stop-process 99)")
                .unwrap_err()
                .message
                .contains("No process 99")
        );
    }
}
