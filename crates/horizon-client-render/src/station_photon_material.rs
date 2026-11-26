//! Photon 1.4 material ABI bridge. Encoding functions are read from the user's pack.
//! Generic opaque material only; does not invent Minecraft IDs, lightmaps or emissions.
use super::{
    station_scene_data::SceneData,
    station_shader_pack::{render_pipeline, run_pass},
};
use horizon_shaders::{Macros, Pack, Preprocessed, Program, Target};
use std::path::Path;

fn source(code: String) -> Preprocessed {
    Preprocessed {
        lines: code
            .lines()
            .enumerate()
            .map(|(i, _)| horizon_shaders::Origin {
                file: 0,
                line: i as u32 + 1,
            })
            .collect(),
        code,
        files: vec!["station bridge + pack include/utility/encoding.glsl".into()],
        version: Some((400, Some("compatibility".into()))),
        ..Default::default()
    }
}
pub fn programs(path: &Path) -> Result<(Program, Program), String> {
    let pack = Pack::open(path).map_err(|e| e.to_string())?;
    let mut macros = Macros::standard();
    macros.define_function("rcp", vec!["x".into()], "(1.0 / (x))");
    let encoding = horizon_shaders::preprocess(&pack, "include/utility/encoding.glsl", &macros);
    if !encoding.diagnostics.is_empty() {
        return Err(format!("Photon encoding: {:?}", encoding.diagnostics));
    }
    let common = format!(
        "float max_of(vec4 v) {{return max(max(v.x,v.y),max(v.z,v.w));}}\n{}\n{}",
        encoding.code, COLOR
    );
    let vertex = source(VERTEX.into());
    let encode = source(format!("{common}\n{ENCODE}"));
    let decode = source(format!("{common}\n{DECODE}"));
    Ok((
        horizon_shaders::translate_composite(&vertex, &encode, Target::Buffers)
            .map_err(|e| format!("Photon material encoder: {e:?}"))?,
        horizon_shaders::translate_composite(&vertex, &decode, Target::Window)
            .map_err(|e| format!("Photon material decoder: {e:?}"))?,
    ))
}
pub struct Material {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    interface: horizon_shaders::Interface,
    sampler: wgpu::Sampler,
    target: Option<wgpu::TextureView>,
    group: Option<wgpu::BindGroup>,
    size: [u32; 2],
}
impl Material {
    pub fn new(device: &wgpu::Device, program: Program) -> Result<Self, String> {
        let entries = super::station_shader_pipeline::sampled_entries(&program.interface)?;
        if program.interface.samplers.iter().any(|s| {
            !matches!(
                s.name.as_str(),
                "stationAlbedo" | "stationNormal" | "depthtex0"
            )
        }) {
            return Err("unexpected Photon material input".into());
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Photon material inputs"),
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Photon material encoder"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Photon material vertex"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.vertex)),
        });
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Photon encoding from user ZIP"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.fragment)),
        });
        let pipeline = render_pipeline(
            device,
            Some(&pl),
            &vs,
            &fs,
            "main",
            "main",
            wgpu::TextureFormat::Rgba32Float,
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Photon encoder uniforms"),
            size: program.interface.uniform_size.max(16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            pipeline,
            layout,
            uniform,
            interface: program.interface,
            sampler: device.create_sampler(&Default::default()),
            target: None,
            group: None,
            size: [0; 2],
        })
    }
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        scene: &SceneData,
    ) {
        if size == self.size {
            return;
        }
        // Half floats lose the two independent 8-bit values packed into each channel.
        self.target = Some(
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("Photon colortex1 packed material"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default()),
        );
        let mut data = self.interface.initial_values.clone();
        data.resize(self.interface.uniform_size as usize, 0);
        let target = [
            size[0] as f32,
            size[1] as f32,
            1. / size[0] as f32,
            1. / size[1] as f32,
        ];
        if let Some(u) = self
            .interface
            .uniforms
            .iter()
            .find(|u| u.name == "hz_target")
        {
            let offset = u.offset as usize;
            data[offset..offset + 16].copy_from_slice(bytemuck::cast_slice(&target));
        }
        queue.write_buffer(&self.uniform, 0, &data);
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: self.uniform.as_entire_binding(),
        }];
        for s in &self.interface.samplers {
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding,
                resource: wgpu::BindingResource::TextureView(scene.input(&s.name).unwrap()),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            });
        }
        self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Photon material scene bindings"),
            layout: &self.layout,
            entries: &entries,
        }));
        self.size = size;
    }
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder) {
        run_pass(
            encoder,
            self.view(),
            &self.pipeline,
            self.group.as_ref().unwrap(),
            6,
        );
    }
    pub fn view(&self) -> &wgpu::TextureView {
        self.target.as_ref().unwrap()
    }
}
const VERTEX: &str = "varying vec2 uv; void main(){uv=gl_MultiTexCoord0.xy;gl_Position=vec4(gl_Vertex.xy*2.0-1.0,0.0,1.0);}";
const COLOR: &str = r"
vec3 station_to_srgb(vec3 v){return mix(1.055*pow(max(v,vec3(0.0)),vec3(1.0/2.4))-0.055,12.92*v,lessThanEqual(v,vec3(0.0031308)));}
vec3 station_to_linear(vec3 v){return mix(pow((v+0.055)/1.055,vec3(2.4)),v/12.92,lessThanEqual(v,vec3(0.04045)));}
";
// Calls the pack's real encoding functions; the buffer schema is the Photon 1.4 ABI.
const ENCODE: &str = r"
uniform sampler2D stationAlbedo;
uniform sampler2D stationNormal;
uniform sampler2D depthtex0;
void main(){
 ivec2 p=ivec2(gl_FragCoord.xy);
 if(texelFetch(depthtex0,p,0).r>=1.0){discard;}
 vec3 color=station_to_srgb(texelFetch(stationAlbedo,p,0).rgb);
 vec3 n=texelFetch(stationNormal,p,0).xyz;
 // Station uses Z up; Photon uses Y up. Preserve handedness.
 n=vec3(n.x,n.z,-n.y);
 gl_FragColor=vec4(pack_unorm_2x8(color.rg),pack_unorm_2x8(color.b,0.0),pack_unorm_2x8(encode_unit_vector(n)),pack_unorm_2x8(vec2(0.0)));
}
";
const DECODE: &str = r"
uniform sampler2D colortex1;
uniform sampler2D depthtex0;
varying vec2 uv;
void main(){
 ivec2 p=ivec2(gl_FragCoord.xy);
 float depth=texelFetch(depthtex0,p,0).r;
 if(depth>=1.0){gl_FragColor=vec4(0.0,0.0,0.0,1.0);return;}
 vec4 data=texelFetch(colortex1,p,0);
 vec2 rg=unpack_unorm_2x8(data.x), bm=unpack_unorm_2x8(data.y);
 vec3 n=decode_unit_vector(unpack_unorm_2x8(data.z));
 n=vec3(n.x,-n.z,n.y);
 vec3 color;
 if(uv.x<0.3333)color=pow(station_to_linear(vec3(rg,bm.x)),vec3(1.0/2.2));
 else if(uv.x<0.6667)color=n*0.5+0.5;
 else color=vec3(depth);
 // A wrong ID or light-level bit indicates a packing/precision failure.
 if(bm.y>0.001 || any(greaterThan(unpack_unorm_2x8(data.w),vec2(0.001))))color=vec3(1.0,0.0,1.0);
 gl_FragColor=vec4(color,1.0);
}
";

#[cfg(test)]
mod tests {
    #[test]
    fn does_not_accept_a_non_photon_pack_as_a_material_encoder() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/scene-data");
        let error = super::programs(&path)
            .err()
            .expect("no encoding.glsl in generic fixture");
        assert!(error.contains("Photon encoding"));
    }
}
