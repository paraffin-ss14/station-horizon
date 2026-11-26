@group(0) @binding(0) var sprite:texture_2d<f32>;
@group(0) @binding(1) var nearest:sampler;
@group(1) @binding(0) var<uniform> camera:mat4x4f;
@group(2) @binding(0) var lightField:texture_2d<f32>;
@group(2) @binding(1) var<uniform> lightBounds:vec4f;
struct V { @builtin(position) p:vec4f, @location(0) color:vec4f, @location(1) uv:vec2f, @location(2) world:vec3f, @location(3) mask:vec2f, @location(4) unshaded:f32 }
@vertex fn vs(@location(0) p:vec3f,@location(1) color:vec4f,@location(2) uv:vec2f,@location(3) mask:vec2f,@location(4) unshaded:f32)->V {return V(camera*vec4f(p,1.),color,uv,p,mask,unshaded);}
fn albedo(v:V)->vec4f {
 var color=v.color;
 if v.uv.x>=0. {
   color=textureSampleLevel(sprite,nearest,v.uv,0.)*v.color;
   if v.mask.x>=0. {color.a*=textureSampleLevel(sprite,nearest,v.mask,0.).a;}
   if color.a<0.01 {discard;}
 }
 return color;
}
fn illuminate(v:V, input:vec4f)->vec4f {
 var color=input;
 if lightBounds.w>0. {
   let uv=(v.world.xy-lightBounds.xy)/lightBounds.z;
   let dimensions=vec2i(textureDimensions(lightField));
   let samplePos=uv*vec2f(dimensions)-vec2f(0.5);
   let base=vec2i(floor(samplePos));
   let f=fract(samplePos);
   let wx=cubic(f.x); let wy=cubic(f.y);
   var light=vec3f(0.);
   // Filter illumination independently; never blur darkness into the lit room.
   for(var y=0;y<4;y++) { for(var x=0;x<4;x++) {
     let coord=clamp(base+vec2i(x-1,y-1),vec2i(0),dimensions-vec2i(1));
     light+=textureLoad(lightField,coord,0).rgb*wx[x]*wy[y];
   }}
   if v.unshaded<0.5 {
     let linear=color.rgb*light;
     // SDR output; preserve HDR energy until the tone curve.
     color=vec4f(linear/(vec3f(1.)+linear),color.a);
   }
   // Bilinear reconstruction locates the sight contour; a screen-pixel-wide
   // transition antialiases it without giving unseen rooms a soft glow.
   let a=textureLoad(lightField,clamp(base,vec2i(0),dimensions-vec2i(1)),0).a;
   let b=textureLoad(lightField,clamp(base+vec2i(1,0),vec2i(0),dimensions-vec2i(1)),0).a;
   let c=textureLoad(lightField,clamp(base+vec2i(0,1),vec2i(0),dimensions-vec2i(1)),0).a;
   let d=textureLoad(lightField,clamp(base+vec2i(1,1),vec2i(0),dimensions-vec2i(1)),0).a;
   let sight=mix(mix(a,b,f.x),mix(c,d,f.x),f.y);
   let edge=max(fwidth(sight)*0.5,0.001);
   color=vec4f(color.rgb*smoothstep(0.5-edge,0.5+edge,sight),color.a);
 }
 return color;
}

fn cubic(t:f32)->vec4f {
 let t2=t*t; let t3=t2*t;
 return vec4f((1.-t)*(1.-t)*(1.-t),3.*t3-6.*t2+4.,-3.*t3+3.*t2+3.*t+1.,t3)/6.;
}

@fragment fn fs(v:V)->@location(0) vec4f {return illuminate(v,albedo(v));}
struct Surface {
 @location(0) color:vec4f,
 @location(1) albedo:vec4f,
 @location(2) normal:vec4f,
 @location(3) depth:f32,
}
@fragment fn fs_surface(v:V)->Surface {
 // Derivatives precede alpha-test discard, so partially covered quads remain valid.
 let n=cross(dpdy(v.world),dpdx(v.world));
 let normal=n*inverseSqrt(max(dot(n,n),1e-20));
 let base=albedo(v);
 return Surface(illuminate(v,base),base,vec4f(normal,1.),v.p.z);
}
