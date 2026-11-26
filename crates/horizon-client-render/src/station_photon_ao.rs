//! Photon SSAO on station surfaces. Original SSAO include, no temporal accumulation.
use super::{
    station_photon_material::Material,
    station_scene_data::SceneData,
    station_shader_pack::{render_pipeline, run_pass},
};
use horizon_shaders::{Macros, Pack, Program, Target};
use std::path::Path;

pub fn load(path: &Path) -> Result<Program, String> {
    let pack = Pack::open(path).map_err(|e| e.to_string())?;
    let macros = Macros::standard();
    let vs = horizon_shaders::preprocess_source(&pack, "station_ao.vsh", VERTEX, &macros);
    let fs = horizon_shaders::preprocess_source(&pack, "station_ao.fsh", FRAGMENT, &macros);
    if !vs.diagnostics.is_empty() || !fs.diagnostics.is_empty() {
        return Err(format!(
            "Photon AO includes: {:?} {:?}",
            vs.diagnostics, fs.diagnostics
        ));
    }
    horizon_shaders::translate_composite(&vs, &fs, Target::Buffers)
        .map_err(|e| format!("Photon SSAO: {e:?}"))
}
pub struct Ao {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    interface: horizon_shaders::Interface,
    nearest: wgpu::Sampler,
    depth_pipeline: wgpu::RenderPipeline,
    combine: wgpu::RenderPipeline,
    targets: Option<Targets>,
    size: [u32; 2],
}
struct Targets {
    depth: wgpu::TextureView,
    ao: wgpu::TextureView,
    color: wgpu::TextureView,
    depth_group: wgpu::BindGroup,
    ao_group: wgpu::BindGroup,
}
impl Ao {
    pub fn new(device: &wgpu::Device, path: &Path) -> Result<Self, String> {
        let program = load(path)?;
        for s in &program.interface.samplers {
            if !matches!(s.name.as_str(), "depthtex1" | "colortex1") {
                return Err(format!("unbound AO sampler {}", s.name));
            }
        }
        for u in &program.interface.uniforms {
            if !u.name.starts_with("hz_")
                && !matches!(
                    u.name.as_str(),
                    "gbufferProjection"
                        | "gbufferProjectionInverse"
                        | "gbufferModelView"
                        | "gbufferModelViewInverse"
                        | "view_res"
                        | "view_pixel_size"
                        | "near"
                        | "far"
                )
            {
                return Err(format!("unbound AO uniform {}", u.name));
            }
        }
        let mut entries = super::station_shader_pipeline::sampled_entries(&program.interface)?;
        for e in &mut entries {
            match &mut e.ty {
                wgpu::BindingType::Texture { sample_type, .. } => {
                    *sample_type = wgpu::TextureSampleType::Float { filterable: false }
                }
                wgpu::BindingType::Sampler(s) => *s = wgpu::SamplerBindingType::NonFiltering,
                _ => {}
            }
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Photon SSAO scene inputs"),
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Photon AO vertex"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.vertex)),
        });
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Photon original SSAO include"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.fragment)),
        });
        let pipeline = render_pipeline(
            device,
            Some(&pl),
            &vs,
            &fs,
            "main",
            "main",
            wgpu::TextureFormat::R16Float,
        );
        let helper = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("station AO depth and composition"),
            source: wgpu::ShaderSource::Wgsl(HELPER.into()),
        });
        let depth_pipeline = render_pipeline(
            device,
            None,
            &helper,
            &helper,
            "vs",
            "depth",
            wgpu::TextureFormat::R32Float,
        );
        let combine = render_pipeline(
            device,
            None,
            &helper,
            &helper,
            "vs",
            "combine",
            wgpu::TextureFormat::Rgba16Float,
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("orthographic Photon AO camera"),
            size: program.interface.uniform_size.max(16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            pipeline,
            layout,
            uniform,
            interface: program.interface,
            nearest: device.create_sampler(&Default::default()),
            depth_pipeline,
            combine,
            targets: None,
            size: [0; 2],
        })
    }
    fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        scene: &SceneData,
        material: &Material,
    ) {
        if self.size == size {
            return;
        }
        let make = |format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("Photon SSAO frame"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let depth = make(wgpu::TextureFormat::R32Float);
        let ao = make(wgpu::TextureFormat::R16Float);
        let color = make(wgpu::TextureFormat::Rgba16Float);
        let depth_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Photon world depth range"),
            layout: &self.depth_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(scene.input("depthtex0").unwrap()),
            }],
        });
        let mut data = self.interface.initial_values.clone();
        data.resize(self.interface.uniform_size as usize, 0);
        let (projection, inverse, view, view_inverse) = camera(size);
        for u in &self.interface.uniforms {
            let values: Vec<f32> = match u.name.as_str() {
                "hz_target" => vec![
                    size[0] as f32,
                    size[1] as f32,
                    1. / size[0] as f32,
                    1. / size[1] as f32,
                ],
                "view_res" => vec![size[0] as f32, size[1] as f32],
                "view_pixel_size" => vec![1. / size[0] as f32, 1. / size[1] as f32],
                "gbufferProjection" => projection.to_vec(),
                "gbufferProjectionInverse" => inverse.to_vec(),
                "gbufferModelView" => view.to_vec(),
                "gbufferModelViewInverse" => view_inverse.to_vec(),
                "near" => vec![(1. + projection[14]) / projection[10]],
                "far" => vec![(projection[14] - 1.) / projection[10]],
                _ => continue,
            };
            let bytes = bytemuck::cast_slice(&values);
            let offset = u.offset as usize;
            data[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
        queue.write_buffer(&self.uniform, 0, &data);
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: self.uniform.as_entire_binding(),
        }];
        for s in &self.interface.samplers {
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding,
                resource: wgpu::BindingResource::TextureView(if s.name == "colortex1" {
                    material.view()
                } else {
                    &depth
                }),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding + 1,
                resource: wgpu::BindingResource::Sampler(&self.nearest),
            });
        }
        let ao_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Photon AO bound scene"),
            layout: &self.layout,
            entries: &entries,
        });
        self.targets = Some(Targets {
            depth,
            ao,
            color,
            depth_group,
            ao_group,
        });
        self.size = size;
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        scene: &SceneData,
        material: &Material,
        color: &wgpu::TextureView,
    ) {
        self.resize(device, queue, size, scene, material);
        let t = self.targets.as_ref().unwrap();
        run_pass(encoder, &t.depth, &self.depth_pipeline, &t.depth_group, 3);
        run_pass(encoder, &t.ao, &self.pipeline, &t.ao_group, 6);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Photon AO composition"),
            layout: &self.combine.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&t.ao),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&t.depth),
                },
            ],
        });
        run_pass(encoder, &t.color, &self.combine, &group, 3);
    }
    pub fn color(&self) -> &wgpu::TextureView {
        &self.targets.as_ref().unwrap().color
    }
}
// Camera-relative view: native z becomes view z-40. Depth remap reserves Photon's hand range.
fn camera(size: [u32; 2]) -> ([f32; 16], [f32; 16], [f32; 16], [f32; 16]) {
    let h = super::VIEW_HALF_HEIGHT;
    let x = h * size[0] as f32 / size[1] as f32;
    let z = -0.8 * super::DEPTH_SCALE;
    let offset = 0.2 + 0.8 * super::DEPTH_BIAS + z * 40.;
    (
        [
            1. / x,
            0.,
            0.,
            0.,
            0.,
            1. / h,
            0.,
            0.,
            0.,
            0.,
            z,
            0.,
            0.,
            0.,
            offset,
            1.,
        ],
        [
            x,
            0.,
            0.,
            0.,
            0.,
            h,
            0.,
            0.,
            0.,
            0.,
            1. / z,
            0.,
            0.,
            0.,
            -offset / z,
            1.,
        ],
        [
            1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., -40., 1.,
        ],
        [
            1., 0., 0., 0., 0., 0., -1., 0., 0., 1., 0., 0., 0., 40., 0., 1.,
        ],
    )
}
const VERTEX: &str = "#version 400 compatibility\nout vec2 uv;void main(){uv=gl_MultiTexCoord0.xy;gl_Position=vec4(gl_Vertex.xy*2.0-1.0,0.0,1.0);}";
const FRAGMENT: &str = r#"#version 400 compatibility
#define WORLD_OVERWORLD
#include "/include/global.glsl"
// This station adapter has no jitter or temporal history yet.
#undef TAA
#undef TAAU
uniform sampler2D depthtex1;
uniform sampler2D colortex1;
uniform mat4 gbufferProjection;
uniform mat4 gbufferProjectionInverse;
uniform mat4 gbufferModelView;
uniform mat4 gbufferModelViewInverse;
uniform vec2 view_res;
uniform vec2 view_pixel_size;
uniform float near;
uniform float far;
in vec2 uv;
#include "/include/utility/encoding.glsl"
#include "/include/lighting/ao/ssao.glsl"
void main(){
 ivec2 p=ivec2(gl_FragCoord.xy);
 float depth=texelFetch(depthtex1,p,0).r;
 if(depth>=1.0){gl_FragColor=vec4(1.0);return;}
 vec4 data=texelFetch(colortex1,p,0);
 vec3 normal=mat3(gbufferModelView)*decode_unit_vector(unpack_unorm_2x8(data.z));
 vec3 screen=vec3(uv,depth);
 vec3 view=screen_to_view_space(screen,false);
 vec2 dither=fract(sin(vec2(dot(vec2(p),vec2(12.9898,78.233)),dot(vec2(p),vec2(39.346,11.135))))*43758.5453);
 float ao=compute_ssao(screen,view,normal,dither);
 gl_FragColor=vec4(ao,ao,ao,1.0);
}
"#;
const HELPER: &str = r"
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var ao:texture_2d<f32>;
@group(0) @binding(2) var surface_depth:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(p[i],0.,1.);}
@fragment fn depth(@builtin(position) p:vec4f)->@location(0) f32 {let d=textureLoad(source,vec2i(p.xy),0).r;return select(0.6+0.4*d,1.,d>=1.);}
@fragment fn combine(@builtin(position) p:vec4f)->@location(0) vec4f {
 let center=vec2i(p.xy);let color=textureLoad(source,center,0);
 let d=textureLoad(surface_depth,center,0).r;
 let dims=vec2i(textureDimensions(ao));var sum=0.;var weights=0.;
 for(var y=-2;y<=2;y++){for(var x=-2;x<=2;x++){
   let q=clamp(center+vec2i(x,y),vec2i(0),dims-vec2i(1));
   let other=textureLoad(surface_depth,q,0).r;
   let w=exp(-0.5*f32(x*x+y*y))*exp(-abs(other-d)*10000.);
   sum+=clamp(textureLoad(ao,q,0).r,0.,1.)*w;weights+=w;
 }}
 return vec4f(color.rgb*(sum/max(weights,0.0001)),color.a);
}
";

