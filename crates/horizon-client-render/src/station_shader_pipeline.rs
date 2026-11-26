//! Loading and capability checks for the complete authored shader pipeline.
//! This module does not execute passes or synthesize missing station scene buffers.
use horizon_shaders::{GameInputs, Macros, Pack, Post, PostProgram};
use std::path::Path;

/// Unlike the final-only adapter, preserves all of the pack's default options.
/// The caller must supply its real geometry contract before using the resulting programs.
pub fn load(path: &Path, folder: &str, inputs: &GameInputs) -> Result<Post, String> {
    let pack = Pack::open(path).map_err(|e| e.to_string())?;
    let mut macros = Macros::standard();
    let (properties, feature_notes) = horizon_shaders::read_properties(&pack, &mut macros);
    if !feature_notes.is_empty() {
        return Err(format!("unsupported pack features: {feature_notes:?}"));
    }
    let post = horizon_shaders::load_post(&pack, folder, &properties, &macros, inputs);
    if !post.notes.is_empty() {
        return Err(format!("pipeline loading diagnostics: {:?}", post.notes));
    }
    if post.programs.is_empty() {
        return Err(format!("no pipeline programs in {folder}"));
    }
    Ok(post)
}

pub fn programs(post: &Post) -> impl Iterator<Item = &PostProgram> {
    post.programs.iter().chain(post.shadow_comp.iter()).chain(
        [
            &post.terrain,
            &post.terrain_cutout,
            &post.water,
            &post.sky,
            &post.sky_textured,
            &post.entities,
            &post.block,
            &post.block_translucent,
            &post.entities_translucent,
            &post.particles,
            &post.particles_translucent,
            &post.beacon_beam,
            &post.damaged_block,
            &post.clouds,
            &post.lines,
            &post.weather,
            &post.hand,
            &post.hand_water,
            &post.shadow,
            &post.shadow_water,
            &post.shadow_entities,
            &post.shadow_block,
        ]
        .into_iter()
        .filter_map(Option::as_ref),
    )
}

fn location_end(binding: &Option<naga::Binding>) -> u32 {
    match binding {
        Some(naga::Binding::Location { location, .. }) => location + 1,
        _ => 0,
    }
}

fn vertex_locations(module: &naga::Module) -> u32 {
    module
        .entry_points
        .iter()
        .filter(|e| e.stage == naga::ShaderStage::Vertex)
        .filter_map(|e| e.function.result.as_ref())
        .map(|result| {
            let direct = location_end(&result.binding);
            match &module.types[result.ty].inner {
                naga::TypeInner::Struct { members, .. } => members
                    .iter()
                    .map(|member| location_end(&member.binding))
                    .max()
                    .unwrap_or(direct),
                _ => direct,
            }
        })
        .max()
        .unwrap_or(0)
}

/// The requested limit is computed from translated vertex outputs, not assumed to be 16.
/// Other device requirements (formats, storage, MRT, etc.) still need their own checks.
pub fn inter_stage_limit(post: &Post, available: u32) -> Result<u32, String> {
    let (name, required) = programs(post)
        .filter_map(|p| {
            p.program
                .as_ref()
                .map(|program| (p.name.as_str(), vertex_locations(&program.vertex)))
        })
        .max_by_key(|(_, count)| *count)
        .unwrap_or(("", 0));
    if required > available {
        return Err(format!(
            "{name} needs {required} inter-stage locations; GPU supports {available}"
        ));
    }
    Ok(required)
}

/// Device limits for the sampled-texture graphics programs supported by `sampled_entries`.
/// Does not imply support for storage, compute, actual attachments or scene rendering.
pub fn sampled_graphics_limits(
    post: &Post,
    available: &wgpu::Limits,
) -> Result<wgpu::Limits, String> {
    let mut limits = wgpu::Limits::default();
    limits.max_inter_stage_shader_variables =
        limits
            .max_inter_stage_shader_variables
            .max(inter_stage_limit(
                post,
                available.max_inter_stage_shader_variables,
            )?);
    for p in programs(post) {
        if let Some(program) = &p.program {
            sampled_entries(&program.interface)?;
            let count = program.interface.samplers.len() as u32;
            if count > available.max_sampled_textures_per_shader_stage
                || count > available.max_samplers_per_shader_stage
            {
                return Err(format!(
                    "{} requires {count} sampled textures and samplers per stage",
                    p.name
                ));
            }
            limits.max_sampled_textures_per_shader_stage =
                limits.max_sampled_textures_per_shader_stage.max(count);
            limits.max_samplers_per_shader_stage = limits.max_samplers_per_shader_stage.max(count);
        }
    }
    if !limits.check_limits(available) {
        return Err("GPU does not satisfy requested pipeline limits".into());
    }
    Ok(limits)
}

