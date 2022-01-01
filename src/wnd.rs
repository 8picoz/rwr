mod dx12_rt;
pub(crate) mod descriptor;

use dx12_rt::*;

use windows::{
    core::*, Win32::Foundation::*, Win32::Graphics::Direct3D::Fxc::*, Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D12::*, Win32::Graphics::Dxgi::Common::*, Win32::Graphics::Dxgi::*,
    Win32::System::LibraryLoader::*, Win32::System::Threading::*,
    Win32::System::WindowsProgramming::*, Win32::UI::WindowsAndMessaging::*,
};

use crate::vertex::Vertex;

const TITLE: &str = "rwr";
const CLASSNAME: &str = "rwr";
const CLASSNAMEWITHNULL: &[u8] = b"rwr\0";
const SIZE: (u32, u32) = (640, 480);

//hwndとかライフタイム的にstructに持ってないとダメ？
pub fn run_with_raytracing() -> Result<()> {

    let mut wnd = Wnd::new();

    //DXR
    wnd.check_raytracing_support().unwrap_or_else(|e| panic!("{}", e));
    wnd.init_dxr().unwrap_or_else(|e| panic!("{}", e));

    if cfg!(debug_assertions) {
        println!("initialized");
    }

    message_main_loop(&mut wnd);

    Ok(())
}

    
fn message_main_loop(wnd: &mut Wnd) {
    loop {
        let mut message = MSG::default();

        if unsafe { PeekMessageA(&mut message, None, 0, 0, PM_REMOVE) }.into() {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageA(&message);
            }

            if message.message == WM_QUIT {
                break;
            }
        }
        wnd.dx.render();
    }
}

struct Wnd {
    hwnd: HWND,
    dx: Dx12Rt,
}

impl Wnd {
    pub fn new() -> Self {
        let (hwnd, dx) = Self::init_wnd().expect("Failed init_wnd");

        let mut wnd = Self { hwnd, dx };

        wnd.init_d3d().expect("Failed init d3d");

        if cfg!(debug_assertions) {
            println!("create wnd");
        }

        wnd
    }

    fn init_wnd() -> Result<(HWND, Dx12Rt)> {
        let instance = unsafe { GetModuleHandleA(None) };
    
        let wc = WNDCLASSEXA {
            cbSize: std::mem::size_of::<WNDCLASSEXA>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(Self::wndproc),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) },
            lpszClassName: PSTR(CLASSNAMEWITHNULL.as_ptr() as _),
            ..Default::default()
        };
        
        let mut dx = Dx12Rt::new(SIZE.0, SIZE.1, 2);
    
        let atom = unsafe { RegisterClassExA(&wc) };
        debug_assert_ne!(atom, 0);
    
        let mut window_rect = RECT {
            left: 0,
            top: 0,
            right: SIZE.0 as _,
            bottom: SIZE.1 as _,
        };
        unsafe { AdjustWindowRect(&mut window_rect, WS_OVERLAPPEDWINDOW, false) };
    
        let hwnd = unsafe {
            CreateWindowExA(
                Default::default(),
                CLASSNAME,
                TITLE,
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                window_rect.right - window_rect.left,
                window_rect.bottom - window_rect.top,
                None,
                None,
                instance,
                &mut dx as *mut _ as _,
            )
        };

        
    
        unsafe { ShowWindow(hwnd, SW_SHOW) };
    
        Ok((hwnd, dx))
    }
    
    fn init_d3d(&mut self) -> Result<()> {
    
        if cfg!(debug_assertions) {
            let mut debug: Option<ID3D12Debug> = None;
            unsafe {
                if D3D12GetDebugInterface(&mut debug).is_ok() {
                    println!("enable debug");
                    let debug = debug.as_ref().unwrap();
                    debug.EnableDebugLayer();
                }
            }
        }

        self.dx.create_device()?;
        self.dx.create_factory()?;
        self.dx.create_command_queue()?;
        self.dx.create_command_allocator()?;
        self.dx.create_command_list()?;
        self.dx.create_swap_chain(&self.hwnd)?;
        self.dx.create_fence()?;
    
        Ok(())
    }

    pub fn check_raytracing_support(&self) -> std::result::Result<(), &'static str> {

        let ops = self.dx.chack_dxr_support().expect("Failed to check dxr support");
        if ops.RaytracingTier == D3D12_RAYTRACING_TIER_NOT_SUPPORTED {
            Err("Not supported raytracing")
        } else {
            Ok(())
        }
    }

    pub fn init_dxr(&mut self) -> Result<()> {

        let tri = [
            Vertex::new(-0.5, -0.5, 0.0),
            Vertex::new(0.5, -0.5, 0.0),
            Vertex::new(0.0, 0.75, 0.0),
        ];

        self.dx.create_vertex_buffer(tri)?;
        self.dx.build_blas()?;
        self.dx.build_tlas()?;
        self.dx.create_global_root_signature()?;
        self.dx.create_state_object()?;
        self.dx.create_result_resource()?;
        self.dx.create_shader_table()?;

        Ok(())
    }
    
    //Win32Api
    fn sample_wndproc(sample: &mut Dx12Rt, message: u32, _: WPARAM) -> bool {
        match message {
            WM_PAINT => {
                //sample.update();
                //sample.render();
    
                true
            }
            _ => {
                false
            }
        }
    }
    
    extern "system" fn wndproc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_CREATE => {
                unsafe {
                    let create_struct: &CREATESTRUCTA = std::mem::transmute(lparam);
                    SetWindowLong(window, GWLP_USERDATA, create_struct.lpCreateParams as _);
                }
                LRESULT::default()
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT::default()
            }
            _ => {
                let user_data = unsafe { GetWindowLong(window, GWLP_USERDATA) };
                let sample = std::ptr::NonNull::<Dx12Rt>::new(user_data as _);
                let handled = sample.map_or(false, |mut s| {
                    Self::sample_wndproc(unsafe { s.as_mut() }, message, wparam)
                });
    
                if handled {
                    LRESULT::default()
                } else {
                    unsafe { DefWindowProcA(window, message, wparam, lparam) }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "32")]
unsafe fn SetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
    SetWindowLongA(window, index, value as _) as _
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "64")]
unsafe fn SetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
    SetWindowLongPtrA(window, index, value)
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "32")]
unsafe fn GetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
    GetWindowLongA(window, index) as _
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "64")]
unsafe fn GetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
    GetWindowLongPtrA(window, index)
}