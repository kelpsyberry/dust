precision mediump float;

uniform sampler2D framebuffer;

varying vec2 tex_coord;

void main() {
    gl_FragColor = vec4(texture2D(framebuffer, tex_coord).rgb, 1.0);
}
