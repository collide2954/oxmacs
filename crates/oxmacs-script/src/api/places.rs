use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use ox_lang::heap::{Heap, Hold};
use ox_lang::native::{count, expected, field, given, text};
use ox_lang::value::Value;
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, Place, Places};
use oxmacs_core::theme::Highlight;

use crate::ide::log;
use oxmacs_core::theme::face_named;

const NAME: &str = "show-places";
const OPTIONS: [&str; 7] = [
    "title",
    "header",
    "ordered",
    "root",
    "empty",
    "on-visit",
    "on-refresh",
];

#[derive(Debug, Default)]
struct Held {
    visit: Option<Hold>,
    refresh: Option<Hold>,
    places: Vec<Hold>,
}

impl Held {
    fn holds(&self) -> impl Iterator<Item = Hold> + '_ {
        self.visit
            .iter()
            .chain(&self.refresh)
            .chain(&self.places)
            .copied()
    }
}

#[derive(Debug)]
enum Call {
    Visit(String, usize),
    Refresh(String),
}

#[derive(Debug, Default)]
struct Lists {
    held: HashMap<String, Held>,
    waiting: Vec<Call>,
}

fn place(heap: &Heap, value: Value) -> Result<Place, Error> {
    if heap.map_of(value).is_none() {
        return Err(expected(
            heap,
            NAME,
            "places as maps such as {file: \"a.ox\" line: 2}",
            value,
        ));
    }
    let file = field(heap, value, "file")
        .ok_or_else(|| expected(heap, NAME, "a :file in every place", value))?;
    let file = text(heap, NAME, file)?;
    let number = |key: &str| -> Result<usize, Error> {
        field(heap, value, key).map_or(Ok(1), |found| count(heap, NAME, found))
    };
    let words = |key: &str| -> Result<Option<String>, Error> {
        field(heap, value, key)
            .map(|found| text(heap, NAME, found).map(str::to_owned))
            .transpose()
    };
    let face = match field(heap, value, "face") {
        None => Highlight::Function,
        Some(Value::Keyword(name)) => face_named(name.name()).ok_or_else(|| {
            expected(heap, NAME, "a :face such as :warning", Value::Keyword(name))
        })?,
        Some(other) => return Err(expected(heap, NAME, "a :face such as :warning", other)),
    };
    Ok(Place {
        label: words("label")?.map(|label| (label, face)),
        text: words("text")?,
        source: words("source")?.map(Arc::from),
        ..Place::at(file, number("line")?, number("column")?)
    })
}

fn queue(editor: &mut Editor, call: Call) {
    if let Some(lists) = editor.plugin_state_mut::<Lists>() {
        lists.waiting.push(call);
    }
}

fn show(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = text(heap, NAME, arguments[0])?.to_owned();
    let items = heap
        .elements(arguments[1])
        .ok_or_else(|| expected(heap, NAME, "a vector of places", arguments[1]))?;
    let places = items
        .iter()
        .map(|item| place(heap, *item))
        .collect::<Result<Vec<_>, _>>()?;
    let mut list = Places {
        title: name.clone(),
        places,
        ..Places::default()
    };
    let mut visit = None;
    let mut refresh = None;
    for (option, value) in given(&OPTIONS, &arguments[2..]) {
        match option {
            "title" => text(heap, NAME, value)?.clone_into(&mut list.title),
            "header" => {
                list.header = text(heap, NAME, value)?
                    .lines()
                    .map(str::to_owned)
                    .collect();
            }
            "ordered" => list.ordered = value.truthy(),
            "root" => list.root = Some(PathBuf::from(text(heap, NAME, value)?)),
            "empty" => list.empty = Some(text(heap, NAME, value)?.to_owned()),
            "on-visit" => visit = Some(value),
            "on-refresh" => refresh = Some(value),
            _ => {}
        }
    }
    let mut held = Held::default();
    if let Some(visit) = visit {
        held.visit = Some(heap.hold(visit));
        held.places = items.iter().map(|item| heap.hold(*item)).collect();
        let named = name.clone();
        list.on_visit = Some(Arc::new(move |editor, index| {
            queue(editor, Call::Visit(named.clone(), index));
        }));
    }
    if let Some(refresh) = refresh {
        held.refresh = Some(heap.hold(refresh));
        let named = name.clone();
        list.on_refresh = Some(Arc::new(move |editor| {
            queue(editor, Call::Refresh(named.clone()));
        }));
    }
    if editor.plugin_state::<Lists>().is_none() {
        editor.set_plugin_state(Lists::default());
    }
    let old = editor
        .plugin_state_mut::<Lists>()
        .and_then(|lists| lists.held.insert(name.clone(), held));
    for hold in old.iter().flat_map(Held::holds) {
        heap.release(hold);
    }
    editor.show_places(&name, list);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "show-places")
        .arity(2, Some(2))
        .keywords(&OPTIONS)
        .host::<Editor>(show);
}

pub fn run_waiting(isolate: &mut Isolate, editor: &mut Editor) {
    let waiting = editor
        .plugin_state_mut::<Lists>()
        .map(|lists| std::mem::take(&mut lists.waiting))
        .unwrap_or_default();
    for call in waiting {
        let found = editor
            .plugin_state::<Lists>()
            .and_then(|lists| match &call {
                Call::Visit(name, index) => {
                    let held = lists.held.get(name)?;
                    Some((held.visit?, held.places.get(*index).copied()))
                }
                Call::Refresh(name) => Some((lists.held.get(name)?.refresh?, None)),
            });
        let Some((function, argument)) = found else {
            continue;
        };
        let Some(function) = isolate.held(function) else {
            continue;
        };
        let arguments: Vec<Value> = argument
            .and_then(|hold| isolate.held(hold))
            .into_iter()
            .collect();
        if let Err(error) = isolate.spawn(function, &arguments) {
            log::report(editor, &error, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::{lines, press, settle};

    fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        match ox.eval(editor, source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn ox_shows_places_and_hears_visits_and_refreshes() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        eval(
            &mut editor,
            &mut ox,
            r#"(def heard (atom []))
               (show-places "*hits*"
                 [{file: "b.ox" line: 2 column: 3 label: "call" text: "(area 1)"}
                  {file: "a.ox" line: 7 label: "def" face: :keyword text: "(def area)"}]
                 :title "2 hits"
                 :on-visit (fn [place] (swap! heard conj [:visit (get place :file)]))
                 :on-refresh (fn [] (swap! heard conj :refresh)))"#,
        );

        let shown = lines(&editor);
        assert_eq!(editor.buffer().name(), "*hits*");
        assert!(shown[0].starts_with("2 hits"));
        assert!(shown[3].ends_with("a.ox"), "{shown:?}");
        assert_eq!(shown[4], "     7:1    def   (def area)");
        assert_eq!(shown[7], "     2:3    call  (area 1)");

        press(&mut editor, &mut ox, "RET g");
        settle(&mut editor, &mut ox);
        assert_eq!(
            eval(&mut editor, &mut ox, "@heard"),
            r#"[[:visit "a.ox"] :refresh]"#
        );
    }

    #[test]
    fn places_must_be_maps_with_a_file() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        assert_eq!(
            eval(&mut editor, &mut ox, r#"(show-places "*x*" [{line: 2}])"#),
            "error: show-places expects a :file in every place, got {line: 2}"
        );
        assert_eq!(
            eval(
                &mut editor,
                &mut ox,
                r#"(show-places "*x*" [{file: "a" face: :plaid}])"#
            ),
            "error: show-places expects a :face such as :warning, got :plaid"
        );
    }
}
