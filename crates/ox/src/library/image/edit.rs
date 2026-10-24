use super::MODULE;
use crate::error::Error;
use crate::isolate::Isolate;
use crate::symbol::Sym;
use crate::value::Value;
use crate::vm::Runtime;
use crate::vm::routine::{Next, Routine, Started};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Begin,
    Change,
    End,
    Done,
}

#[derive(Debug)]
struct Edit {
    change: Value,
    held: [Value; 1],
    step: Step,
}

fn image_function(runtime: &Runtime, name: &str) -> Value {
    runtime
        .module_ids
        .get(MODULE)
        .and_then(|module| runtime.lookup(*module, Sym::intern(name)))
        .unwrap_or(Value::Nil)
}

impl Routine for Edit {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        match self.step {
            Step::Begin => {
                self.step = Step::Change;
                Ok(Next::Call(
                    image_function(runtime, "begin-edit"),
                    &self.held,
                ))
            }
            Step::Change => {
                self.held = [value];
                self.step = Step::End;
                Ok(Next::Call(self.change, &self.held))
            }
            Step::End => {
                self.step = Step::Done;
                Ok(Next::Call(image_function(runtime, "end-edit"), &self.held))
            }
            Step::Done => Ok(Next::Return(value)),
        }
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.push(self.change);
        values.extend(self.held);
    }
}

pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_routine_in(MODULE, "edit", 2, Some(2), None, |_, _, arguments| {
        Ok(Started::Running(Box::new(Edit {
            change: arguments[1],
            held: [arguments[0]],
            step: Step::Begin,
        })))
    });
}
