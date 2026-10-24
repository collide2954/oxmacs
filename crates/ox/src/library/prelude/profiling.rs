use crate::error::{Error, ErrorKind};
use crate::isolate::Isolate;
use crate::library::errors::catches;
use crate::native::Control;
use crate::value::Value;
use crate::vm::Runtime;
use crate::vm::build::raised;
use crate::vm::routine::{Next, Routine, Started};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Starting,
    Working,
    Reporting,
    Done,
    Stopping,
}

#[derive(Debug)]
struct Profile {
    work: Value,
    step: Step,
    mine: bool,
    failure: Value,
}

impl Routine for Profile {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        match self.step {
            Step::Starting => {
                self.step = Step::Working;
                Ok(Next::Call(
                    runtime.control_value(Control::ProfileStart),
                    &[],
                ))
            }
            Step::Working => {
                self.mine = value.truthy();
                self.step = Step::Reporting;
                Ok(Next::Catch(self.work, &[]))
            }
            Step::Reporting => {
                self.step = Step::Done;
                let report = if self.mine {
                    Control::ProfileStop
                } else {
                    Control::ProfileSnapshot
                };
                Ok(Next::Call(runtime.control_value(report), &[]))
            }
            Step::Done => Ok(Next::Return(value)),
            Step::Stopping => Err(raised(&runtime.heap, self.failure)),
        }
    }

    fn raised(&mut self, runtime: &mut Runtime, error: Value) -> Result<Next<'_>, Error> {
        let any = runtime
            .error_types
            .get(&ErrorKind::Other)
            .copied()
            .unwrap_or(Value::Nil);
        if !self.mine || !catches(&runtime.heap, error, any) {
            return Err(raised(&runtime.heap, error));
        }
        self.failure = error;
        self.step = Step::Stopping;
        Ok(Next::Call(runtime.control_value(Control::ProfileStop), &[]))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.work, self.failure]);
    }
}

pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_routine("profile", 1, Some(1), None, |_, _, arguments| {
        Ok(Started::Running(Box::new(Profile {
            work: arguments[0],
            step: Step::Starting,
            mine: false,
            failure: Value::Nil,
        })))
    });
}
