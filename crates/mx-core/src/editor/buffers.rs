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
}
