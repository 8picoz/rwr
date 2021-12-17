#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl Vertex {
    pub fn new(p_x: f32, p_y: f32, p_z: f32, c_x: f32, c_y: f32, c_z: f32, c_a: f32) -> Self {
        Vertex { position: [p_x, p_y, p_z], color: [c_x, c_y, c_z, c_a] }
    }
}