#version 400 compatibility
// Original station fixture (MIT): real surface attachments, sampled in GL row order.
uniform sampler2D stationAlbedo;
uniform sampler2D stationNormal;
uniform sampler2D depthtex0;
varying vec2 uv;
void main(){
 ivec2 p=ivec2(gl_FragCoord.xy);
 vec4 albedo=texelFetch(stationAlbedo,p,0);
 vec4 normal=texelFetch(stationNormal,p,0);
 float depth=texelFetch(depthtex0,p,0).r;
 vec3 color;
 if(uv.x<0.3333) color=pow(albedo.rgb,vec3(1.0/2.2));
 else if(uv.x<0.6667) color=normal.a>0.0?normal.xyz*0.5+0.5:vec3(0.0);
 else color=depth<1.0?vec3(depth):vec3(0.0);
 gl_FragColor=vec4(color,1.0);
}