#[cfg(test)]
mod tests {
    use super::*;
    fn mul(m: [f32; 16], v: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|r| (0..4).map(|c| m[4 * c + r] * v[c]).sum())
    }
    #[test]
    fn orthographic_camera_matches_remapped_station_depth() {
        let (p, inv, view, view_inv) = camera([1200, 800]);
        for z in [-20., 0., 2., 10., 30.] {
            let world = [2., z, -3., 1.];
            let v = mul(view, world);
            let clip = mul(p, v);
            let depth = 0.5 * clip[2] + 0.5;
            assert!((depth - (0.6 + 0.4 * (0.4 - 0.01 * z))).abs() < 1e-6);
            assert!(depth > 0.56 && depth < 1.);
            let back = mul(view_inv, mul(inv, clip));
            for i in 0..4 {
                assert!((back[i] - world[i]).abs() < 1e-4);
            }
        }
    }
    #[test]
    fn generated_include_preserves_origins_without_writing_pack() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/scene-data");
        let pack = Pack::open(&path).unwrap();
        let pp = horizon_shaders::preprocess_source(
            &pack,
            "virtual.fsh",
            "#version 400\n#include \"/final.fsh\"",
            &Macros::standard(),
        );
        assert!(pp.diagnostics.is_empty(), "{:?}", pp.diagnostics);
        assert!(pp.files.iter().any(|f| f == "final.fsh"));
        assert!(pp.code.contains("stationAlbedo"));
        assert!(!pack.exists("virtual.fsh"));
        assert_eq!(pp.lines.len(), pp.code.split('\n').count());
    }
}
