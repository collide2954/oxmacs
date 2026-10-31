use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use iced::advanced::image::Handle;
use iced::futures::executor::block_on;
use iced::wgpu;
use oxmacs_core::view::takeover::Backdrop;

use super::compiled::Compiled;

const MOST_PIXELS: f32 = 640.0 * 360.0;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug)]
struct Job {
    backdrop: Backdrop,
    size: (f32, f32),
}

#[derive(Debug, Clone)]
enum Shown {
    Nothing,
    Frame(u64, Handle),
    Unavailable,
}

#[derive(Debug)]
struct Worker {
    jobs: Mutex<Sender<Job>>,
    busy: Arc<AtomicBool>,
    shown: Arc<Mutex<Shown>>,
}

static WORKER: OnceLock<Worker> = OnceLock::new();

fn worker() -> &'static Worker {
    WORKER.get_or_init(|| {
        let (jobs, received) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        let shown = Arc::new(Mutex::new(Shown::Nothing));
        let (working, showing) = (Arc::clone(&busy), Arc::clone(&shown));
        thread::spawn(move || run(&received, &working, &showing));
        Worker {
            jobs: Mutex::new(jobs),
            busy,
            shown,
        }
    })
}

fn current(worker: &Worker) -> Shown {
    worker
        .shown
        .lock()
        .map_or(Shown::Nothing, |shown| shown.clone())
}

pub fn frame(backdrop: &Backdrop, size: (f32, f32)) -> Option<Handle> {
    let worker = worker();
    if !worker.busy.swap(true, Ordering::AcqRel)
        && let Ok(jobs) = worker.jobs.lock()
    {
        let _ = jobs.send(Job {
            backdrop: backdrop.clone(),
            size,
        });
    }
    match current(worker) {
        Shown::Frame(key, handle) if key == backdrop.shader.key => Some(handle),
        _ => None,
    }
}

pub fn newly_unavailable() -> bool {
    static TOLD: AtomicBool = AtomicBool::new(false);
    WORKER
        .get()
        .is_some_and(|worker| matches!(current(worker), Shown::Unavailable))
        && !TOLD.swap(true, Ordering::AcqRel)
}

fn show(shown: &Mutex<Shown>, now: Shown) {
    if let Ok(mut shown) = shown.lock() {
        *shown = now;
    }
}

fn run(jobs: &Receiver<Job>, busy: &AtomicBool, shown: &Mutex<Shown>) {
    let Some(device) = Device::new() else {
        show(shown, Shown::Unavailable);
        return;
    };
    let mut compiled = HashMap::new();
    for job in jobs {
        if let Some(handle) = device.render(&mut compiled, &job) {
            show(shown, Shown::Frame(job.backdrop.shader.key, handle));
        }
        busy.store(false, Ordering::Release);
    }
}

#[derive(Debug)]
struct Device {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Device {
    fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all()),
            flags: wgpu::InstanceFlags::empty(),
            ..wgpu::InstanceDescriptor::default()
        });
        let adapter = [false, true].into_iter().find_map(|fallback| {
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: fallback,
                compatible_surface: None,
            }))
            .ok()
        })?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("oxmacs.backdrop.software"),
            required_limits: adapter.limits(),
            ..wgpu::DeviceDescriptor::default()
        }))
        .ok()?;
        Some(Self { device, queue })
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "frames are a few hundred pixels across"
    )]
    fn render(&self, compiled: &mut HashMap<u64, Option<Compiled>>, job: &Job) -> Option<Handle> {
        let shader = &job.backdrop.shader;
        let compiled = compiled
            .entry(shader.key)
            .or_insert_with(|| Compiled::new(&self.device, FORMAT, &shader.source))
            .as_ref()?;
        let (width, height) = job.size;
        let scale = (MOST_PIXELS / (width * height).max(1.0)).sqrt().min(1.0);
        let wide = (width * scale).round().max(1.0) as u32;
        let high = (height * scale).round().max(1.0) as u32;
        let extent = wgpu::Extent3d {
            width: wide,
            height: high,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxmacs.backdrop.frame"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.queue.write_buffer(
            &compiled.uniforms,
            0,
            &job.backdrop.uniforms((0.0, 0.0), (width, height), scale),
        );
        let row = (wide * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxmacs.backdrop.readback"),
            size: u64::from(row * high),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oxmacs.backdrop.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..wgpu::RenderPassDescriptor::default()
            });
            compiled.draw(&mut pass);
        }
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            extent,
        );
        let index = self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: None,
            })
            .ok()?;
        let pixels: Vec<u8> = slice
            .get_mapped_range()
            .chunks(row as usize)
            .flat_map(|line| line[..wide as usize * 4].iter().copied())
            .collect();
        Some(Handle::from_rgba(wide, high, pixels))
    }
}
