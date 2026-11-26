struct Params { bounds:vec4f, observer:vec4f, grid:vec4i }
struct Lamp { position:vec4f, color:vec4f }
@group(0) @binding(0) var outputField:texture_storage_2d<rgba32float,write>;
@group(0) @binding(1) var<uniform> params:Params;
@group(0) @binding(2) var<storage,read> walls:array<u32>;
@group(0) @binding(3) var<storage,read> lamps:array<Lamp>;
fn solid(p:vec2i,mask:u32)->bool {
 let local=p-params.grid.xy;
 if any(local<vec2i(0)) || any(local>=params.grid.zw) {return true;}
 return (walls[u32(local.y*params.grid.z+local.x)]&mask)!=0u;
}
fn visible(source:vec2f,to:vec2f,mask:u32)->bool {
 var cell=vec2i(floor(source)); let end=vec2i(floor(to));
 if all(cell==end) {return true;} if solid(cell,mask) {return false;}
 let delta=to-source; let step=vec2i(sign(delta));
 let dt=1./max(abs(delta),vec2f(0.0000001));
 var t=select((source-vec2f(cell))*dt,(vec2f(cell+vec2i(1))-source)*dt,step>vec2i(0));
 t=select(t,vec2f(1e20),step==vec2i(0));
 for(var i=0;i<2048;i++) {
  if min(t.x,t.y)>=0.999999 {return true;}
  if abs(t.x-t.y)<0.000001 {
   if solid(cell+vec2i(step.x,0),mask) || solid(cell+vec2i(0,step.y),mask) {return false;}
   cell+=step; t+=dt;
  } else if t.x<t.y {cell.x+=step.x; t.x+=dt.x;}
  else {cell.y+=step.y; t.y+=dt.y;}
  if all(cell==end) {return true;} if solid(cell,mask) {return false;}
 }
 return false;
}
// Resolve lighting at an exposed face instead of tracing into solid volume.
fn surfaceVisible(source:vec2f,to:vec2f,mask:u32)->bool {
 let cell=vec2i(floor(to));
 if !solid(cell,mask) {return visible(source,to,mask);}
 for(var axis=0;axis<2;axis++) {
  let low=f32(cell[axis]);
  var side=0;
  if source[axis]<low {side=-1;} else if source[axis]>low+1. {side=1;} else {continue;}
  var adjacent=cell; adjacent[axis]+=side;
  if solid(adjacent,mask) {continue;}
  var face=to; face[axis]=low+select(-0.001,1.001,side>0);
  if visible(source,face,mask) {return true;}
 }
 return false;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u) {
 let size=textureDimensions(outputField);
 if any(id.xy>=size) {return;}
 let p=params.bounds.xy+(vec2f(id.xy)+vec2f(0.5))/vec2f(size)*params.bounds.z;
 var rgb=vec3f(0.12);
 var sight=1.;
 if params.observer.z>0. {
  sight=0.;
  let quarter=params.bounds.z/vec2f(size)*0.25;
  for(var sy=0;sy<2;sy++) {for(var sx=0;sx<2;sx++) {
   let offset=vec2f(f32(sx)*2.-1.,f32(sy)*2.-1.)*quarter;
   sight+=select(0.,0.25,surfaceVisible(params.observer.xy,p+offset,1u));
  }}
 }
 {
  for(var i=0u;i<u32(params.observer.w);i++) {
   let lamp=lamps[i]; let d=dot(p-lamp.position.xy,p-lamp.position.xy);
   if d>=lamp.position.z*lamp.position.z {continue;}
   if !surfaceVisible(lamp.position.xy,p,2u) {continue;}
   let s=clamp((d+1.)/(lamp.position.z*lamp.position.z),0.,1.);
   rgb+=lamp.color.rgb*lamp.position.w*(1.-s)*(1.-s)/(1.+s);
  }
 }
 textureStore(outputField,vec2i(id.xy),vec4f(rgb,sight));
}
