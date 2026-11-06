//! Opt-in ownership regression against the production ShadowGpu, not a second
//! shadow implementation. Callback delivery is deliberately delayed by omitting
//! submit/poll calls; this does not claim the device remains physically busy.
use super::*;
use crate::shadow_gpu::{Frame, Geometry, Request, ShadowGpu};
use ruster_scene::{ShadowQuality, SunShadow};
use std::sync::atomic::{AtomicUsize, Ordering};

const STANDARD_BYTES: u64 = 1024 * 1024 * 4;
const HIGH_BYTES: u64 = 2048 * 2048 * 4;

struct Probe {
    shadows: ShadowGpu,
    cube: GpuMesh,
    meshes: BTreeMap<String, GpuMesh>,
    instances: wgpu::Buffer,
    groups: Vec<DrawGroup>,
    context: egui::Context,
    repaint_requests: Arc<AtomicUsize>,
}

impl Probe {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let (vertices, indices) = cube();
        let instance = Instance {
            model: Mat4::IDENTITY.to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.; 4],
            base_color: [1., 1., 1., 0.],
        };
        let instances = instance_buffer(device, 1);
        queue.write_buffer(&instances, 0, bytemuck::bytes_of(&instance));
        let context = egui::Context::default();
        let repaint_requests = Arc::new(AtomicUsize::new(0));
        let observed = repaint_requests.clone();
        context.set_repaint_observer(move |_| {
            observed.fetch_add(1, Ordering::Relaxed);
        });
        Self {
            shadows: ShadowGpu::new(device, queue),
            cube: GpuMesh::new(device, None, &vertices, &indices),
            meshes: BTreeMap::new(),
            instances,
            groups: vec![DrawGroup {
                key: None,
                binding: BindingKey::default(),
                texture: None,
                mesh: None,
                instances: vec![instance],
                range: 0..1,
            }],
            context,
            repaint_requests,
        }
    }

    /// Run real egui frame accounting and the production frame claim. The
    /// caller decides whether to submit this owned command buffer or drop it.
    fn frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        settings: Option<SunShadow>,
        has_geometry: bool,
        direction: Vec3,
    ) -> (Frame, wgpu::CommandBuffer) {
        let context = self.context.clone();
        let before = context.cumulative_frame_nr();
        let mut frame = None;
        let mut encoder = device.create_command_encoder(&Default::default());
        context
            .run_pass(egui::RawInput::default(), |context| {
                assert!(frame.is_none(), "the probe must produce one egui pass");
                assert!(self.shadows.claim_frame(context));
                frame = Some(self.shadows.prepare(
                    device,
                    queue,
                    &mut encoder,
                    Geometry {
                        groups: if has_geometry { &self.groups } else { &[] },
                        meshes: &self.meshes,
                        cube: &self.cube,
                        instances: &self.instances,
                    },
                    Request {
                        settings,
                        direction,
                        builtin: true,
                        context,
                        timestamps: None,
                    },
                ));
                // This is the actual production binding choice, including the
                // neutral fallback while an occupied slot retains a live map.
                let _ = self.shadows.bind();
                assert!(
                    !self.shadows.claim_frame(context),
                    "a second callback cannot claim the same real egui frame"
                );
                // Do not prepare again after rejection. Subsequent allocation
                // and pass assertions protect that boundary; this is claim
                // evidence, not a proof of SceneCallback's composite behavior.
            })
            .drop_without_applying_deltas();
        assert_eq!(context.cumulative_frame_nr(), before + 1);
        let frame = frame.unwrap();
        assert!(frame.live_bytes <= HIGH_BYTES);
        assert!(frame.retiring_bytes <= HIGH_BYTES);
        assert!(frame.live_bytes + frame.retiring_bytes <= STANDARD_BYTES + HIGH_BYTES);
        (frame, encoder.finish())
    }

    fn drain(&self, device: &wgpu::Device) {
        let before = self.repaint_requests.load(Ordering::Relaxed);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("real queue progress must deliver retirement callbacks");
        assert!(
            self.repaint_requests.load(Ordering::Relaxed) > before,
            "the genuine queue callback must request a later egui frame"
        );
    }
}