/// Shared explicit layout prevents VS texelFetch and FS filtered reads from deriving
/// incompatible types for the same texture (Photon deferred4/colortex4).
pub fn sampled_entries(
    interface: &horizon_shaders::Interface,
) -> Result<Vec<wgpu::BindGroupLayoutEntry>, String> {
    if !interface.images.is_empty() || !interface.buffers.is_empty() {
        return Err("storage images/buffers need a separate pipeline layout".into());
    }
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    for s in &interface.samplers {
        let (dimension, comparison) = match s.ty.as_str() {
            "sampler2D" => (wgpu::TextureViewDimension::D2, false),
            "sampler3D" => (wgpu::TextureViewDimension::D3, false),
            "sampler2DShadow" => (wgpu::TextureViewDimension::D2, true),
            _ => return Err(format!("unsupported sampled layout type: {}", s.ty)),
        };
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: s.binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: if comparison {
                    wgpu::TextureSampleType::Depth
                } else {
                    wgpu::TextureSampleType::Float {
                        filterable: s.name != "depthtex0",
                    }
                },
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: s.binding + 1,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Sampler(if comparison {
                wgpu::SamplerBindingType::Comparison
            } else if s.name == "depthtex0" {
                wgpu::SamplerBindingType::NonFiltering
            } else {
                wgpu::SamplerBindingType::Filtering
            }),
            count: None,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_post_loader_preserves_order_metadata_and_reports_disabled() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/horizon-shaders/tests/packs/post-test");
        let pack = Pack::open(&path).unwrap();
        let macros = Macros::standard();
        let props = horizon_shaders::Properties::parse(
            "scale.composite=0.5 0.25 0.5\nflip.composite.colortex2=false\nblend.composite=ONE ONE ZERO ONE\nblend.composite.colortex0=off",
            &macros,
        );
        let post = horizon_shaders::load_post_subset(
            &pack,
            "",
            &props,
            &macros,
            &["composite1", "composite"],
        );
        assert!(post.notes.is_empty(), "{:?}", post.notes);
        assert_eq!(
            post.programs
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["composite", "composite1"]
        );
        assert!(post.terrain.is_none());
        let first = &post.programs[0];
        assert_eq!(first.scale, [0.5, 0.25, 0.5]);
        assert_eq!(first.flips, [(2, false)]);
        assert_eq!(first.buffers, [0, 2]);
        // Horizon's Iris-compatible post semantics ignore per-buffer blend overrides.
        assert!(first.buffer_blends.is_empty());
        let raster = super::super::station_shader_raster::Raster::from_post(first);
        assert!(
            raster
                .targets()
                .iter()
                .all(|t| t.as_ref().unwrap().blend.is_some())
        );
        let props = horizon_shaders::Properties::parse("program.composite.enabled=false", &macros);
        let disabled = horizon_shaders::load_post_subset(
            &pack,
            "",
            &props,
            &macros,
            &["composite", "composite99"],
        );
        assert!(disabled.programs.is_empty());
        assert_eq!(disabled.notes.len(), 2);
        assert!(
            disabled
                .notes
                .iter()
                .all(|n| n.contains("missing, disabled or failed"))
        );
    }
    #[test]
    fn counts_sparse_output_locations_and_ignores_position() {
        let module = naga::front::wgsl::parse_str("struct Out { @builtin(position) p:vec4f, @location(18) x:vec4f }; @vertex fn main()->Out { return Out(vec4f(0.),vec4f(1.)); }").unwrap();
        assert_eq!(vertex_locations(&module), 19);
    }
    #[test]
    fn loads_all_fixture_passes_without_dropping_composites() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/horizon-shaders/tests/packs/post-test");
        let post = load(&path, "", &GameInputs::default()).unwrap();
        let names: Vec<_> = programs(&post).map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"composite"));
        assert!(names.contains(&"composite1"));
        assert!(names.contains(&"final"));
        assert!(inter_stage_limit(&post, 32).is_ok());
        assert!(inter_stage_limit(&post, 0).is_err());
    }
    #[test]
    fn negotiates_eighteen_samplers_and_refuses_a_smaller_gpu() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/horizon-shaders/tests/packs/post-test");
        let mut post = load(&path, "", &GameInputs::default()).unwrap();
        post.programs[0]
            .program
            .as_mut()
            .unwrap()
            .interface
            .samplers = (0..18)
            .map(|i| horizon_shaders::Sampler {
                name: format!("texture{i}"),
                ty: "sampler2D".into(),
                binding: 1 + 2 * i,
            })
            .collect();
        let mut available = wgpu::Limits::default();
        assert!(sampled_graphics_limits(&post, &available).is_err());
        available.max_sampled_textures_per_shader_stage = 32;
        available.max_samplers_per_shader_stage = 32;
        let requested = sampled_graphics_limits(&post, &available).unwrap();
        assert_eq!(requested.max_sampled_textures_per_shader_stage, 18);
        assert_eq!(requested.max_samplers_per_shader_stage, 18);
    }
    #[test]
    fn comparison_sampler_requires_depth_texture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/horizon-shaders/tests/packs/post-test");
        let mut post = load(&path, "", &GameInputs::default()).unwrap();
        let interface = &mut post.programs[0].program.as_mut().unwrap().interface;
        interface.samplers = vec![horizon_shaders::Sampler {
            name: "shadowtex0".into(),
            ty: "sampler2DShadow".into(),
            binding: 1,
        }];
        let entries = sampled_entries(interface).unwrap();
        assert!(matches!(
            entries[1].ty,
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                ..
            }
        ));
        assert!(matches!(
            entries[2].ty,
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)
        ));
        interface.samplers[0].ty = "unrecognized".into();
        assert!(sampled_entries(interface).is_err());
    }
    #[test]
    fn missing_dimension_is_not_reported_as_loaded() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/horizon-shaders/tests/packs/post-test");
        assert!(load(&path, "missing", &GameInputs::default()).is_err());
    }
}
