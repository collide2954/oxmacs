use std::collections::HashMap;

use ox_lang::heap::{Heap, Hold};
use ox_lang::native::field;
use ox_lang::task::TaskId;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, Event, Offer, Picked, Place, Service, Source};
use oxmacs_core::text::BufferId;

use crate::api::places;
use crate::ide::{log, repl};

pub(crate) const OPTIONS: [(&str, Service); 6] = [
    ("items", Service::Items),
    ("complete", Service::Complete),
    ("signature", Service::Signature),
    ("definition", Service::Definition),
    ("references", Service::References),
    ("docs", Service::Docs),
];

pub(crate) fn hook(option: &str, value: Value) -> Option<(Service, Value)> {
    OPTIONS
        .iter()
        .find(|(name, _)| *name == option && value != Value::Nil)
        .map(|(_, service)| (*service, value))
}

#[derive(Debug, Clone, Copy)]
struct Asking {
    task: TaskId,
    buffer: BufferId,
    service: Service,
    explicit: bool,
}

#[derive(Debug, Default)]
struct Provided {
    hooks: Vec<(Service, Source, Hold)>,
    offered: HashMap<BufferId, Hold>,
    asking: Vec<Asking>,
}

pub(crate) fn provide(
    heap: &mut Heap,
    editor: &mut Editor,
    service: Service,
    source: Source,
    hook: Value,
) {
    let hold = heap.hold(hook);
    let mut provided = editor.take_plugin_state::<Provided>().unwrap_or_default();
    for (_, _, old) in provided
        .hooks
        .extract_if(.., |(known, from, _)| *known == service && *from == source)
    {
        heap.release(old);
    }
    provided.hooks.push((service, source.clone(), hold));
    editor.set_plugin_state(provided);
    editor.provide(service, source);
}

pub(crate) fn release(heap: &mut Heap, editor: &mut Editor, buffer: BufferId) {
    editor.withdraw(&Source::Buffer(buffer));
    let Some(provided) = editor.plugin_state_mut::<Provided>() else {
        return;
    };
    let mut gone: Vec<Hold> = provided.offered.remove(&buffer).into_iter().collect();
    gone.extend(
        provided
            .hooks
            .extract_if(.., |(_, from, _)| *from == Source::Buffer(buffer))
            .map(|(_, _, hold)| hold),
    );
    for hold in gone {
        heap.release(hold);
    }
}

fn hook_for(editor: &Editor, buffer: BufferId, service: Service) -> Option<Hold> {
    let provided = editor.plugin_state::<Provided>()?;
    let mode = editor
        .buffer_language(buffer)
        .map(|language| Source::Mode(language.name().to_owned()));
    [Some(Source::Buffer(buffer)), mode]
        .into_iter()
        .flatten()
        .find_map(|wanted| {
            provided
                .hooks
                .iter()
                .find(|(known, from, _)| *known == service && *from == wanted)
                .map(|(_, _, hold)| *hold)
        })
}

fn text_field(heap: &Heap, item: Value, key: &str) -> Option<String> {
    field(heap, item, key).map(|value| print::display(heap, value))
}

fn offer(
    heap: &mut Heap,
    editor: &mut Editor,
    buffer: BufferId,
    found: Value,
) -> Result<(), Error> {
    let entries = heap.elements(found).ok_or_else(|| {
        Error::new(format!(
            ":items gives a vector of maps with a :label, got {}",
            print::brief(heap, found)
        ))
    })?;
    if entries.is_empty() {
        editor.show_message("No items in this buffer");
        return Ok(());
    }
    let offers: Vec<Offer> = entries
        .iter()
        .map(|item| Offer {
            label: text_field(heap, *item, "label").unwrap_or_default(),
            note: text_field(heap, *item, "note"),
        })
        .collect();
    let hold = heap.hold(found);
    let mut provided = editor.take_plugin_state::<Provided>().unwrap_or_default();
    if let Some(old) = provided.offered.insert(buffer, hold) {
        heap.release(old);
    }
    editor.set_plugin_state(provided);
    editor.prompt_pick(
        "Index item: ",
        move |_| offers.clone(),
        move |editor, picked| {
            if let Picked::Offered(index) = picked {
                editor.choose_item(buffer, index);
            }
        },
    );
    Ok(())
}

fn choose(
    isolate: &mut Isolate,
    editor: &mut Editor,
    buffer: BufferId,
    index: usize,
) -> Result<(), Error> {
    let Some(found) = editor
        .plugin_state::<Provided>()
        .and_then(|provided| provided.offered.get(&buffer).copied())
        .and_then(|hold| isolate.held(hold))
    else {
        return Ok(());
    };
    let heap = isolate.heap();
    let Some(item) = heap
        .elements(found)
        .and_then(|items| items.get(index).copied())
    else {
        return Ok(());
    };
    if let Some(go) = field(heap, item, "go") {
        isolate.spawn(go, &[])?;
        return Ok(());
    }
    if let Some(Value::Int(line)) = field(heap, item, "line")
        && editor.buffer().id() == buffer
    {
        let shown = editor.buffer_mut();
        let line = usize::try_from(line)
            .unwrap_or(0)
            .min(shown.line_of(shown.len_chars()));
        let point = shown.point();
        shown.set_mark(point);
        shown.deactivate_mark();
        shown.set_point(shown.line_start(line));
    }
    Ok(())
}