#[test]
#[ignore = "actual Vulkan ShadowGpu ownership; root GPU/validation lease required"]
fn sun_shadow_gpu_retirement_lifecycle_probe() {
    // Retain Vulkan's log-based driver diagnostics in captured stderr as well
    // as the explicit wgpu validation scope and uncaptured-error callback.
    struct Diagnostics;
    impl log::Log for Diagnostics {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() <= log::Level::Warn
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                eprintln!("{} {}: {}", record.level(), record.target(), record.args());
            }
        }
        fn flush(&self) {}
    }
    static DIAGNOSTICS: Diagnostics = Diagnostics;
    // Existing test-process logging, if installed, remains its owner's sink.
    let _ = log::set_logger(&DIAGNOSTICS);
    log::set_max_level(log::LevelFilter::Warn);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = if let Ok(name) = std::env::var("RUSTER_SHADOW_PROBE_ADAPTER") {
        let mut matches = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN))
            .into_iter()
            .filter(|adapter| adapter.get_info().name.contains(&name));
        let adapter = matches
            .next()
            .expect("requested lifecycle adapter must exist");
        assert!(
            matches.next().is_none(),
            "lifecycle adapter must be unambiguous"
        );
        adapter
    } else {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("Vulkan adapter")
    };
    eprintln!("ShadowGpu lifecycle adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let recorded = errors.clone();
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        eprintln!("uncaptured GPU error: {error}");
        recorded.lock().unwrap().push(error.to_string());
    }));
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut probe = Probe::new(&device, &queue);
    let direction = Vec3::new(-1., 2., 0.).normalize();
    let standard = SunShadow {
        enabled: true,
        ..SunShadow::default()
    };
    let high = SunShadow {
        quality: ShadowQuality::High,
        ..standard
    };

    // The initial real submitted pass establishes prior GPU work. There is no
    // hidden full-size allocation while disabled, and unchanged rendering reuses
    // the same production map instead of incrementing the allocation count.
    let (disabled, commands) = probe.frame(&device, &queue, None, true, direction);
    assert_eq!(disabled.status, ShadowStatus::Disabled);
    assert!(!disabled.rendered && !disabled.pending);
    assert_eq!(disabled.passes, 0);
    assert_eq!(
        (
            disabled.live_bytes,
            disabled.retiring_bytes,
            disabled.allocations
        ),
        (0, 0, 0)
    );
    queue.submit([commands]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let (first, commands) = probe.frame(&device, &queue, Some(standard), true, direction);
    assert_eq!(first.status, ShadowStatus::Ready);
    assert_eq!(
        (
            first.map_size,
            first.live_bytes,
            first.retiring_bytes,
            first.allocations
        ),
        (1024, STANDARD_BYTES, 0, 1)
    );
    assert!(first.rendered && !first.pending);
    assert_eq!((first.passes, first.draw_calls), (1, 1));
    queue.submit([commands]);

    // Hold callback delivery by performing no submit/poll from here until drain.
    // These subsequently encoded passes are dropped rather than pretending they
    // were submitted frames. The original Standard pass above is real GPU work.
    let (upgraded, unsent) = probe.frame(&device, &queue, Some(high), true, direction);
    drop(unsent);
    assert_eq!(upgraded.status, ShadowStatus::Ready);
    assert_eq!(
        (
            upgraded.map_size,
            upgraded.live_bytes,
            upgraded.retiring_bytes,
            upgraded.allocations
        ),
        (2048, HIGH_BYTES, STANDARD_BYTES, 2)
    );
    assert!(upgraded.pending);
    let (occupied, unsent) = probe.frame(&device, &queue, Some(standard), true, direction);
    drop(unsent);
    assert_eq!(occupied.status, ShadowStatus::Pending);
    assert_eq!(
        (
            occupied.map_size,
            occupied.live_bytes,
            occupied.retiring_bytes,
            occupied.allocations
        ),
        (2048, HIGH_BYTES, STANDARD_BYTES, 2)
    );
    let (disabled, unsent) = probe.frame(&device, &queue, None, true, direction);
    drop(unsent);
    assert_eq!(disabled.status, ShadowStatus::Disabled);
    assert!(!disabled.rendered && disabled.pending);
    assert_eq!(
        (
            disabled.live_bytes,
            disabled.retiring_bytes,
            disabled.allocations
        ),
        (HIGH_BYTES, STANDARD_BYTES, 2)
    );
    assert_eq!(disabled.passes, occupied.passes);
    let (reenabled, unsent) = probe.frame(&device, &queue, Some(standard), true, direction);
    drop(unsent);
    assert_eq!(reenabled.status, ShadowStatus::Pending);
    assert!(reenabled.rendered && reenabled.pending);
    assert_eq!(
        (
            reenabled.live_bytes,
            reenabled.retiring_bytes,
            reenabled.allocations
        ),
        (HIGH_BYTES, STANDARD_BYTES, 2)
    );
    probe.drain(&device);

    // A genuine later frame reclaims the completed Standard retirement before
    // replacing High. Its returned submission runs the replacement depth pass.
    let (replaced, commands) = probe.frame(&device, &queue, Some(standard), true, direction);
    assert_eq!(replaced.status, ShadowStatus::Ready);
    assert_eq!(
        (
            replaced.live_bytes,
            replaced.retiring_bytes,
            replaced.allocations
        ),
        (STANDARD_BYTES, HIGH_BYTES, 3)
    );
    assert!(replaced.pending);
    // Drain before submit so this separately demonstrates that retirement is
    // based on genuinely prior submitted work, never the just-encoded buffer.
    probe.drain(&device);
    queue.submit([commands]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let (steady, commands) = probe.frame(&device, &queue, Some(standard), true, direction);
    assert_eq!(steady.status, ShadowStatus::Ready);
    assert_eq!(
        (steady.live_bytes, steady.retiring_bytes, steady.allocations),
        (STANDARD_BYTES, 0, 3)
    );
    assert!(!steady.pending);
    queue.submit([commands]);

    // Empty/invalid coverage must not allocate/repaint forever. Test while the
    // retirement slot is occupied as well as after genuine completion delivery.
    let (empty, unsent) = probe.frame(&device, &queue, Some(standard), false, direction);
    drop(unsent);
    assert_eq!(empty.status, ShadowStatus::NoGeometry);
    assert_eq!(
        (empty.live_bytes, empty.retiring_bytes, empty.allocations),
        (0, STANDARD_BYTES, 3)
    );
    let (invalid, unsent) = probe.frame(&device, &queue, Some(high), true, Vec3::ZERO);
    drop(unsent);
    assert_eq!(invalid.status, ShadowStatus::InvalidCoverage);
    assert_eq!(
        (
            invalid.live_bytes,
            invalid.retiring_bytes,
            invalid.allocations
        ),
        (0, STANDARD_BYTES, 3)
    );
    assert!(!invalid.rendered && invalid.pending);
    assert_eq!(invalid.passes, empty.passes);
    probe.drain(&device);
    for (has_geometry, direction, expected) in [
        (false, direction, ShadowStatus::NoGeometry),
        (true, Vec3::ZERO, ShadowStatus::InvalidCoverage),
        (false, direction, ShadowStatus::NoGeometry),
    ] {
        let (frame, unsent) = probe.frame(&device, &queue, Some(high), has_geometry, direction);
        drop(unsent);
        assert_eq!(frame.status, expected);
        assert!(!frame.rendered && !frame.pending);
        assert_eq!(
            (
                frame.live_bytes,
                frame.retiring_bytes,
                frame.allocations,
                frame.passes
            ),
            (0, 0, 3, empty.passes)
        );
    }

    // Re-enable after full drain, execute real High work, then disable and drain
    // to the neutral binding with no application-owned full-size map remaining.
    let (high_frame, commands) = probe.frame(&device, &queue, Some(high), true, direction);
    assert_eq!(high_frame.status, ShadowStatus::Ready);
    assert_eq!(
        (
            high_frame.live_bytes,
            high_frame.retiring_bytes,
            high_frame.allocations
        ),
        (HIGH_BYTES, 0, 4)
    );
    queue.submit([commands]);
    let (last_retirement, unsent) = probe.frame(&device, &queue, None, true, direction);
    drop(unsent);
    assert!(!last_retirement.rendered && last_retirement.pending);
    assert_eq!(
        (last_retirement.live_bytes, last_retirement.retiring_bytes),
        (0, HIGH_BYTES)
    );
    probe.drain(&device);
    let (drained, commands) = probe.frame(&device, &queue, None, true, direction);
    assert_eq!(drained.status, ShadowStatus::Disabled);
    assert!(!drained.rendered && !drained.pending);
    assert_eq!(
        (
            drained.map_size,
            drained.live_bytes,
            drained.retiring_bytes,
            drained.allocations
        ),
        (0, 0, 0, 4)
    );
    queue.submit([commands]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    drop(probe);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    verify_rejected_callback_paint(&device, &queue);
    assert!(
        pollster::block_on(scope.pop()).is_none(),
        "production shadow lifecycle validation"
    );
    assert!(
        errors.lock().unwrap().is_empty(),
        "uncaptured lifecycle GPU errors"
    );
    eprintln!("ShadowGpu lifecycle: delayed genuine retirement, occupied-slot quality/toggle bounds, real egui frame progress, empty/invalid no-allocation state, final drain and actual rejected-callback paint readback passed; logical map payload only, not whole-driver peak memory");
}

/// A rejected second viewport must not composite the first viewport's retained
/// scene target. This tests actual SceneCallback prepare/paint and copied pixels,
/// rather than treating claim_frame or its per-callback flag as visual proof.
fn verify_rejected_callback_paint(device: &wgpu::Device, queue: &wgpu::Queue) {
    use egui_wgpu::CallbackTrait;
    use std::sync::atomic::AtomicBool;

    const SIZE: u32 = 64;
    let context = egui::Context::default();
    let r = Resources::new(device, queue, COLOR, 1);
    let metrics = r.metrics.clone();
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let object = Instance {
        model: Mat4::IDENTITY.to_cols_array_2d(),
        normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
        color: [1.; 4],
        base_color: [1., 1., 1., 0.],
    };
    let first = SceneCallback {
        groups: vec![DrawGroup {
            key: None,
            binding: BindingKey::default(),
            texture: None,
            mesh: None,
            instances: vec![object],
            range: 0..1,
        }],
        camera: Mat4::IDENTITY.to_cols_array(),
        lighting: LightingPacket::try_pack(&ruster_scene::Lighting::authored_default("sun"))
            .unwrap(),
        size: egui::vec2(SIZE as f32, SIZE as f32),
        cpu_us: 0.,
        shadow: Some(SunShadow {
            enabled: true,
            ..SunShadow::default()
        }),
        context: context.clone(),
        prepared: AtomicBool::new(false),
    };
    let second = SceneCallback {
        groups: Vec::new(),
        camera: Mat4::from_scale(Vec3::splat(0.5)).to_cols_array(),
        lighting: LightingPacket::authored_dark(),
        size: egui::vec2(17., 19.),
        cpu_us: 0.,
        shadow: None,
        context: context.clone(),
        prepared: AtomicBool::new(false),
    };
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [SIZE, SIZE],
        pixels_per_point: 1.,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    let before_frame = context.cumulative_frame_nr();
    let mut prepared_once = false;
    context
        .run_pass(egui::RawInput::default(), |_| {
            assert!(!prepared_once, "one real egui pass for both callbacks");
            prepared_once = true;
            assert!(first
                .prepare(device, queue, &screen, &mut encoder, &mut resources)
                .is_empty());
            assert!(first.prepared.load(Ordering::Relaxed));
            let before = metrics.lock().unwrap().clone();
            assert_eq!(before.frames, 1);
            assert_eq!(before.shadow_passes, 1);
            assert_eq!(before.shadow_status, ShadowStatus::Ready);
            assert!(second
                .prepare(device, queue, &screen, &mut encoder, &mut resources)
                .is_empty());
            assert!(!second.prepared.load(Ordering::Relaxed));
            let after = metrics.lock().unwrap().clone();
            assert_eq!(after.shadow_status, ShadowStatus::MultipleViewports);
            assert_eq!(
                callback_work(&after),
                callback_work(&before),
                "rejected prepare cannot write/upload, resize or encode another scene"
            );
        })
        .drop_without_applying_deltas();
    assert_eq!(context.cumulative_frame_nr(), before_frame + 1);

    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("distinct rejected-viewport magenta target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = image.create_view(&Default::default());
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rejected callback actual pixels"),
        size: u64::from(SIZE * SIZE * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SIZE as f32, SIZE as f32));
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("paint only the rejected second callback"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.,
                            g: 0.,
                            b: 1.,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        second.paint(
            egui::PaintCallbackInfo {
                viewport: rect,
                clip_rect: rect,
                pixels_per_point: 1.,
                screen_size_px: [SIZE, SIZE],
            },
            &mut pass,
            &resources,
        );
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (send, recv) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let bytes = read
        .slice(..)
        .get_mapped_range()
        .expect("rejected callback readback mapping");
    assert_eq!(bytes.len(), (SIZE * SIZE * 4) as usize);
    for (index, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
        assert_eq!(
            pixel,
            &[255, 0, 255, 255],
            "rejected second callback painted pixel {index} from the first scene"
        );
    }
    drop(bytes);
    read.unmap();
    drop(resources);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

fn callback_work(metrics: &Metrics) -> [u64; 15] {
    [
        metrics.frames,
        metrics.mesh_uploads,
        metrics.mesh_upload_bytes,
        metrics.texture_uploads,
        metrics.texture_upload_bytes,
        metrics.lighting_uploads,
        metrics.lighting_upload_bytes,
        metrics.shadow_allocations,
        metrics.shadow_passes,
        metrics.instance_capacity_bytes,
        metrics.target_bytes,
        u64::from(metrics.target_size[0]),
        u64::from(metrics.target_size[1]),
        metrics.objects as u64,
        metrics.draw_calls as u64,
    ]
}
