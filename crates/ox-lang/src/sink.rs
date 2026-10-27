use std::any::Any;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Started {
    pub program: String,
    pub args: Vec<String>,
    pub directory: Option<PathBuf>,
    pub pid: u32,
    pub handle: Arc<dyn Any + Send + Sync>,
}

impl fmt::Debug for Started {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Started")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("directory", &self.directory)
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

pub trait Sink: Send + Sync + fmt::Debug {
    fn process_started(&self, process: Started) -> Option<u64>;

    fn process_ended(&self, id: u64, code: Option<i32>);

    fn work_ready(&self);

    fn printed(&self, text: &str);
}
