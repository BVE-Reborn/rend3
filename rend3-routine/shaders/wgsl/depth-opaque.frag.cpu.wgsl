fn main_1() {
    return;
}

@fragment 
fn main(@location(0) member: vec4<f32>,
    @builtin(position) @invariant gl_Position: vec4<f32>,
    @location(3) @interpolate(flat) member_1: u32,
    @location(2) member_2: vec4<f32>,
    @location(1) member_3: vec2<f32>) {
    main_1();
}