pub(crate) fn hear(isolate: &mut Isolate, editor: &mut Editor, event: &Event) {
    let result = match *event {
        Event::Asked {
            buffer,
            service,
            explicit,
        } => assist(isolate, editor, buffer, service, explicit),
        Event::ItemChosen { buffer, index } => choose(isolate, editor, buffer, index),
        _ => Ok(()),
    };
    if let Err(error) = result {
        log::report(editor, &error, None);
    }
}

fn assist(
    isolate: &mut Isolate,
    editor: &mut Editor,
    buffer: BufferId,
    service: Service,
    explicit: bool,
) -> Result<(), Error> {
    let hook = hook_for(editor, buffer, service).and_then(|hold| isolate.held(hold));
    let Some(hook) = hook else {
        if !repl::asked(editor, buffer, service, explicit) && editor.buffer().id() == buffer {
            editor.fall_back(service, explicit);
        }
        return Ok(());
    };
    if editor.buffer().id() != buffer {
        return Ok(());
    }
    let arguments = match service {
        Service::Complete => vec![Value::Bool(explicit)],
        _ => Vec::new(),
    };
    let task = isolate.spawn(hook, &arguments)?;
    let mut provided = editor.take_plugin_state::<Provided>().unwrap_or_default();
    provided.asking.push(Asking {
        task,
        buffer,
        service,
        explicit,
    });
    editor.set_plugin_state(provided);
    Ok(())
}

pub(crate) fn finished(
    isolate: &mut Isolate,
    editor: &mut Editor,
    finished: Vec<(TaskId, Result<Value, Error>)>,
) -> Vec<(TaskId, Result<Value, Error>)> {
    let mut others = Vec::new();
    for (task, result) in finished {
        let asking = editor.plugin_state_mut::<Provided>().and_then(|provided| {
            let at = provided
                .asking
                .iter()
                .position(|known| known.task == task)?;
            Some(provided.asking.remove(at))
        });
        let Some(asking) = asking else {
            others.push((task, result));
            continue;
        };
        let answered = result.and_then(|answer| {
            if editor.buffer().id() != asking.buffer {
                return Ok(());
            }
            answered(isolate.heap_mut(), editor, &asking, answer)
        });
        if let Err(error) = answered {
            log::report(editor, &error, None);
        }
    }
    others
}

fn places(heap: &Heap, answer: Value) -> Result<Vec<Place>, Error> {
    let found = match heap.elements(answer) {
        Some(found) => found,
        None => vec![answer],
    };
    found
        .into_iter()
        .map(|place| places::place(heap, place))
        .collect()
}

