use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use iced::Rectangle;
use iced::wgpu;
use iced::widget::shader::{Pipeline, Primitive, Viewport};
use oxmacs_core::view::takeover::Backdrop;

use super::compiled::Compiled;

static WAITING: AtomicBool = AtomicBool::new(false);

pub fn wait_for_shaders() {
    WAITING.store(true, Ordering::Release);
}

type Pending = Arc<Mutex<Option<Option<Compiled>>>>;

#[derive(Debug)]
enum Slot {
    Compiling(Pending),
    Ready(Option<Compiled>),
}

impl Slot {
    fn start(device: &wgpu::Device, format: wgpu::TextureFormat, source: &Arc<str>) -> Self {
        if WAITING.load(Ordering::Acquire) {
            return Self::Ready(Compiled::new(device, format, source));
        }
        let pending: Pending = Arc::new(Mutex::new(None));
        let (filled, device, source) = (Arc::clone(&pending), device.clone(), Arc::clone(source));
        thread::spawn(move || {
            let compiled = Compiled::new(&device, format, &source);
            if let Ok(mut slot) = filled.lock() {
                *slot = Some(compiled);
            }
        });
        Self::Compiling(pending)
    }

    fn settle(&mut self) {
        if let Self::Compiling(pending) = self
            && let Some(done) = pending.lock().ok().and_then(|mut slot| slot.take())
        {
            *self = Self::Ready(done);
        }
    }

    fn compiled(&self) -> Option<&Compiled> {
        match self {
            Self::Ready(Some(compiled)) => Some(compiled),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Shaders {
    format: wgpu::TextureFormat,
    compiled: HashMap<u64, Slot>,
}

impl Pipeline for Shaders {
    fn new(_device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            compiled: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct Frame(pub Backdrop);

impl Primitive for Frame {
    type Pipeline = Shaders;

    fn prepare(
        &self,
        shaders: &mut Shaders,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let format = shaders.format;
        let shader = &self.0.shader;
        let slot = shaders
            .compiled
            .entry(shader.key)
            .or_insert_with(|| Slot::start(device, format, &shader.source));
        slot.settle();
        if let Some(compiled) = slot.compiled() {
            let scale = viewport.scale_factor();
            let uniforms = self.0.uniforms(
                (bounds.x * scale, bounds.y * scale),
                (bounds.width, bounds.height),
                scale,
            );
            queue.write_buffer(&compiled.uniforms, 0, &uniforms);
        }
    }

    fn draw(&self, shaders: &Shaders, pass: &mut wgpu::RenderPass<'_>) -> bool {
        if let Some(compiled) = shaders
            .compiled
            .get(&self.0.shader.key)
            .and_then(Slot::compiled)
        {
            compiled.draw(pass);
        }
        true
    }
}
