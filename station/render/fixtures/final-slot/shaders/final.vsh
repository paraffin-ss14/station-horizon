#version 130
out vec2 texCoord;
void main(){texCoord=gl_MultiTexCoord0.xy;gl_Position=vec4(gl_Vertex.xy*2.0-1.0,0.0,1.0);}
