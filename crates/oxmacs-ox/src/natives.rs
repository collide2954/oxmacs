use ox::Isolate;
use ox::value::{Value, print};
use oxmacs_core::editor::Editor;

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("message", 1, Some(1), |heap, editor, arguments| {
        editor.show_message(print::display(heap, arguments[0]));
        Ok(Value::Nil)
    });
}