fn answered(
    heap: &mut Heap,
    editor: &mut Editor,
    asking: &Asking,
    answer: Value,
) -> Result<(), Error> {
    let Asking {
        buffer,
        service,
        explicit,
        ..
    } = *asking;
    match (service, answer) {
        (_, Value::Nil | Value::Bool(false)) => editor.fall_back(service, explicit),
        (_, Value::Bool(true)) => {}
        (Service::Items, found) => offer(heap, editor, buffer, found)?,
        (Service::Complete, found) => {
            let candidates = heap
                .elements(found)
                .ok_or_else(|| {
                    Error::new(format!(
                        ":complete gives a vector of names, true or nil, got {}",
                        print::brief(heap, found)
                    ))
                })?
                .into_iter()
                .map(|candidate| print::display(heap, candidate))
                .collect();
            let start = editor.name_start();
            editor.complete_at(start, candidates, explicit);
        }
        (Service::Signature, text) => editor.answer_signature(Some(print::display(heap, text))),
        (Service::Docs, text) => editor.show_doc(&print::display(heap, text)),
        (Service::Definition, found) => editor.answer_definition(places(heap, found)?),
        (Service::References, found) => editor.answer_references(places(heap, found)?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::Editor;
    use oxmacs_core::testing::labels;

    use crate::testing::{press, settle, started, typed};

    #[test]
    fn a_mode_s_items_are_offered_by_m_g_i_and_jump_to_their_line() {
        let (home, mut editor, mut ox) = started(
            r#"(defmode notes "Notes" :extensions ["note"]
                 :items (fn [] [{label: "Second" note: "L2" line: 1} {label: "Third" line: 2}]))"#,
        );
        let path = home.path().join("a.note");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        editor.open_file(&path);
        settle(&mut editor, &mut ox);

        press(&mut editor, &mut ox, "M-g i");
        settle(&mut editor, &mut ox);
        assert_eq!(labels(&editor), ["Second", "Third"]);
        typed(&mut editor, &mut ox, "Third");
        press(&mut editor, &mut ox, "RET");
        settle(&mut editor, &mut ox);

        assert_eq!(editor.buffer().point(), 8);
        assert_eq!(editor.buffer().mark(), Some(0));
    }

    #[test]
    fn a_view_s_items_call_go_when_chosen() {
        let (_home, mut editor, mut ox) = started("");
        ox.eval(
            &mut editor,
            r#"(module (import (ox ui)))
               (def picked (atom nil))
               (open-view "*chapters*" (fn [] (ui/text "x"))
                          :items (fn [] [{label: "Intro" go: (fn [] (reset! picked :intro))}]))"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        press(&mut editor, &mut ox, "M-g i");
        settle(&mut editor, &mut ox);
        press(&mut editor, &mut ox, "RET");
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "@picked").unwrap(), ":intro");
    }

    const NOTES: &str = r#"(defmode notes "Notes" :extensions ["note"]
  :complete (fn [explicit] (if explicit ["alpha" "alps"] ["alpha"]))
  :signature (fn [] (when (> (point) 0) "(greet name)"))
  :docs (fn [] (if (> (point) 0) "Greets someone." nil))
  :definition (fn [] {file: $"{(oxmacs-home)}/b.note" line: 2})
  :references (fn [] [{file: $"{(oxmacs-home)}/b.note" line: 1} {file: $"{(oxmacs-home)}/b.note" line: 3}]))"#;

    fn noting(text: &str) -> (tempfile::TempDir, Editor, crate::Ox) {
        let (home, mut editor, mut ox) = started(NOTES);
        std::fs::write(home.path().join("b.note"), "one\ntwo\nthree\n").unwrap();
        let path = home.path().join("a.note");
        std::fs::write(&path, text).unwrap();
        editor.open_file(&path);
        let end = editor.buffer().len_chars();
        editor.buffer_mut().set_point(end);
        settle(&mut editor, &mut ox);
        (home, editor, ox)
    }

    fn rest(editor: &mut Editor, ox: &mut crate::Ox) {
        let since = Instant::now();
        editor.idle(since, since + Duration::from_secs(2));
        settle(editor, ox);
    }

    fn offered(editor: &Editor) -> Option<Vec<String>> {
        editor.pane().popup().map(|view| view.items)
    }

    #[test]
    fn a_mode_s_completions_answer_c_m_i_and_typing() {
        let (_home, mut editor, mut ox) = noting("al");

        press(&mut editor, &mut ox, "C-M-i");
        assert_eq!(
            offered(&editor),
            Some(vec!["alpha".to_owned(), "alps".to_owned()])
        );
        press(&mut editor, &mut ox, "C-g");

        typed(&mut editor, &mut ox, " alp");
        rest(&mut editor, &mut ox);
        assert_eq!(offered(&editor), Some(vec!["alpha".to_owned()]));
        press(&mut editor, &mut ox, "TAB");
        assert_eq!(editor.buffer().contents(), "al alpha");
    }

    #[test]
    fn a_mode_s_signature_and_docs_show_or_leave_it_to_the_language_server() {
        let (_home, mut editor, mut ox) = noting("hello");

        rest(&mut editor, &mut ox);
        assert_eq!(editor.message(), Some("(greet name)"));
        press(&mut editor, &mut ox, "C-c C-d");
        let doc = editor.buffer_named("*doc*").unwrap();
        assert!(
            editor
                .buffer_by_id(doc)
                .unwrap()
                .contents()
                .contains("Greets someone.")
        );

        press(&mut editor, &mut ox, "M-<");
        rest(&mut editor, &mut ox);
        assert_eq!(editor.message(), None);
        press(&mut editor, &mut ox, "C-c C-d");
        assert_eq!(
            editor.message(),
            Some("No language server works on this buffer")
        );
    }

    #[test]
    fn a_hook_may_wait_before_it_answers() {
        let (home, mut editor, mut ox) = started(
            r#"(defmode notes "Notes" :extensions ["note"] :docs (fn [] (sleep 20) "Late."))"#,
        );
        let path = home.path().join("a.note");
        std::fs::write(&path, "x").unwrap();
        editor.open_file(&path);

        press(&mut editor, &mut ox, "C-c C-d");
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor.buffer_named("*doc*").is_none() && Instant::now() < deadline {
            settle(&mut editor, &mut ox);
        }
        let doc = editor.buffer_named("*doc*").unwrap();
        assert!(
            editor
                .buffer_by_id(doc)
                .unwrap()
                .contents()
                .contains("Late.")
        );
    }

    #[test]
    fn a_mode_s_definition_is_gone_to_and_its_references_listed() {
        let (_home, mut editor, mut ox) = noting("greet");

        press(&mut editor, &mut ox, "M-.");
        assert_eq!(editor.buffer().name(), "b.note");
        assert_eq!(editor.buffer().point(), 4);
        press(&mut editor, &mut ox, "M-,");
        assert_eq!(editor.buffer().name(), "a.note");

        press(&mut editor, &mut ox, "M-?");
        let xref = editor.buffer_named("*xref*").unwrap();
        let listed = editor.buffer_by_id(xref).unwrap().contents();
        assert!(listed.contains("2 references to greet"), "{listed}");
    }
}
