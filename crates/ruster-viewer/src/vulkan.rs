//! Audited FFI boundary. Single event-loop thread, one submission in flight.
//! Device resources die before the instance, and the surface before the window.
use crate::scene::{Camera, Rect};
use ash::{vk, Entry};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    ffi::{CStr, CString},
    sync::atomic::{AtomicUsize, Ordering},
};
use winit::{dpi::PhysicalSize, window::Window};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
static VALIDATION_ERRORS: AtomicUsize = AtomicUsize::new(0);
static VALIDATION_WARNINGS: AtomicUsize = AtomicUsize::new(0);

pub fn validation_counts() -> (usize, usize) {
    (
        VALIDATION_ERRORS.load(Ordering::Relaxed),
        VALIDATION_WARNINGS.load(Ordering::Relaxed),
    )
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut std::ffi::c_void,
) -> vk::Bool32 {
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        VALIDATION_ERRORS.fetch_add(1, Ordering::Relaxed);
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        VALIDATION_WARNINGS.fetch_add(1, Ordering::Relaxed);
    }
    // SAFETY: Vulkan supplies valid callback data/message for the duration of this call.
    let message = unsafe { CStr::from_ptr((*data).p_message) };
    eprintln!(
        "Vulkan validation {severity:?}: {}",
        message.to_string_lossy()
    );
    vk::FALSE
}

struct InstanceState {
    // Entry must remain alive until all dynamically loaded calls finish.
    _entry: Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    debug: Option<ash::ext::debug_utils::Instance>,
    messenger: vk::DebugUtilsMessengerEXT,
}

impl Drop for InstanceState {
    fn drop(&mut self) {
        // SAFETY: Renderer drops DeviceState first; no child resources or pending device work remain.
        unsafe {
            if self.surface != vk::SurfaceKHR::null() {
                self.surface_loader.destroy_surface(self.surface, None);
            }
            if let Some(debug) = &self.debug {
                debug.destroy_debug_utils_messenger(self.messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

struct Swapchain {
    device: ash::Device,
    loader: ash::khr::swapchain::Device,
    handle: vk::SwapchainKHR,
    views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    finished: Vec<vk::Semaphore>,
    pass: vk::RenderPass,
    extent: vk::Extent2D,
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        // SAFETY: Caller idles the device before replacement/drop, including partial-construction errors.
        unsafe {
            for value in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(value, None);
            }
            for value in self.views.drain(..) {
                self.device.destroy_image_view(value, None);
            }
            for value in self.finished.drain(..) {
                self.device.destroy_semaphore(value, None);
            }
            if self.pass != vk::RenderPass::null() {
                self.device.destroy_render_pass(self.pass, None);
            }
            if self.handle != vk::SwapchainKHR::null() {
                self.loader.destroy_swapchain(self.handle, None);
            }
        }
    }
}

struct DeviceState {
    device: ash::Device,
    loader: ash::khr::swapchain::Device,
    queue: vk::Queue,
    family: u32,
    physical: vk::PhysicalDevice,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    acquired: vk::Semaphore,
    timestamps: vk::QueryPool,
    timestamp_bits: u32,
    timestamp_period: f32,
    query_pending: bool,
    swapchain: Option<Swapchain>,
}

impl Drop for DeviceState {
    fn drop(&mut self) {
        // SAFETY: This is the sole device owner, called after event-loop rendering stops.
        // DEVICE_LOST may reject wait_idle; destruction is still legal and must proceed.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.swapchain.take();
            if self.timestamps != vk::QueryPool::null() {
                self.device.destroy_query_pool(self.timestamps, None);
            }
            if self.acquired != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.acquired, None);
            }
            if self.fence != vk::Fence::null() {
                self.device.destroy_fence(self.fence, None);
            }
            if self.pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.pool, None);
            }
            self.device.destroy_device(None);
        }
    }
}

pub struct Renderer {
    // Declaration order is deliberate: Rust drops fields in declaration order.
    gpu: DeviceState,
    host: InstanceState,
    pub gpu_milliseconds: Vec<f64>,
}

