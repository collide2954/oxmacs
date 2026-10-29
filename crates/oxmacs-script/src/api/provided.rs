use std::collections::HashMap;

use ox_lang::heap::{Heap, Hold};
use ox_lang::native::field;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, Event, Offer, Picked, Service, Source};
use oxmacs_core::text::BufferId;

use crate::ide::{log, repl};

#[derive(Debug, Default)]
struct Provided {
    hooks: Vec<(Service, Source, Hold)>,
    offered: HashMap<BufferId, Hold>,
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

fn offer(isolate: &mut Isolate, editor: &mut Editor, buffer: BufferId) -> Result<(), Error> {
    let Some(hook) = hook_for(editor, buffer, Service::Items).and_then(|hold| isolate.held(hold))
    else {
        return Ok(());
    };
    let found = isolate.call_with(editor, hook, &[])?;
    let heap = isolate.heap_mut();
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
            service: Service::Items,
            ..
        } => offer(isolate, editor, buffer),
        Event::Asked {
            buffer,
            service,
            explicit,
        } => {
            if !repl::asked(editor, buffer, service, explicit) && editor.buffer().id() == buffer {
                editor.fall_back(service, explicit);
            }
            Ok(())
        }
        Event::ItemChosen { buffer, index } => choose(isolate, editor, buffer, index),
        _ => Ok(()),
    };
    if let Err(error) = result {
        log::report(editor, &error, None);
    }
}

#[cfg(test)]
mod tests {
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
}
