attribute vec2 coords;

varying vec2 tex_coord;

void main() {
    tex_coord = (vec2(coords.x, -coords.y) + 1.0) * 0.5;
    gl_Position = vec4(coords.x, coords.y, 0.0, 1.0);
}
