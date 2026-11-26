#version 130
uniform sampler2D colortex7;
in vec2 texCoord;
void main(){gl_FragColor.rgb=texture2D(colortex7,texCoord).rgb;}