impl Renderer {
    pub fn new(window: &Window, validation: bool) -> Result<Self> {
        // SAFETY: The loader is retained in InstanceState through final instance destruction.
        let entry = unsafe { Entry::load() }.map_err(|e| {
            format!("Vulkan loader unavailable: {e}; install a Vulkan driver/loader")
        })?;
        let display = window.display_handle()?.as_raw();
        let handle = window.window_handle()?.as_raw();
        let mut extensions = ash_window::enumerate_required_extensions(display)?.to_vec();
        let layer = CString::new("VK_LAYER_KHRONOS_validation")?;
        let mut layers = Vec::new();
        if validation {
            // SAFETY: Entry is live; enumeration owns returned properties.
            let available = unsafe { entry.enumerate_instance_layer_properties() }?;
            if !available
                .iter()
                .any(|p| unsafe { CStr::from_ptr(p.layer_name.as_ptr()) } == layer.as_c_str())
            {
                return Err("--validation requires VK_LAYER_KHRONOS_validation; install Vulkan validation layers or set VK_LAYER_PATH".into());
            }
            layers.push(layer.as_ptr());
            extensions.push(ash::ext::debug_utils::NAME.as_ptr());
            extensions.push(ash::ext::validation_features::NAME.as_ptr());
        }
        let name = CString::new("Ruster Vulkan schematic")?;
        let app = vk::ApplicationInfo::default()
            .application_name(&name)
            .api_version(vk::API_VERSION_1_0);
        let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_callback));
        let mut create = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&extensions)
            .enabled_layer_names(&layers);
        let sync_validation = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        let mut features =
            vk::ValidationFeaturesEXT::default().enabled_validation_features(&sync_validation);
        if validation {
            create = create.push_next(&mut debug_info).push_next(&mut features);
        }
        // SAFETY: All create-info pointers remain live through the call; no custom allocator.
        let instance = unsafe { entry.create_instance(&create, None) }?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let mut host = InstanceState {
            _entry: entry,
            instance,
            surface_loader,
            surface: vk::SurfaceKHR::null(),
            debug: None,
            messenger: vk::DebugUtilsMessengerEXT::null(),
        };
        if validation {
            let debug = ash::ext::debug_utils::Instance::new(&host._entry, &host.instance);
            // SAFETY: Instance and callback are live, with no user-data pointer.
            host.messenger = unsafe { debug.create_debug_utils_messenger(&debug_info, None) }?;
            host.debug = Some(debug);
        }
        // SAFETY: Window outlives Renderer; surface is destroyed before the owning window.
        host.surface = unsafe {
            ash_window::create_surface(&host._entry, &host.instance, display, handle, None)
        }?;
        // SAFETY: Instance remains live for all enumeration/query calls.
        let devices = unsafe { host.instance.enumerate_physical_devices() }?;
        let mut selected = None;
        for physical in devices {
            let properties = unsafe { host.instance.get_physical_device_properties(physical) };
            let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }.to_string_lossy();
            let extensions = unsafe {
                host.instance
                    .enumerate_device_extension_properties(physical)
            }?;
            if !extensions.iter().any(|e| unsafe { CStr::from_ptr(e.extension_name.as_ptr()) } == ash::khr::swapchain::NAME) { continue; }
            let families = unsafe {
                host.instance
                    .get_physical_device_queue_family_properties(physical)
            };
            for (index, family) in families.iter().enumerate() {
                if family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && unsafe {
                        host.surface_loader.get_physical_device_surface_support(
                            physical,
                            index as u32,
                            host.surface,
                        )
                    }?
                {
                    eprintln!("Vulkan device: {name}; api {}.{}.{}; driver {}; graphics/present queue {}; timestamp bits {}; validation {validation}", vk::api_version_major(properties.api_version), vk::api_version_minor(properties.api_version), vk::api_version_patch(properties.api_version), properties.driver_version, index, family.timestamp_valid_bits);
                    selected = Some((
                        physical,
                        index as u32,
                        family.timestamp_valid_bits,
                        properties.limits.timestamp_period,
                    ));
                    break;
                }
            }
            if selected.is_some() {
                break;
            }
        }
        let (physical, family, timestamp_bits, timestamp_period) = selected.ok_or("No Vulkan device with a combined graphics/presentation queue and VK_KHR_swapchain; check driver/display compatibility")?;
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let extensions = [ash::khr::swapchain::NAME.as_ptr()];
        let create = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&extensions);
        // SAFETY: Selected queue exists, requested core 1.0 feature set is empty.
        let device = unsafe { host.instance.create_device(physical, &create, None) }?;
        let loader = ash::khr::swapchain::Device::new(&host.instance, &device);
        let queue = unsafe { device.get_device_queue(family, 0) };
        let mut gpu = DeviceState {
            device,
            loader,
            queue,
            family,
            physical,
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            acquired: vk::Semaphore::null(),
            timestamps: vk::QueryPool::null(),
            timestamp_bits,
            timestamp_period,
            query_pending: false,
            swapchain: None,
        };
        // SAFETY: Every successful handle is immediately retained by the partial-construction guard.
        unsafe {
            gpu.pool = gpu.device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?;
            gpu.command = gpu.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(gpu.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0];
            gpu.fence = gpu.device.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?;
            gpu.acquired = gpu
                .device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
            if timestamp_bits > 0 {
                gpu.timestamps = gpu.device.create_query_pool(
                    &vk::QueryPoolCreateInfo::default()
                        .query_type(vk::QueryType::TIMESTAMP)
                        .query_count(2),
                    None,
                )?;
            }
        }
        let mut renderer = Self {
            gpu,
            host,
            gpu_milliseconds: Vec::new(),
        };
        renderer.resize(window.inner_size())?;
        Ok(renderer)
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        // SAFETY: Event-loop ownership prevents concurrent submits. Quiescence includes present work.
        unsafe { self.gpu.device.device_wait_idle() }?;
        let caps = unsafe {
            self.host
                .surface_loader
                .get_physical_device_surface_capabilities(self.gpu.physical, self.host.surface)
        }?;
        if !caps
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        {
            return Err("Surface does not support color attachments".into());
        }
        let formats = unsafe {
            self.host
                .surface_loader
                .get_physical_device_surface_formats(self.gpu.physical, self.host.surface)
        }?;
        let format = formats
            .iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_SRGB
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| formats.first())
            .copied()
            .ok_or("Surface has no formats")?;
        let format = if format.format == vk::Format::UNDEFINED {
            vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_UNORM,
                color_space: format.color_space,
            }
        } else {
            format
        };
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: size
                    .width
                    .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: size
                    .height
                    .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        if extent.width == 0 || extent.height == 0 {
            return Ok(());
        }
        let count = if caps.max_image_count > 0 {
            (caps.min_image_count + 1).min(caps.max_image_count)
        } else {
            caps.min_image_count + 1
        };
        let alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ]
        .into_iter()
        .find(|a| caps.supported_composite_alpha.contains(*a))
        .ok_or("Surface has no composite-alpha mode")?;
        // Creation retires the previous swapchain. Every error after this point is terminal;
        // a partially built replacement is destroyed by its guard and the old owner at shutdown.
        let old = self
            .gpu
            .swapchain
            .as_ref()
            .map_or(vk::SwapchainKHR::null(), |swap| swap.handle);
        let create = vk::SwapchainCreateInfoKHR::default()
            .surface(self.host.surface)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true)
            .old_swapchain(old);
        let mut swap = Swapchain {
            device: self.gpu.device.clone(),
            loader: self.gpu.loader.clone(),
            handle: vk::SwapchainKHR::null(),
            views: Vec::new(),
            framebuffers: Vec::new(),
            finished: Vec::new(),
            pass: vk::RenderPass::null(),
            extent,
        };
        // SAFETY: Surface/device are live; new resources are guarded and not yet used by the GPU.
        unsafe {
            swap.handle = self.gpu.loader.create_swapchain(&create, None)?;
            let images = self.gpu.loader.get_swapchain_images(swap.handle)?;
            let attachments = [vk::AttachmentDescription::default()
                .format(format.format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
            let colors = [vk::AttachmentReference::default()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
            let subpasses = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&colors)];
            let dependencies = [vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
            swap.pass = self.gpu.device.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )?;
            for image in images {
                let range = vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1);
                let view = self.gpu.device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(range),
                    None,
                )?;
                swap.views.push(view);
                let views = [view];
                let framebuffer = self.gpu.device.create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(swap.pass)
                        .attachments(&views)
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1),
                    None,
                )?;
                swap.framebuffers.push(framebuffer);
                swap.finished.push(
                    self.gpu
                        .device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
                );
            }
        }
        eprintln!(
            "Vulkan surface: {}x{} {:?}, {} images, FIFO; queue {}",
            extent.width,
            extent.height,
            format.format,
            swap.views.len(),
            self.gpu.family
        );
        self.gpu.swapchain = Some(swap);
        Ok(())
    }

    /// Returns false on an out-of-date/suboptimal swapchain; caller recreates before another frame.
    pub fn draw(&mut self, geometry: &[Rect], camera: Camera) -> Result<bool> {
        let gpu = &mut self.gpu;
        let Some(swap) = &gpu.swapchain else {
            return Ok(false);
        };
        // SAFETY: One frame in flight. Fence protects command/query/acquire-semaphore reuse.
        unsafe {
            gpu.device
                .wait_for_fences(&[gpu.fence], true, 5_000_000_000)?;
            if gpu.query_pending {
                let mut ticks = [0_u64; 2];
                gpu.device.get_query_pool_results(
                    gpu.timestamps,
                    0,
                    &mut ticks,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )?;
                let mask = u64::MAX >> (64 - gpu.timestamp_bits);
                let elapsed = ticks[1].wrapping_sub(ticks[0]) & mask;
                if self.gpu_milliseconds.len() < 600 {
                    self.gpu_milliseconds
                        .push(elapsed as f64 * f64::from(gpu.timestamp_period) / 1_000_000.0);
                }
                gpu.query_pending = false;
            }
            let (index, suboptimal) = match gpu.loader.acquire_next_image(
                swap.handle,
                5_000_000_000,
                gpu.acquired,
                vk::Fence::null(),
            ) {
                Ok(value) => value,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(false),
                Err(error) => return Err(error.into()),
            };
            gpu.device
                .reset_command_buffer(gpu.command, vk::CommandBufferResetFlags::empty())?;
            gpu.device.begin_command_buffer(
                gpu.command,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            if gpu.timestamps != vk::QueryPool::null() {
                gpu.device
                    .cmd_reset_query_pool(gpu.command, gpu.timestamps, 0, 2);
                gpu.device.cmd_write_timestamp(
                    gpu.command,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    gpu.timestamps,
                    0,
                );
            }
            let area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: swap.extent,
            };
            let clear = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.019, 0.032, 0.055, 1.0],
                },
            }];
            gpu.device.cmd_begin_render_pass(
                gpu.command,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(swap.pass)
                    .framebuffer(swap.framebuffers[index as usize])
                    .render_area(area)
                    .clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
            for rect in geometry {
                if let Some(area) = project(rect.bounds, camera, swap.extent) {
                    let attachments = [vk::ClearAttachment::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .color_attachment(0)
                        .clear_value(vk::ClearValue {
                            color: vk::ClearColorValue {
                                float32: rect.color,
                            },
                        })];
                    let rectangles = [vk::ClearRect::default()
                        .rect(area)
                        .base_array_layer(0)
                        .layer_count(1)];
                    gpu.device
                        .cmd_clear_attachments(gpu.command, &attachments, &rectangles);
                }
            }
            gpu.device.cmd_end_render_pass(gpu.command);
            if gpu.timestamps != vk::QueryPool::null() {
                gpu.device.cmd_write_timestamp(
                    gpu.command,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    gpu.timestamps,
                    1,
                );
            }
            gpu.device.end_command_buffer(gpu.command)?;
            let waits = [gpu.acquired];
            let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let commands = [gpu.command];
            // Per-image semaphore: acquire's wait orders reuse after that image's previous present.
            let signals = [swap.finished[index as usize]];
            let submits = [vk::SubmitInfo::default()
                .wait_semaphores(&waits)
                .wait_dst_stage_mask(&stages)
                .command_buffers(&commands)
                .signal_semaphores(&signals)];
            // Reset only once submission is ready. Any subsequent failure terminates rendering.
            gpu.device.reset_fences(&[gpu.fence])?;
            gpu.device.queue_submit(gpu.queue, &submits, gpu.fence)?;
            gpu.query_pending = gpu.timestamps != vk::QueryPool::null();
            let swapchains = [swap.handle];
            let indices = [index];
            match gpu.loader.queue_present(
                gpu.queue,
                &vk::PresentInfoKHR::default()
                    .wait_semaphores(&signals)
                    .swapchains(&swapchains)
                    .image_indices(&indices),
            ) {
                Ok(value) => Ok(!suboptimal && !value),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(false),
                Err(error) => Err(error.into()),
            }
        }
    }
}

