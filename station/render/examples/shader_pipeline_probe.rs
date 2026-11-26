//! Compile the complete authored pipeline, without claiming GPU/render compatibility.
use horizon_shaders::{GameInputs, GbuffersInputs};
use horizon_station_render::station_shader_pipeline;
fn main() {
    let path = std::env::args().nth(1).expect("shader pack path");
    let input = GbuffersInputs::default();
    let inputs = GameInputs {
        terrain: Some(input.clone()),
        entities: Some(input.clone()),
        sky: Some(input.clone()),
        celestial: Some(input.clone()),
        clouds: Some(input.clone()),
        weather: Some(input.clone()),
        lines: Some(input.clone()),
        hand: Some(input),
        ..Default::default()
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&Default::default())).expect("GPU adapter");
    println!("Adapter: {:?}", adapter.get_info());
    let post = station_shader_pipeline::load(std::path::Path::new(&path), "world0", &inputs)
        .expect("complete pipeline translation");
    let limits = station_shader_pipeline::sampled_graphics_limits(&post, &adapter.limits())
        .expect("GPU limits");
    println!(
        "Requested inter-stage locations: {}",
        limits.max_inter_stage_shader_variables
    );
    let (device, _queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))
    .expect("GPU device");
    let mut failures = 0;
    let mut checked = 0;
    for p in station_shader_pipeline::programs(&post) {
        println!(
            "{} {:?} buffers={:?} compute={}",
            p.name,
            p.group,
            p.buffers,
            p.compute.len()
        );
        for (name, compute) in &p.compute {
            checked += 1;
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(name),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(compute.module.clone())),
            });
            let _pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
            if let Some(error) = pollster::block_on(scope.pop()) {
                failures += 1;
                println!("  COMPUTE FAILED {name}: {error}");
            } else {
                println!("  GPU compute pipeline compiled: {name}");
            }
        }
        if let Some(program) = &p.program {
            checked += 1;
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&p.name),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.vertex.clone())),
            });
            let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&p.name),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.fragment.clone())),
            });
            let targets: Vec<_> = program
                .interface
                .outputs
                .iter()
                .map(|_| {
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })
                })
                .collect();
            let entries = station_shader_pipeline::sampled_entries(&program.interface)
                .expect("sampled resource contract");
            let inputs_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("pack inputs"),
                entries: &entries,
            });
            let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[],
            });
            let draw = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("draw uniforms"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&p.name),
                bind_group_layouts: &[Some(&inputs_layout), Some(&empty), Some(&draw)],
                immediate_size: 0,
            });
            let _pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&p.name),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &vertex,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fragment,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &targets,
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            if let Some(error) = pollster::block_on(scope.pop()) {
                failures += 1;
                println!("  GPU FAILED: {error}");
            } else {
                println!("  GPU pipeline compiled (probe attachments)");
            }
            println!("  samplers={:?}", program.interface.samplers);
            println!("  uniforms={:?}", program.interface.uniforms);
        }
    }
    println!("GPU pipelines checked={checked} failed={failures}");
    if failures > 0 {
        std::process::exit(1);
    }
    println!("NOTES ({})", post.notes.len());
    for n in &post.notes {
        println!("{n}");
    }
    println!(
        "Translation only. Geometry inputs are placeholders; GPU pipeline creation only, no rendering or station compatibility asserted."
    );
}
