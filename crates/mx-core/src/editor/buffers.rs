use crate::text::Buffer;

#[derive(Debug)]
pub(super) struct BufferList {
    buffers: Vec<Buffer>,
}

impl BufferList {
    pub(super) fn new(buffer: Buffer) -> Self {
        Self {
            buffers: vec![buffer],
        }
    }

    pub(super) fn current(&self) -> &Buffer {
        &self.buffers[0]
    }

    pub(super) fn current_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[0]
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Buffer> {
        self.buffers.iter()
    }

    pub(super) fn position(&self, predicate: impl Fn(&Buffer) -> bool) -> Option<usize> {
        self.buffers.iter().position(predicate)
    }

    pub(super) fn push_front(&mut self, buffer: Buffer) {
        self.buffers.insert(0, buffer);
    }

    pub(super) fn select(&mut self, index: usize) {
        if index < self.buffers.len() {
            let buffer = self.buffers.remove(index);
            self.buffers.insert(0, buffer);
        }
    }
}
