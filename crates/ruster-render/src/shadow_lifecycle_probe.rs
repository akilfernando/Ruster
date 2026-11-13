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
    builtins: BuiltinMeshes,
    meshes: BTreeMap<String, GpuMesh>,
    instances: wgpu::Buffer,
    groups: Vec<DrawGroup>,
    context: egui::Context,
    repaint_requests: Arc<AtomicUsize>,
}

impl Probe {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let instance = Instance {
            model: Mat4::IDENTITY.to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.; 4],
            base_color: [1., 1., 1., 0.],
            response: [0.; 4],
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
            builtins: BuiltinMeshes {
                cube: GpuMesh::new(device, None, &primitives::cube()),
                sphere: GpuMesh::new(device, None, builtin_mesh(ruster_scene::Primitive::Sphere)),
                capsule: GpuMesh::new(device, None, builtin_mesh(ruster_scene::Primitive::Capsule)),
            },
            meshes: BTreeMap::new(),
            instances,
            groups: vec![DrawGroup {
                key: None,
                primitive: None,
                binding: BindingKey::default(),
                texture: None,
                roughness: None,
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
                        builtins: &self.builtins,
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
        assert!(frame.live_bytes + frame.retiring_bytes <= 2 * HIGH_BYTES);
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
    assert_eq!(disabled.requested_size, 0);
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
    assert_eq!(occupied.requested_size, 1024);
    assert_eq!(occupied.uniform_uploads, upgraded.uniform_uploads);
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
        disabled.requested_size, 0,
        "retained payload is not an enabled request"
    );
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
    assert_eq!(steady.uniform_uploads, replaced.uniform_uploads);
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
    assert_eq!(invalid.uniform_uploads, empty.uniform_uploads);
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
    // Same-quality re-enable before completion can own two High maps, unlike
    // the Standard/High replacement above. Exercise the actual declared32MiB
    // overlap bound, not merely the20MiB observed in alternating-quality work.
    let (recreated, unsent) = probe.frame(&device, &queue, Some(high), true, direction);
    drop(unsent);
    assert_eq!(
        (
            recreated.live_bytes,
            recreated.retiring_bytes,
            recreated.allocations
        ),
        (HIGH_BYTES, HIGH_BYTES, 5)
    );
    let (repeat, unsent) = probe.frame(&device, &queue, Some(high), true, direction);
    drop(unsent);
    assert_eq!(repeat.allocations, recreated.allocations);
    assert_eq!(repeat.uniform_uploads, recreated.uniform_uploads);
    let (disabled, unsent) = probe.frame(&device, &queue, None, true, direction);
    drop(unsent);
    assert!(!disabled.rendered && disabled.pending);
    assert_eq!(
        (disabled.live_bytes, disabled.retiring_bytes),
        (HIGH_BYTES, HIGH_BYTES)
    );
    probe.drain(&device);
    let (cleanup, unsent) = probe.frame(&device, &queue, None, true, direction);
    drop(unsent);
    assert_eq!(
        (cleanup.live_bytes, cleanup.retiring_bytes),
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
        (0, 0, 0, 5)
    );
    queue.submit([commands]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    drop(probe);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    verify_rejected_callback_paint(&device, &queue);
    for change in [
        AbandonedChange::Unchanged,
        AbandonedChange::ExposureOnly,
        AbandonedChange::TargetOnly,
        AbandonedChange::TargetAndExposure,
    ] {
        verify_asset_submission_backpressure(&instance, &adapter, &device, &queue, change);
    }
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
        response: [0.; 4],
    };
    let first = SceneCallback {
        groups: vec![DrawGroup {
            key: None,
            primitive: None,
            binding: BindingKey::default(),
            texture: None,
            roughness: None,
            mesh: None,
            instances: vec![object],
            range: 0..1,
        }],
        camera: Mat4::IDENTITY.to_cols_array(),
        camera_eye_mode: [0., 0., 2., 1.],
        display: ruster_scene::DisplayProfile::LegacySdr,
        lighting: LightingPacket::try_pack(&ruster_scene::Lighting::authored_default("sun"))
            .unwrap(),
        size: egui::vec2(SIZE as f32, SIZE as f32),
        cpu_us: 0.,
        shadow: Some(SunShadow {
            enabled: true,
            ..SunShadow::default()
        }),
        context: context.clone(),
        receipt: FrameReceipt::default(),
        prepared: AtomicBool::new(false),
    };
    let second = SceneCallback {
        groups: Vec::new(),
        camera: Mat4::from_scale(Vec3::splat(0.5)).to_cols_array(),
        camera_eye_mode: [0., 0., 2., 1.],
        display: ruster_scene::DisplayProfile::LegacySdr,
        lighting: LightingPacket::authored_dark(),
        size: egui::vec2(17., 19.),
        cpu_us: 0.,
        shadow: None,
        context: context.clone(),
        receipt: FrameReceipt::default(),
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
    // Keep the preceding submitted64² target in the real retirement slot.
    // Hold only observation of genuine completion: submit/poll can deliver its
    // callback immediately, so omitting poll is not a deterministic barrier.
    let retained = SceneCallback {
        size: egui::vec2(63., 63.),
        shadow: None,
        receipt: FrameReceipt::default(),
        prepared: AtomicBool::new(false),
        ..first
    };
    let pending = SceneCallback {
        display: ruster_scene::DisplayProfile::SurfaceHdr { exposure_stops: 4. },
        receipt: FrameReceipt::default(),
        prepared: AtomicBool::new(false),
        ..second
    };
    {
        let r = resources.get_mut::<Resources>().unwrap();
        r.hold_target_acknowledgement = true;
        assert!(r.resize(device, queue, [63, 63], COLOR, 1, &context));
        assert!(r.retiring_target.is_some());
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    context
        .run_pass(egui::RawInput::default(), |_| {
            retained.prepare(device, queue, &screen, &mut encoder, &mut resources);
            assert!(retained.prepared.load(Ordering::Relaxed));
        })
        .drop_without_applying_deltas();
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("establish exact fresh retained target eligibility"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        retained.paint(
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
    assert!(retained.receipt.is_fresh_painted());
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(
        resources
            .get::<Resources>()
            .unwrap()
            .retiring_target
            .as_ref()
            .unwrap()
            .completed
            .load(Ordering::Acquire),
        "target retirement must receive genuine completion while only its observation is held"
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    let before = metrics.lock().unwrap().clone();
    context
        .run_pass(egui::RawInput::default(), |_| {
            pending.prepare(device, queue, &screen, &mut encoder, &mut resources);
            assert!(
                pending.prepared.load(Ordering::Relaxed),
                "accepted pending callback must retain last prepared composite"
            );
            let after = metrics.lock().unwrap().clone();
            assert_eq!(after.display_status, DisplayStatus::Pending);
            assert!(after.display_requested_hdr);
            assert_eq!(after.display_requested_exposure_stops, 4.);
            assert!(!after.display_applied_hdr);
            assert_eq!(after.display_applied_exposure_stops, 0.);
            assert_eq!(after.target_requested_size, [17, 19]);
            assert_eq!(after.target_size, [63, 63]);
            assert_eq!(after.target_allocations, before.target_allocations);
            assert_eq!(
                callback_work(&after),
                callback_work(&before),
                "pending callback must not encode or upload new scene state"
            );
            // Rejected duplicates are still unable to paint, even in this pending
            // frame; accepting a stale target cannot weaken the unique-frame guard.
            retained.prepare(device, queue, &screen, &mut encoder, &mut resources);
            assert!(!retained.prepared.load(Ordering::Relaxed));
        })
        .drop_without_applying_deltas();
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pending accepted callback must paint retained scene"),
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
        pending.paint(
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
    let bytes = read.slice(..).get_mapped_range().unwrap();
    let center = &bytes[((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize..][..4];
    assert!(
        center[..3].iter().all(|&value| value.abs_diff(215) <= 2) && center[3] == 255,
        "pending callback must composite last lit scene, not clear/pending dark pixels: {center:?}"
    );
    drop(bytes);
    read.unmap();
    {
        let r = resources.get_mut::<Resources>().unwrap();
        assert!(
            r.retiring_target.is_some(),
            "the test hold keeps the fixed retirement slot occupied"
        );
        r.hold_target_acknowledgement = false;
        let allocations = r.target_allocations;
        assert!(r.resize(device, queue, [63, 63], COLOR, 1, &context));
        assert!(
            r.retiring_target.is_none(),
            "release must reap the genuinely completed target"
        );
        assert_eq!(
            r.target_allocations, allocations,
            "reaping does not allocate another target"
        );
    }
    eprintln!("SceneCallback pending retirement retains actual lit scene pixels and applied state; duplicates remain unpaintable");
    drop(resources);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbandonedChange {
    Unchanged,
    ExposureOnly,
    TargetOnly,
    TargetAndExposure,
}

/// Upload-only submission and real logic maintenance share the production
/// RenderState API. No surface, fake completion, or synthetic paint witness.
fn verify_asset_submission_backpressure(
    instance: &wgpu::Instance,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    change: AbandonedChange,
) {
    use egui_wgpu::CallbackTrait;
    use std::sync::atomic::AtomicBool;

    let context = egui::Context::default();
    let wakes = Arc::new(AtomicUsize::new(0));
    let delayed = Arc::new(AtomicUsize::new(0));
    let observed = wakes.clone();
    let delayed_observer = delayed.clone();
    context.set_repaint_observer(move |request| {
        observed.fetch_add(1, Ordering::Relaxed);
        if request.delay == std::time::Duration::from_millis(16) {
            delayed_observer.fetch_add(1, Ordering::Relaxed);
        }
    });
    let mut renderer =
        egui_wgpu::Renderer::new(device, COLOR, egui_wgpu::RendererOptions::default());
    let mut r = Resources::new(device, queue, COLOR, 1);
    let changes_configuration = change != AbandonedChange::Unchanged;
    let changes_target = matches!(
        change,
        AbandonedChange::TargetOnly | AbandonedChange::TargetAndExposure
    );
    if matches!(
        change,
        AbandonedChange::ExposureOnly | AbandonedChange::TargetAndExposure
    ) {
        r.enable_hdr(device, 1);
    }
    let metrics = r.metrics.clone();
    renderer.callback_resources.insert(r);
    let render = egui_wgpu::RenderState {
        adapter: adapter.clone(),
        #[cfg(not(target_arch = "wasm32"))]
        available_adapters: vec![adapter.clone()],
        instance: instance.clone(),
        device: device.clone(),
        queue: queue.clone(),
        target_format: COLOR,
        renderer: Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
    };
    let make_callback = |extent: f32, color: [u8; 4], size: f32| {
        let mesh = Arc::new(ruster_assets::parse_obj(format!(
            "v -{extent} -1 0.5\nv {extent} -1 0.5\nv 0 1 0.5\nvt 0 0\nvt 1 0\nvt 0.5 1\nf 1/1 2/2 3/3\n"
        ).as_bytes()).unwrap());
        let texture = Arc::new(ruster_assets::Texture {
            role: ruster_assets::TextureRole::BaseColor,
            width: 1,
            height: 1,
            mips: vec![ruster_assets::TextureMip {
                width: 1,
                height: 1,
                bytes: color.to_vec(),
            }],
        });
        SceneCallback {
            groups: vec![DrawGroup {
                key: Some("epoch-mesh".into()),
                primitive: None,
                binding: BindingKey {
                    texture: Some("epoch-image".into()),
                    ..Default::default()
                },
                texture: Some(texture),
                roughness: None,
                mesh: Some(mesh),
                instances: vec![Instance {
                    model: Mat4::IDENTITY.to_cols_array_2d(),
                    normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                    color: [1.; 4],
                    base_color: [1., 1., 1., 0.],
                    response: [0.; 4],
                }],
                range: 0..1,
            }],
            camera: Mat4::IDENTITY.to_cols_array(),
            camera_eye_mode: [0., 0., 2., 1.],
            display: ruster_scene::DisplayProfile::LegacySdr,
            lighting: LightingPacket::try_pack(&ruster_scene::Lighting::authored_default("sun"))
                .unwrap(),
            size: egui::vec2(size, size),
            cpu_us: 0.,
            shadow: None,
            context: context.clone(),
            receipt: FrameReceipt::default(),
            prepared: AtomicBool::new(false),
        }
    };
    let mut a = make_callback(1., [255, 0, 0, 255], 64.);
    if change == AbandonedChange::ExposureOnly {
        a.display = ruster_scene::DisplayProfile::SurfaceHdr { exposure_stops: 0. };
    }
    let mut b = make_callback(
        0.8,
        [0, 255, 0, 255],
        if changes_target { 63. } else { 64. },
    );
    if matches!(
        change,
        AbandonedChange::ExposureOnly | AbandonedChange::TargetAndExposure
    ) {
        b.display = ruster_scene::DisplayProfile::SurfaceHdr { exposure_stops: 3. };
    }
    let c = make_callback(0.6, [0, 0, 255, 255], 63.);
    let mut d = make_callback(0.4, [255, 255, 0, 255], 62.);
    d.display = ruster_scene::DisplayProfile::SurfaceHdr { exposure_stops: 4. };
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [64, 64],
        pixels_per_point: 1.,
    };
    let frame = |callback: &SceneCallback| {
        let mut encoder = device.create_command_encoder(&Default::default());
        context
            .run_pass(egui::RawInput::default(), |_| {
                let mut renderer = render.renderer.write();
                callback.prepare(
                    device,
                    queue,
                    &screen,
                    &mut encoder,
                    &mut renderer.callback_resources,
                );
            })
            .drop_without_applying_deltas();
        encoder.finish()
    };
    let logic = || {
        let before = (
            context.cumulative_frame_nr(),
            metrics.lock().unwrap().frames,
        );
        let mut pending = None;
        let _ = context.run_logic(&egui::RawInput::default(), |ctx| {
            pending = Some(maintain_assets(&render, ctx).unwrap());
        });
        assert_eq!(
            (
                context.cumulative_frame_nr(),
                metrics.lock().unwrap().frames
            ),
            before,
            "logic maintenance must not invent an egui/rendered frame"
        );
        pending.unwrap()
    };
    let drain = || {
        let _ = logic();
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert!(
            !logic(),
            "genuine completion must release the sole retired generation"
        );
        let observation = display_observation(&render).unwrap();
        assert_eq!(
            (
                observation.asset_retired_mesh_bytes,
                observation.asset_retired_texture_bytes
            ),
            (0, 0)
        );
        assert_eq!(observation.asset_epoch_phase, "idle");
    };
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("asset epoch actual composite"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
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
        label: Some("asset epoch readback"),
        size: 64 * 64 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let paint = |callback: &SceneCallback| {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let renderer = render.renderer.read();
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("exact asset callback composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64., 64.));
            callback.paint(
                egui::PaintCallbackInfo {
                    viewport: rect,
                    clip_rect: rect,
                    pixels_per_point: 1.,
                    screen_size_px: [64, 64],
                },
                &mut pass,
                &renderer.callback_resources,
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
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
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
        let bytes = read.slice(..).get_mapped_range().unwrap().to_vec();
        read.unmap();
        bytes
    };

    let delayed_before = delayed.load(Ordering::Relaxed);
    let commands = frame(&a);
    assert!(
        !a.receipt.is_fresh_painted(),
        "prepare cannot certify a painted frame"
    );
    assert!(
        delayed.load(Ordering::Relaxed) > delayed_before,
        "isolated first upload must schedule its own maintenance wake"
    );
    queue.submit([commands]);
    let pixels_a = paint(&a);
    assert!(a.receipt.is_fresh_painted());
    drain();
    let accepted_a = metrics.lock().unwrap().clone();
    assert_eq!(
        (accepted_a.texture_uploads, accepted_a.mesh_uploads),
        (1, 4) // Three engine-owned meshes plus the admitted asset.
    );

    let delayed_before = delayed.load(Ordering::Relaxed);
    drop(frame(&b)); // Failed surface acquisition: abandon the scene encoder.
    assert!(!b.receipt.is_fresh_painted());
    assert!(
        delayed.load(Ordering::Relaxed) > delayed_before,
        "final isolated replacement cannot rely on an unrelated UI event"
    );
    queue.submit(std::iter::empty()); // Actual pending upload-only submission.
    {
        let mut renderer = render.renderer.write();
        renderer
            .callback_resources
            .get_mut::<Resources>()
            .unwrap()
            .assets
            .hold_acknowledgement = true;
    }
    let accepted_b = metrics.lock().unwrap().clone();
    assert_eq!(accepted_b.texture_uploads, accepted_a.texture_uploads + 1);
    assert_eq!(accepted_b.mesh_uploads, accepted_a.mesh_uploads + 1);
    assert_eq!(
        accepted_b.asset_retired_mesh_bytes,
        accepted_a.asset_live_mesh_bytes
    );
    assert_eq!(
        accepted_b.asset_retired_texture_bytes,
        accepted_a.asset_live_texture_bytes
    );
    assert!(logic()); // Arms/polls only; real completion remains held, not fabricated.
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    {
        let renderer = render.renderer.read();
        assert!(renderer
            .callback_resources
            .get::<Resources>()
            .unwrap()
            .assets
            .genuinely_completed());
    }
    assert!(
        logic(),
        "test holds acknowledgement after genuine GPU completion"
    );
    assert!(
        !b.receipt.is_fresh_painted(),
        "logic completion is not a paint witness"
    );

    if change == AbandonedChange::ExposureOnly {
        assert_eq!(
            accepted_b.target_allocations, accepted_a.target_allocations,
            "exposure-only fixture must isolate packet invalidation without target allocation"
        );
    }
    if changes_target {
        assert_eq!(
            accepted_b.target_allocations,
            accepted_a.target_allocations + 1,
            "target fixture must install a distinct actual allocation"
        );
    }
    if changes_configuration {
        let stale_pixels = paint(&a);
        assert!(
            stale_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [0, 0, 0, 255]),
            "a stale-generation callback must not composite the unproven current configuration"
        );
        let renderer = render.renderer.read();
        assert!(
            !renderer
                .callback_resources
                .get::<Resources>()
                .unwrap()
                .retained_image_eligible
                .load(Ordering::Acquire),
            "stale paint cannot re-enable retained-image eligibility"
        );
    }
    for (name, callback) in [("C", &c), ("D", &d)] {
        let mut encoder = device.create_command_encoder(&Default::default());
        context
            .run_pass(egui::RawInput::default(), |_| {
                let mut renderer = render.renderer.write();
                callback.prepare(
                    device,
                    queue,
                    &screen,
                    &mut encoder,
                    &mut renderer.callback_resources,
                );
                let pending = metrics.lock().unwrap().clone();
                assert_eq!(
                    pending.target_allocations, accepted_b.target_allocations,
                    "unfinished shared epoch must withhold {name} before target allocation"
                );
                assert_eq!(
                    callback_work(&pending),
                    callback_work(&accepted_b),
                    "unfinished shared epoch must withhold {name} before scene writes/uploads"
                );
                assert!(pending.asset_pending);
                assert_eq!(pending.asset_epoch_phase, "armed");
                assert!(
                    pending.asset_live_mesh_bytes + pending.asset_retired_mesh_bytes
                        <= 2 * ruster_assets::MAX_RESOLVED_BYTES as u64
                );
                assert!(
                    pending.asset_live_texture_bytes + pending.asset_retired_texture_bytes
                        <= 2 * ruster_assets::MAX_TEXTURE_RESOLVED_BYTES as u64
                );
                assert_eq!(pending.display_status, if changes_configuration { DisplayStatus::PendingUnavailable } else { DisplayStatus::Pending },
                    "pending image availability must match the actual painted configuration: {change:?}");
                if changes_configuration {
                    assert!(!callback.prepared.load(Ordering::Relaxed),
                        "an abandoned resized/exposure target must be unavailable, never composited as last-good");
                    assert_eq!(pending.display_status.warning(), Some("Waiting for the viewport to finish rendering."));
                }
                if name == "D" {
                    assert!(pending.display_requested_hdr);
                    assert_eq!(pending.display_requested_exposure_stops, 4.);
                    assert_eq!(pending.target_requested_size, [62, 62]);
                    assert_eq!(pending.display_applied_hdr, accepted_b.display_applied_hdr);
                    assert_eq!(pending.display_applied_exposure_stops, accepted_b.display_applied_exposure_stops);
                }
                a.prepare(
                    device,
                    queue,
                    &screen,
                    &mut encoder,
                    &mut renderer.callback_resources,
                );
                assert!(!a.prepared.load(Ordering::Relaxed));
                let duplicate = metrics.lock().unwrap().clone();
                assert_eq!(callback_work(&duplicate), callback_work(&pending));
                assert_eq!(
                    (
                        duplicate.asset_epoch_phase,
                        duplicate.asset_retired_mesh_bytes,
                        duplicate.asset_retired_texture_bytes
                    ),
                    (
                        pending.asset_epoch_phase,
                        pending.asset_retired_mesh_bytes,
                        pending.asset_retired_texture_bytes
                    ),
                    "same-frame rejection precedes epoch maintenance"
                );
            })
            .drop_without_applying_deltas();
        drop(encoder);
        queue.submit(std::iter::empty());
        let pending_pixels = paint(callback);
        if changes_configuration {
            assert!(pending_pixels.as_chunks::<4>().0.iter().all(|pixel| *pixel == [0, 0, 0, 255]),
                "{name}/{change:?} must leave the actual cleared background untouched while no safe image exists");
        } else {
            assert_eq!(
                pending_pixels, pixels_a,
                "{name} must composite exactly the previous complete image"
            );
        }
        assert!(
            !callback.receipt.is_fresh_painted(),
            "retained pixels must not certify {name}"
        );
    }
    let mesh_a = Arc::downgrade(a.groups[0].mesh.as_ref().unwrap());
    let texture_a = Arc::downgrade(a.groups[0].texture.as_ref().unwrap());
    let mesh_b = Arc::downgrade(b.groups[0].mesh.as_ref().unwrap());
    let texture_b = Arc::downgrade(b.groups[0].texture.as_ref().unwrap());
    drop(a);
    drop(b);
    drop(c);
    assert!(mesh_a.upgrade().is_none() && texture_a.upgrade().is_none());
    assert!(
        mesh_b.upgrade().is_none() && texture_b.upgrade().is_none(),
        "active/retired GPU owners must retain no CPU snapshot"
    );
    {
        let mut renderer = render.renderer.write();
        renderer
            .callback_resources
            .get_mut::<Resources>()
            .unwrap()
            .assets
            .hold_acknowledgement = false;
    }
    assert!(!logic());
    let quiet_wakes = wakes.load(Ordering::Relaxed);
    for _ in 0..4 {
        assert!(!logic());
    }
    assert_eq!(
        wakes.load(Ordering::Relaxed),
        quiet_wakes,
        "completed logic maintenance must stop scheduling wakes"
    );
    let commands = frame(&d); // Latest state only; C was never retained/adopted.
    assert!(!d.receipt.is_fresh_painted());
    let admitted_d = metrics.lock().unwrap().clone();
    assert_eq!(admitted_d.mesh_uploads, accepted_b.mesh_uploads + 1);
    assert_eq!(admitted_d.texture_uploads, accepted_b.texture_uploads + 1);
    {
        let renderer = render.renderer.read();
        assert!(renderer
            .callback_resources
            .get::<Resources>()
            .unwrap()
            .meshes["epoch-mesh"]
            .matches(d.groups[0].mesh.as_ref().unwrap()));
    }
    queue.submit([commands]);
    assert_ne!(paint(&d), pixels_a);
    assert!(
        d.receipt.is_fresh_painted(),
        "only fresh exact callback paint certifies pixels"
    );
    {
        let renderer = render.renderer.read();
        assert!(
            renderer
                .callback_resources
                .get::<Resources>()
                .unwrap()
                .retained_image_eligible
                .load(Ordering::Acquire),
            "an exact fresh D paint must restore eligibility for its configuration"
        );
    }
    drain();
    queue.submit([frame(&d)]);
    let unchanged = metrics.lock().unwrap().clone();
    assert_eq!(
        (unchanged.mesh_uploads, unchanged.texture_uploads),
        (admitted_d.mesh_uploads, admitted_d.texture_uploads)
    );
    assert!(
        !unchanged.asset_pending,
        "unchanged identity does not create an epoch"
    );
    assert!(!d.receipt.is_fresh_painted());
    let _ = paint(&d);
    assert!(d.receipt.is_fresh_painted());
    let receipt = d.receipt.clone();
    let source = Arc::downgrade(d.groups[0].mesh.as_ref().unwrap());
    drop(d);
    assert!(
        source.upgrade().is_none(),
        "scalar receipt must not keep the callback/assets alive"
    );
    assert!(receipt.is_fresh_painted());
    // Stable rejected identities are already represented by fallback; they
    // cannot manufacture another generation/upload on every ordinary redraw.
    let mut invalid = make_callback(0.5, [255; 4], 62.);
    Arc::get_mut(invalid.groups[0].mesh.as_mut().unwrap())
        .unwrap()
        .vertices[0]
        .position[0] = f32::NAN;
    Arc::get_mut(invalid.groups[0].texture.as_mut().unwrap())
        .unwrap()
        .mips[0]
        .bytes
        .clear();
    queue.submit([frame(&invalid)]);
    drain();
    let fallback = metrics.lock().unwrap().clone();
    queue.submit([frame(&invalid)]);
    let repeated = metrics.lock().unwrap().clone();
    assert_eq!(
        (repeated.mesh_uploads, repeated.texture_uploads),
        (fallback.mesh_uploads, fallback.texture_uploads)
    );
    assert!(
        !repeated.asset_pending,
        "stable invalid fallback owns no new GPU generation"
    );
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    drop(render);
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
