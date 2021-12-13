use windows::{
    core::*, Win32::Foundation::*,
};
pub struct Dx12 {

}

impl Dx12 {
    pub fn new() -> Self {
        Dx12 {}
    }

    pub fn bind_to_window(&mut self, hwnd: &HWND) -> Result<()> {

        Ok(())
    }

    pub fn update(&mut self) {

    }

    pub fn render(&mut self) {
        
    }
}