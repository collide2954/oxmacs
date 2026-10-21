use crate::compile::compile;
use crate::error::Error;
use crate::heap::Heap;
use crate::value::Value;
use crate::{print, reader, vm};

#[derive(Debug, Default)]
pub struct Isolate {
    heap: Heap,
}

impl Isolate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn eval(&mut self, source: &str) -> Result<Value, Error> {
        let (forms, errors) = reader::read(source);
        if let Some(error) = errors.into_iter().next() {
            return Err(Error::at(error.span, error.message));
        }
        let mut result = Value::Nil;
        for form in &forms {
            let code = compile(&mut self.heap, form)?;
            result = vm::run(&code)?;
        }
        Ok(result)
    }

    pub fn write(&self, value: Value) -> String {
        print::write(&self.heap, value)
    }
}

#[cfg(test)]
mod tests {
    use super::Isolate;

    fn evaluated(source: &str) -> String {
        let mut isolate = Isolate::new();
        match isolate.eval(source) {
            Ok(value) => isolate.write(value),
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn constants_evaluate_to_themselves() {
        for (source, printed) in [
            ("42", "42"),
            ("-3.5", "-3.5"),
            (r#""hi""#, r#""hi""#),
            ("true", "true"),
            ("nil", "nil"),
            (":key", ":key"),
            (r"\a", r"\a"),
        ] {
            assert_eq!(evaluated(source), printed, "{source}");
        }
    }

    #[test]
    fn the_last_form_gives_the_result() {
        assert_eq!(evaluated("1 2 3"), "3");
        assert_eq!(evaluated(""), "nil");
    }

    #[test]
    fn read_errors_stop_evaluation() {
        assert_eq!(evaluated("1 (2"), "error: This ( is never closed");
    }
}
