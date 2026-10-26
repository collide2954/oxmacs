use super::MODULE;
use ox_lang::Isolate;
use ox_lang::Runtime;
use ox_lang::error::Error;
use ox_lang::routine::{Next, Routine, Started};
use ox_lang::symbol::Sym;
use ox_lang::value::Value;

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
    isolate
        .native(MODULE, "edit")
        .arity(2, Some(2))
        .routine(|_, _, arguments| {
            Ok(Started::Running(Box::new(Edit {
                change: arguments[1],
                held: [arguments[0]],
                step: Step::Begin,
            })))
        });
}