fn project(bounds: [f32; 4], camera: Camera, extent: vk::Extent2D) -> Option<vk::Rect2D> {
    let scale = (extent.width as f32 / 1040.0).min(extent.height as f32 / 650.0) * camera.zoom;
    let x = (bounds[0] - camera.x) * scale + extent.width as f32 / 2.0;
    let y = (bounds[1] - camera.y) * scale + extent.height as f32 / 2.0;
    let left = x.round().clamp(0.0, extent.width as f32) as u32;
    let top = y.round().clamp(0.0, extent.height as f32) as u32;
    let right = (x + bounds[2] * scale)
        .round()
        .clamp(0.0, extent.width as f32) as u32;
    let bottom = (y + bounds[3] * scale)
        .round()
        .clamp(0.0, extent.height as f32) as u32;
    if right <= left || bottom <= top {
        return None;
    }
    Some(vk::Rect2D {
        offset: vk::Offset2D {
            x: left as i32,
            y: top as i32,
        },
        extent: vk::Extent2D {
            width: right - left,
            height: bottom - top,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_projection_clips_clear_rectangles_before_vulkan_submission() {
        let extent = vk::Extent2D {
            width: 1040,
            height: 650,
        };
        let cases = [
            (
                [-600.0, -400.0, 200.0, 100.0],
                Camera::default(),
                Some((0, 0, 120, 25)),
            ),
            ([-600.0, 0.0, 10.0, 10.0], Camera::default(), None),
            (
                [0.0, 0.0, 10.0, 10.0],
                Camera {
                    x: 10.0,
                    y: 20.0,
                    zoom: 2.0,
                },
                Some((500, 285, 20, 20)),
            ),
            ([0.0, 0.0, 0.01, 0.01], Camera::default(), None),
            (
                [500.0, 300.0, 100.0, 100.0],
                Camera::default(),
                Some((1020, 625, 20, 25)),
            ),
        ];
        for (bounds, camera, expected) in cases {
            let result = project(bounds, camera, extent);
            assert_eq!(
                result.map(|r| (r.offset.x, r.offset.y, r.extent.width, r.extent.height)),
                expected
            );
            if let Some(rect) = result {
                assert!(rect.offset.x >= 0 && rect.offset.y >= 0);
                assert!(rect.extent.width > 0 && rect.extent.height > 0);
                assert!(rect.offset.x as u32 + rect.extent.width <= extent.width);
                assert!(rect.offset.y as u32 + rect.extent.height <= extent.height);
            }
        }
    }
}
