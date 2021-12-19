#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
}

impl Vertex {
    pub fn new(p_x: f32, p_y: f32, p_z: f32) -> Self {
        Vertex { position: [p_x, p_y, p_z] }
    }
}