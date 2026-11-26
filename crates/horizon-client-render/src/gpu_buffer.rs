//! Shared GPU allocation extracted from Horizon terrain arena.
pub fn try_create(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> Option<wgpu::Buffer> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: false,
    });
    // Native error scopes resolve at once.
    pollster::block_on(scope.pop()).is_none().then_some(buffer)
}
