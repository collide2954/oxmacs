use std::sync::{Arc, Mutex};

use ox_lang::Isolate;
use ox_lang::sink::{Sink, Started};

#[derive(Debug, Default)]
struct Heard {
    printed: Mutex<Vec<String>>,
}

impl Sink for Heard {
    fn process_started(&self, _: Started) -> Option<u64> {
        None
    }

    fn process_ended(&self, _: u64, _: Option<i32>) {}

    fn work_ready(&self) {}

    fn printed(&self, text: &str) {
        self.printed.lock().unwrap().push(text.to_owned());
    }
}

#[test]
fn printed_text_goes_to_the_sink_instead_of_stdout() {
    let heard = Arc::new(Heard::default());
    let mut isolate = Isolate::new();
    isolate.set_sink(heard.clone());

    isolate.eval(r#"(print "a" 1) (println "b")"#).unwrap();

    assert_eq!(*heard.printed.lock().unwrap(), ["a 1", "b\n"]);
}
