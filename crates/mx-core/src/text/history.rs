#[derive(Debug, Clone)]
pub enum Edit {
    Insert { at: usize, text: String },
    Delete { at: usize, text: String },
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: u64,
    pub edits: Vec<Edit>,
    pub point_before: usize,
    pub point_after: usize,
}

#[derive(Debug, Clone, Default)]
pub struct History {
    done: Vec<Step>,
    undone: Vec<Step>,
    open: bool,
    next_id: u64,
}

impl History {
    pub fn record(&mut self, edit: Edit, point_before: usize, point_after: usize) {
        self.undone.clear();
        match self.done.last_mut() {
            Some(step) if self.open => {
                step.edits.push(edit);
                step.point_after = point_after;
            }
            _ => {
                self.next_id += 1;
                self.done.push(Step {
                    id: self.next_id,
                    edits: vec![edit],
                    point_before,
                    point_after,
                });
                self.open = true;
            }
        }
    }

    pub fn current(&self) -> u64 {
        self.done.last().map_or(0, |step| step.id)
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn take_undo(&mut self) -> Option<Step> {
        self.open = false;
        self.done.pop()
    }

    pub fn take_redo(&mut self) -> Option<Step> {
        self.undone.pop()
    }

    pub fn push_undone(&mut self, step: Step) {
        self.undone.push(step);
    }

    pub fn push_done(&mut self, step: Step) {
        self.open = false;
        self.done.push(step);
    }
}
