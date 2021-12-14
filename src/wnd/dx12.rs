use windows::{
    core::*, Win32::Foundation::*, Win32::Graphics::Direct3D::Fxc::*, Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D12::*, Win32::Graphics::Dxgi::Common::*, Win32::Graphics::Dxgi::*,
    Win32::System::LibraryLoader::*, Win32::System::Threading::*,
    Win32::System::WindowsProgramming::*, Win32::UI::WindowsAndMessaging::*,
};
pub struct Dx12 {
    width: u32,
    height: u32,
    frame_count: u32,
    device: Option<ID3D12Device>,
    command_queue: Option<ID3D12CommandQueue>,
    dxgi_factory: Option<IDXGIFactory4>,
    swap_chain: Option<IDXGISwapChain3>,
}

impl Dx12 {
    pub fn new(width: u32, height: u32, frame_count: u32) -> Self {
        Dx12 { width, height, frame_count, command_queue: None, device: None, dxgi_factory: None, swap_chain: None }
    }

    //create系はcreate_swapchainがhwndを必要とするので統一性を持たせるためにnew()で呼ばないようにしている

    pub fn create_device(&mut self) -> Result<()> {
        let mut device: Option<ID3D12Device> = None;
        //H/WアダプタをNoneにすることでデフォルトを指定
        unsafe { D3D12CreateDevice( None, D3D_FEATURE_LEVEL_12_0, &mut device) }?;

        self.device = device;

        Ok(())
    }

    pub fn create_factory(&mut self) -> Result<()> {

        let dxgi_factory_flags = if cfg!(debug_assertions) {
            DXGI_CREATE_FACTORY_DEBUG
        } else {
            0
        };

        self.dxgi_factory = Some(unsafe { CreateDXGIFactory2(dxgi_factory_flags) }?);

        Ok(())
    }

    pub fn create_command_queue(&mut self, hwnd: &HWND) -> Result<()> {

        let device = self.device.as_ref().expect("You haven't done initializing a device");

        self.command_queue = Some(unsafe {
            device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })?
        });

        Ok(())
    }

    pub fn create_swap_chain(&mut self, hwnd: &HWND) -> Result<()> {

        let factory = self.dxgi_factory.as_ref().expect("You haven't done initializing a factory");
        let command_queue = self.command_queue.as_ref().expect("You haven't initialzing a command queue");

        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC { 
                Width: self.width, 
                Height: self.height, 
                RefreshRate: DXGI_RATIONAL { Numerator: 60, Denominator: 1 }, 
                Format: DXGI_FORMAT_B8G8R8A8_UNORM, 
                ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, 
                Scaling: DXGI_MODE_SCALING_UNSPECIFIED
            },
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: self.frame_count,
            OutputWindow: *hwnd,
            Windowed: BOOL::from(true),
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH.0 as _,
        };

        self.swap_chain = Some(unsafe {
            factory.CreateSwapChain(command_queue, &swap_chain_desc)?
        }.cast()?);

        Ok(())
    }

    pub fn update(&mut self) {

    }

    pub fn render(&mut self) {
        
    }
}