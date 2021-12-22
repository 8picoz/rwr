use std::borrow::Cow;
use std::ffi::c_void;

use windows::{
    core::*, Win32::Foundation::*, Win32::Graphics::Direct3D::Fxc::*, Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D12::*, Win32::Graphics::Dxgi::Common::*, Win32::Graphics::Dxgi::*,
    Win32::System::LibraryLoader::*, Win32::System::Threading::*,
    Win32::System::WindowsProgramming::*, Win32::UI::WindowsAndMessaging::*,
};

use crate::vertex::Vertex;

#[repr(C)]
pub struct Dx12Rt {
    width: u32,
    height: u32,
    frame_count: u32,
    device: Option<ID3D12Device5>,
    command_queue: Option<ID3D12CommandQueue>,
    dxgi_factory: Option<IDXGIFactory4>,
    swap_chain: Option<IDXGISwapChain3>,
    render_targets: Vec<ID3D12Resource>,
    render_target_view_descriptor: Option<ID3D12DescriptorHeap>,
    command_allocator: Option<Vec<ID3D12CommandAllocator>>,
    command_list: Option<Vec<ID3D12GraphicsCommandList4>>,
    frame_index: u32,

    vb: Option<ID3D12Resource>,
    vbv: Option<D3D12_VERTEX_BUFFER_VIEW>,
    vertices_count: u32,
    blas_scratch: Option<ID3D12Resource>,
    blas: Option<ID3D12Resource>,
    tlas_scratch: Option<ID3D12Resource>,
    tlas: Option<ID3D12Resource>,
    global_root_signature: Option<ID3D12RootSignature>,
    state_object: Option<ID3D12StateObject>,

    tlas_descriptor: Option<ID3D12DescriptorHeap>,

    result_buffer: Option<ID3D12Resource>,
    result_resource_descriptor: Option<ID3D12DescriptorHeap>,

    shader_table: Option<ID3D12Resource>,

    dispatch_ray_desc: D3D12_DISPATCH_RAYS_DESC,

    //Fence
    fence: Option<ID3D12Fence>,
    fence_value: u64,
    fence_event: HANDLE,

    //shader symbols
    ray_gen_symbol: Vec<u16>,
    miss_symbol: Vec<u16>,
    closest_hit_symbol: Vec<u16>,
    //hit group
    default_hit_group_symbol: Vec<u16>,

    //shader
    ray_shader_blob: ID3DBlob,
    check: bool
}

impl Dx12Rt {
    pub fn new(width: u32, height: u32, frame_count: u32) -> Self {

        //[TODO]: argsで受け取れるように
        let ray_shader_blob = Self::load_shader("E:\\Projects\\rwr\\ray_shader.cso").expect("Failed to load ray shader");

        Dx12Rt { 
            width, 
            height, 
            frame_count, 
            command_queue: None, 
            device: None,
            dxgi_factory: None, 
            swap_chain: None, 
            render_targets: vec![],
            render_target_view_descriptor: None,
            command_allocator: None, 
            command_list: None,
            frame_index: 0,
            vb: None, 
            vbv: None,
            vertices_count: 0,
            blas_scratch: None,
            blas: None,
            tlas_scratch: None,
            tlas: None,
            global_root_signature: None,
            state_object: None,
            tlas_descriptor: None,
            result_buffer: None,
            result_resource_descriptor: None,
            shader_table: None,
            dispatch_ray_desc: D3D12_DISPATCH_RAYS_DESC::default(),
            fence: None,
            fence_value: 1,
            fence_event: unsafe { CreateEventA(std::ptr::null(), false, false, None) },
            ray_gen_symbol: "MainRayGen\0".encode_utf16().collect(),
            miss_symbol: "MainMiss\0".encode_utf16().collect(),
            closest_hit_symbol: "MainClosestHit\0".encode_utf16().collect(),
            default_hit_group_symbol: "DefaultHitGroup\0".encode_utf16().collect(),
            ray_shader_blob,
            check: false,
        }
    }

    //create系はcreate_swapchainがhwndを必要とするので統一性を持たせるためにnew()で呼ばないようにしている

    pub fn create_device(&mut self) -> Result<()> {
        let mut device: Option<ID3D12Device5> = None;
        //H/WアダプタをNoneにすることでデフォルトを指定
        unsafe { D3D12CreateDevice( 
            None, 
            D3D_FEATURE_LEVEL_12_0, 
            &mut device) }?;

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

    pub fn create_command_queue(&mut self) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");

        self.command_queue = Some(unsafe {
            device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })?
        });

        Ok(())
    }

    pub fn create_swap_chain(&mut self, hwnd: &HWND) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");
        let factory = self.dxgi_factory.as_ref().expect("You have to initialize a factory");
        let command_queue = self.command_queue.as_ref().expect("You have to initialize a command queue");

        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC { 
                Width: self.width, 
                Height: self.height, 
                RefreshRate: DXGI_RATIONAL { Numerator: 60, Denominator: 1 }, 
                Format: DXGI_FORMAT_R8G8B8A8_UNORM, 
                ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, 
                Scaling: DXGI_MODE_SCALING_UNSPECIFIED
            },
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: self.frame_count,
            OutputWindow: *hwnd,
            Windowed: BOOL::from(true),
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH as _,
        };

        self.swap_chain = Some(unsafe {
            factory.CreateSwapChain(command_queue, &swap_chain_desc)?
        }.cast()?);

        let swap_chain = self.swap_chain.as_ref().unwrap();

        let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            NumDescriptors: self.frame_count,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            NodeMask: 0,
        };
        
        let rtv_heap: ID3D12DescriptorHeap = unsafe { 
            device.CreateDescriptorHeap(
                &heap_desc
        )}?;

        let mut handle = unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() };

        let increment_size = unsafe { device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) };

        for i in 0..self.frame_count {
            
            let render_target: ID3D12Resource = unsafe {
                swap_chain.GetBuffer(i)?
            };
            
            let rtv_desc = D3D12_RENDER_TARGET_VIEW_DESC {
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ViewDimension: D3D12_RTV_DIMENSION_TEXTURE2D,
                ..Default::default()
            };
        
            unsafe {
                device.CreateRenderTargetView(&render_target, &rtv_desc as *const _, handle);
            }

            self.render_targets.push(render_target);
            
            handle.ptr += increment_size as usize;
        }
        
        Ok(())
    }
    
    pub fn create_command_allocator(&mut self) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");

        let mut alc: Vec<ID3D12CommandAllocator> = vec![];
        for _ in 0..self.frame_count {
            alc.push(
                unsafe {
                    device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                }?
            );
        }

        self.command_allocator = Some(alc);

        Ok(())
    }

    pub fn create_command_list(&mut self) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");
        let command_allocators = self.command_allocator.as_ref().expect("You have to initialize a command allocator");

        let mut cmd_lists: Vec<ID3D12GraphicsCommandList4> = vec![];
        //コマンドリストはRTVごとに作らなくて良いので後で修正
        for alc in command_allocators {
            cmd_lists.push(
                unsafe {
                    device.CreateCommandList(
                        0, 
                        D3D12_COMMAND_LIST_TYPE_DIRECT, 
                        alc, 
                        //psoは後で設定
                        None,
                    )?
                }
            );
        }

        self.command_list = Some(cmd_lists);

        Ok(())
    }

    pub fn create_fence(&mut self) -> Result<()> {
        
        let device = self.device.as_ref().expect("You have to initialize a device");


        self.fence = unsafe { Some(device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?) };
        
        Ok(())
    }

    pub fn chack_dxr_support(&self) -> Result<D3D12_FEATURE_DATA_D3D12_OPTIONS5> {

        let device = self.device.as_ref().expect("You have to initialize a device");
        
        let mut ops = D3D12_FEATURE_DATA_D3D12_OPTIONS5::default();
        unsafe {
            device.CheckFeatureSupport(
                D3D12_FEATURE_D3D12_OPTIONS5, 
                &mut ops as *mut _ as _, 
                std::mem::size_of::<D3D12_FEATURE_DATA_D3D12_OPTIONS5>() as u32
            )
        }?;

        Ok(ops)
    }

    pub fn create_vertex_buffer<const SIZE: usize>(&mut self, vertices: [Vertex; SIZE]) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");

        let prop = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_UPLOAD,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: std::mem::size_of_val(&vertices) as u64,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };

        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &desc as *const _ as _, 
                D3D12_RESOURCE_STATE_GENERIC_READ, 
                std::ptr::null(), 
                &mut self.vb as *mut _ as _,
            )?;
        };

        let vertex_buffer = self.vb.as_ref().expect("Failed to create committed resource of vertex buffer");

        unsafe {
            let mut data = std::ptr::null_mut();
            
            vertex_buffer.Map(0, std::ptr::null(), &mut data)?;
            std::ptr::copy_nonoverlapping(
                vertices.as_ptr(), 
                data as *mut Vertex, 
                std::mem::size_of_val(&vertices)
            );
            vertex_buffer.Unmap(0, std::ptr::null());
        };

        self.vbv = Some(
            D3D12_VERTEX_BUFFER_VIEW {
                BufferLocation: unsafe { vertex_buffer.GetGPUVirtualAddress() },
                StrideInBytes: std::mem::size_of::<Vertex>() as u32,
                SizeInBytes: std::mem::size_of_val(&vertices) as u32,
            }
        );

        self.vertices_count = SIZE as u32;

        Ok(())
    }

    pub fn build_blas(&mut self) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");
        let vertex_buffer = self.vb.as_ref().expect("You have to initialize a vertex buffer");
        let command_list = &self.command_list.as_ref().expect("You have to initialize a command list")[self.frame_index as usize];
        let queue = self.command_queue.as_ref().expect("You have to initialize a command queue");
        let command_allocator = &self.command_allocator.as_ref().expect("You have to initialize a command allocator")[self.frame_index as usize];
        let fence = self.fence.as_ref().expect("You have to initialize a fence");

        //まずBLASに必要なメモリ量を求める
        let mut geom_desc = unsafe { D3D12_RAYTRACING_GEOMETRY_DESC {
            Type: D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES,
            Flags: D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE,
            Anonymous: D3D12_RAYTRACING_GEOMETRY_DESC_0 {
                //今回は三角形なのでこの構造体を指定
                Triangles: D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
                    VertexBuffer: D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE {
                        StartAddress: vertex_buffer.GetGPUVirtualAddress(),
                        StrideInBytes: std::mem::size_of::<Vertex>() as u64,
                    },
                    VertexFormat: DXGI_FORMAT_R32G32B32_FLOAT,
                    VertexCount: self.vertices_count,
                    ..Default::default()
                },
            },
        }};

        let mut build_as_desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            //このINPUTSはTLASとBLASのどちらにも使われる
            Inputs: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
                Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL,
                DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
                Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_NONE,
                NumDescs: 1,
                Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
                    pGeometryDescs: &mut geom_desc as *mut _ as _
                }
            },
            ..Default::default()
        };

        let inputs = &build_as_desc.Inputs;

        let mut blas_pre_build = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO::default();

        unsafe { 
            //必要なメモリ量を求める
            device.GetRaytracingAccelerationStructurePrebuildInfo(
                inputs as *const _ as _, 
                &mut blas_pre_build as *mut _ as _
            ) 
        };

        //必要なメモリ量を求めたのでBLASのバッファとスクラッチバッファ(UAVアクセス)のバッファ確保

        //スクラッチリソース

        let prop = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        let scratch_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: blas_pre_build.ScratchDataSizeInBytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        };

        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &scratch_desc as *const _ as _, 
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS, 
                std::ptr::null(), 
                &mut self.blas_scratch as *mut _ as _,
            )?;
        }

        let blas_buffer_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: blas_pre_build.ResultDataMaxSizeInBytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        };

        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &blas_buffer_desc as *const _ as _, 
                D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE, 
                std::ptr::null(), 
                &mut self.blas as *mut _ as _,
            )?;
        }

        let blas_scratch = self.blas_scratch.as_ref().expect("Failed to create blas scratch");
        let blas = self.blas.as_ref().expect("Failed to create blas");

        //アクセラレーションストラクチャー構築
        build_as_desc.ScratchAccelerationStructureData = unsafe { blas_scratch.GetGPUVirtualAddress() };
        build_as_desc.DestAccelerationStructureData = unsafe { blas.GetGPUVirtualAddress() };

        //コマンドリストに積んで実行
        //もしかしてCopyしてるから反映されない？

        //BLASを実際にビルド
        //疑問: TLASと一緒にコマンドリストに登録sh知恵ビルドではなく一回リセットを挟んでからでも良いのか？

        unsafe {
            command_list.BuildRaytracingAccelerationStructure(
                &build_as_desc as *const _ as _, 
                0, 
                std::ptr::null(),
            );
        }

        //リソースバリアの設定
        let uav_barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                    pResource: Some(blas.clone()),
                }),
            },
            ..Default::default()
        };

        unsafe {
            command_list.ResourceBarrier(1, &uav_barrier);
            command_list.Close()?;
            queue.ExecuteCommandLists(1, &Some(command_list.cast()?));
        };
        
        //リソースバリア
        self.fence_value = Self::wait_for_gpu(queue, fence, self.fence_value, &self.fence_event)?;
        
        //command_listがレコード状態でResetをかけるとエラーとなるので使った後に必ずResetをかけることで二重Resetを防ぐ
        unsafe { command_list.Reset(command_allocator, None)? };

        Ok(())
    }

    pub fn build_tlas(&mut self) -> Result<()> {
        
        let device = self.device.as_ref().expect("You have to initialize a device");
        let vertex_buffer = self.vb.as_ref().expect("You have to initialize a vertex buffer");
        let command_list = &self.command_list.as_ref().expect("You have to initialize a command list")[self.frame_index as usize];
        let command_allocator = &self.command_allocator.as_ref().expect("You have to initialize a command allocator")[self.frame_index as usize];
        let queue = self.command_queue.as_ref().expect("You have to initialize a command queue");
        let fence = self.fence.as_ref().expect("You have to initialize a fence");

        let blas = self.blas.as_ref().expect("You have to build a blas");

        //ここからTLAS
    
        //instance_descの生成

        //https://docs.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_raytracing_instance_desc
        /*
        _bitfield1と_bitfield2は上位24bitと下位8bitでそれぞれ分かれている？
        */
        let instance_desc = D3D12_RAYTRACING_INSTANCE_DESC {
            //単位行列
            Transform: [1.0, 0.0, 0.0, 0.0,
                        0.0, 1.0, 0.0, 0.0,
                        0.0, 0.0, 1.0, 0.0],
            _bitfield1: 0x0000_00FF, //0x0000_0000 + 0xFF
            _bitfield2: D3D12_RAYTRACING_INSTANCE_FLAG_NONE, //0x0000_0000 + D3D12_RAYTRACING_INSTANCE_FLAG_NONE.0
            AccelerationStructure: unsafe { blas.GetGPUVirtualAddress() }
        };

        let prop = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_UPLOAD,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: std::mem::size_of_val(&instance_desc) as u64,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            //ここのFlagで悩んだ
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };

        let mut instance_desc_buffer: Option<ID3D12Resource> = None;
        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &desc as *const _ as _, 
                D3D12_RESOURCE_STATE_GENERIC_READ, 
                std::ptr::null(), 
                &mut instance_desc_buffer as *mut _ as _
            )?;
        }

        unsafe {
            let mut data = std::ptr::null_mut();
            
            vertex_buffer.Map(0, std::ptr::null(), &mut data)?;
            std::ptr::copy_nonoverlapping(
                &instance_desc as *const _ as _, 
                data as *mut _ as _, 
                1
            );
            vertex_buffer.Unmap(0, std::ptr::null());
        };
        

        let mut build_as_desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            Inputs: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
                Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL,
                DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
                Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_NONE,
                NumDescs: 1,
                ..Default::default()
            },
            ..Default::default()
        };

        let inputs = &build_as_desc.Inputs;

        let mut tlas_pre_build = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO::default();

        unsafe {
            device.GetRaytracingAccelerationStructurePrebuildInfo(
                inputs as *const _ as _, 
                &mut tlas_pre_build as *mut _ as _,
            );
        }

        //tlas scratch

        let prop = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        let scratch_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: tlas_pre_build.ScratchDataSizeInBytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        };

        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &scratch_desc as *const _ as _,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS, 
                std::ptr::null(), 
                &mut self.tlas_scratch as *mut _ as _,
            )?;
        }

        let tlas_buffer_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: tlas_pre_build.ResultDataMaxSizeInBytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        };

        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &tlas_buffer_desc as *const _ as _, 
                D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE, 
                std::ptr::null(), 
                &mut self.tlas as *mut _ as _,
            )?;
        }

        let instance_desc_buffer = instance_desc_buffer.expect("Failed to create instance desc buffer");
        let tlas_scratch = self.tlas_scratch.as_ref().expect("Failed to create tlas scratch");
        let tlas = self.tlas.as_ref().expect("Failed to create tlas");

        build_as_desc.Inputs.Anonymous.InstanceDescs = unsafe { instance_desc_buffer.GetGPUVirtualAddress() };
        build_as_desc.ScratchAccelerationStructureData = unsafe { tlas_scratch.GetGPUVirtualAddress() };
        build_as_desc.DestAccelerationStructureData = unsafe { tlas.GetGPUVirtualAddress() };

        unsafe {
            command_list.BuildRaytracingAccelerationStructure(
                &build_as_desc, 
                0, 
                std::ptr::null(),
            );
        }

        let uav_barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                    pResource: Some(tlas.clone()),
                }),
            },
            ..Default::default()
        };

        unsafe {
            command_list.ResourceBarrier(1, &uav_barrier);
            command_list.Close()?;
            queue.ExecuteCommandLists(1, &Some(command_list.cast()?));
        };
        
        self.fence_value = Self::wait_for_gpu(queue, fence, self.fence_value, &self.fence_event)?;

        //D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAVの確保
        let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            NumDescriptors: 1,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
            NodeMask: 0,
        };

        self.tlas_descriptor = Some(unsafe {
            device.CreateDescriptorHeap(&heap_desc)?
        });

        let heap = self.tlas_descriptor.as_ref().unwrap();

        let srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            ViewDimension: D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                RaytracingAccelerationStructure: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_SRV {
                    Location: unsafe { tlas.GetGPUVirtualAddress() },
                },
            },
            ..Default::default()
        };

        unsafe {
            device.CreateShaderResourceView(
                None, 
                &srv_desc as *const _ as _, 
                heap.GetCPUDescriptorHandleForHeapStart(),
            );
        }

        unsafe { command_list.Reset(command_allocator, None)? };

        Ok(())
    }

    pub fn create_global_root_signature(&mut self) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");

        //TLASをt0レジスタに割り当てる用の設定
        let mut desc_range_tlas = D3D12_DESCRIPTOR_RANGE {
            BaseShaderRegister: 0, //t0レジスタ
            NumDescriptors: 1,
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            ..Default::default()
        };

        //UAVの出力バッファをu0レジスタに割り当てる用の設定
        let mut desc_range_output = D3D12_DESCRIPTOR_RANGE {
            BaseShaderRegister: 0, //u0レジスタ
            NumDescriptors: 1,
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_UAV,
            ..Default::default()
        };

        let mut root_params = [
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &mut desc_range_tlas as *mut _ as _,
                    }
                },
                ..Default::default()
            }, 
            D3D12_ROOT_PARAMETER {
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &mut desc_range_output as *mut _ as _,
                    }
                },
                ..Default::default()
            },
        ];


        let root_sig_desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: root_params.len() as u32,
            pParameters: &mut root_params as *mut _ as _,
            ..Default::default()
        };
        
        unsafe {
            let mut blob: Option<ID3DBlob> = Some(D3DCreateBlob(2048)?);
            let mut err_blob: Option<ID3DBlob> = Some(D3DCreateBlob(2048)?);

            D3D12SerializeRootSignature(
                &root_sig_desc, 
                D3D_ROOT_SIGNATURE_VERSION_1, 
                &mut blob, 
                &mut err_blob
            )?;

            let blob: ID3DBlob = blob.unwrap();
            //let err_blob = err_blob.unwrap();

            let root_sig: ID3D12RootSignature = device.CreateRootSignature(
                0,
                blob.GetBufferPointer(),
                blob.GetBufferSize()
            )?;
    
            root_sig.SetName("global_root_signature")?;
            
            self.global_root_signature = Some(root_sig);
        }

        Ok(())
    }

    pub fn create_state_object(&mut self) -> Result<()> {

        let device = self.device.as_ref().expect("You have to initialize a device");

        //global root sigantureの生成とメソッドを分けるべきか？

        let mut global_root_signature = D3D12_GLOBAL_ROOT_SIGNATURE {
            pGlobalRootSignature: self.global_root_signature.clone(),
        };

        let mut exports = [
            D3D12_EXPORT_DESC {
                Name: PWSTR(self.ray_gen_symbol.as_mut_ptr()),
                Flags: D3D12_EXPORT_FLAG_NONE,
                ..Default::default()
            },
            D3D12_EXPORT_DESC {
                Name: PWSTR(self.miss_symbol.as_mut_ptr()),
                Flags: D3D12_EXPORT_FLAG_NONE,
                ..Default::default()
            },
            D3D12_EXPORT_DESC {
                Name: PWSTR(self.closest_hit_symbol.as_mut_ptr()),
                Flags: D3D12_EXPORT_FLAG_NONE,
                ..Default::default()
            },
        ];
        
        let mut hit_group_desc = D3D12_HIT_GROUP_DESC {
            Type: D3D12_HIT_GROUP_TYPE_TRIANGLES,
            ClosestHitShaderImport: PWSTR(self.closest_hit_symbol.as_mut_ptr()),
            HitGroupExport: PWSTR(self.default_hit_group_symbol.as_mut_ptr()),
            ..Default::default()
        };

        let mut dxil_lib_desc = D3D12_DXIL_LIBRARY_DESC {
            DXILLibrary: D3D12_SHADER_BYTECODE {
                pShaderBytecode: unsafe { self.ray_shader_blob.GetBufferPointer() },
                BytecodeLength: unsafe { self.ray_shader_blob.GetBufferSize() },
            },
            //pExportにアクセスしようとしない限り
            NumExports: exports.len() as u32,
            pExports: exports.as_mut_ptr(),
        };

        let mut shader_config = D3D12_RAYTRACING_SHADER_CONFIG {
            MaxPayloadSizeInBytes: std::mem::size_of::<[f32; 3]>() as u32,
            MaxAttributeSizeInBytes: std::mem::size_of::<[f32; 2]>() as u32,
        };

        let mut pipeline_config = D3D12_RAYTRACING_PIPELINE_CONFIG {
            MaxTraceRecursionDepth: 1,
        };

        let mut sub_objs = vec![

            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_GLOBAL_ROOT_SIGNATURE,
                pDesc: &mut global_root_signature as *mut _ as _,
            },
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_RAYTRACING_SHADER_CONFIG,
                pDesc: &mut shader_config as *mut _ as _,
            },
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_RAYTRACING_PIPELINE_CONFIG,
                pDesc: &mut pipeline_config as *mut _ as _,
            },
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_DXIL_LIBRARY,
                pDesc: &mut dxil_lib_desc as *mut _ as _,
            },
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_HIT_GROUP,
                pDesc: &mut hit_group_desc as *mut _ as _,
            },

        ];

        let state_obj_desc = D3D12_STATE_OBJECT_DESC {
            Type: D3D12_STATE_OBJECT_TYPE_RAYTRACING_PIPELINE,
            NumSubobjects: sub_objs.len() as u32,
            pSubobjects: sub_objs.as_mut_ptr(),
        };

        self.state_object = Some(unsafe {
            device.CreateStateObject(
                &state_obj_desc
            )?
        });

        Ok(())
    }

    pub fn create_result_resource(&mut self) -> Result<()> {
        
        let device = self.device.as_ref().expect("You have to initialize a device");

        let output_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: self.width as u64,
            Height: self.height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 1,
            },
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        };

        let prop = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _,
                D3D12_HEAP_FLAG_NONE,
                &output_desc as *const _ as _,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                std::ptr::null(),
                &mut self.result_buffer as *mut _ as _,
            )?;
        };

        let output_buffer = self.result_buffer.as_ref().unwrap().clone();

        let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            NumDescriptors: 1,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
            NodeMask: 0,
        };

        self.result_resource_descriptor = Some(unsafe {
            device.CreateDescriptorHeap(&heap_desc)?
        });

        let output_view_heap = self.result_resource_descriptor.as_ref().unwrap();

        let uav_desc = D3D12_UNORDERED_ACCESS_VIEW_DESC {
            ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
            ..Default::default()
        };

        unsafe {
            device.CreateUnorderedAccessView(
                output_buffer, 
                None, 
                &uav_desc as *const _ as _, 
                output_view_heap.GetCPUDescriptorHandleForHeapStart()
            )
        }

        Ok(())
    }

    pub fn create_shader_table(&mut self) -> Result<()> {
        
        let device = self.device.as_ref().expect("You have to initialize a device");
        let state_object = self.state_object.as_ref().expect("You have to initialize a device");

        //レコードはシェーダーテーブルのそれぞれの要素のこと
        let record_size = D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES;
        let record_size = (record_size + D3D12_RAYTRACING_SHADER_RECORD_BYTE_ALIGNMENT - 1) & !(D3D12_RAYTRACING_SHADER_RECORD_BYTE_ALIGNMENT - 1);

        let ray_gen_size = 1 * record_size; //ray gen shaderは一つ
        let miss_size = 1 * record_size; //miss shaderは一つ
        let hit_group_size = 1 * record_size; //hit groupは一つ

        //アライメント調整
        let table_align = D3D12_RAYTRACING_SHADER_TABLE_BYTE_ALIGNMENT;
        let ray_gen_region = ((ray_gen_size + table_align - 1) & !(table_align - 1)) as usize;
        let miss_region = ((miss_size + table_align - 1) & !(table_align - 1)) as usize;
        let hit_group_region = ((hit_group_size + table_align - 1) & !(table_align - 1)) as usize;

        //シェーダーテーブル生成
        let table_size = ray_gen_region + miss_region + hit_group_region;
        
        let prop = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_UPLOAD,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: table_size as u64,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            //ここのFlagで悩んだ
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };

        unsafe {
            device.CreateCommittedResource(
                &prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &desc as *const _ as _, 
                D3D12_RESOURCE_STATE_GENERIC_READ, 
                std::ptr::null(), 
                &mut self.shader_table as *mut _ as _
            )?;
        }

        let rtso_props: ID3D12StateObjectProperties = state_object.cast()?;
        
        unsafe {
            let shader_table = self.shader_table.as_ref().unwrap();

            let mut data = std::ptr::null_mut();
            shader_table.Map(0, std::ptr::null(), &mut data)?;

            //Raygenシェーダー
            let ray_gen_shader_p = data;
            let id = rtso_props.GetShaderIdentifier(PWSTR(self.ray_gen_symbol.as_mut_ptr()));

            std::ptr::copy_nonoverlapping(
                id, 
                ray_gen_shader_p, 
                D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES as usize,
            );

            //Raygenシェーダーにローカルルートシグニチャが存在するならこのあとに書く
            //let ray_gen_shader_p = ray_gen_shader_p.add(D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES as usize);

            let miss_shader_p = data.add(ray_gen_region);
            let id = rtso_props.GetShaderIdentifier(PWSTR(self.miss_symbol.as_mut_ptr()));

            std::ptr::copy_nonoverlapping(
                id, 
                miss_shader_p, 
                D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES as usize,
            );

            //Missシェーダーにローカルルートシグニチャが存在するならこの後に書く
            //let miss_shader_p = miss_shader_p.add((D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES as usize);

            let hit_group_p = data.add(ray_gen_region).add(miss_region);
            let id = rtso_props.GetShaderIdentifier(PWSTR(self.default_hit_group_symbol.as_mut_ptr()));

            std::ptr::copy_nonoverlapping(
                id, 
                hit_group_p,
                D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES as usize,
            );

            //HitGroupにローカルルートシグニチャが存在するならこの後に書く
            //let hit_group_p = hit_group_p.add(D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES as usize);

            shader_table.Unmap(0, std::ptr::null());

            //RayGenerationShaderRecordの設定
            let start_address = shader_table.GetGPUVirtualAddress();
            self.dispatch_ray_desc.RayGenerationShaderRecord = D3D12_GPU_VIRTUAL_ADDRESS_RANGE {
                StartAddress: start_address,
                SizeInBytes: ray_gen_size as u64,
            };
            
            let start_address = start_address + ray_gen_region as u64;
            self.dispatch_ray_desc.MissShaderTable = D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE {
                StartAddress: start_address,
                SizeInBytes: miss_size as u64,
                StrideInBytes: record_size as u64,
            };

            let start_address = start_address + miss_region as u64;
            self.dispatch_ray_desc.HitGroupTable = D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE {
                StartAddress: start_address,
                SizeInBytes: hit_group_size as u64,
                StrideInBytes: record_size as u64,
            };
            
            //let start_address = start_address + hit_group_region as u64;

            self.dispatch_ray_desc.Width = self.width;
            self.dispatch_ray_desc.Height = self.height;
            self.dispatch_ray_desc.Depth = 1;
        };
        
        Ok(())
    }

    fn wait_for_gpu(queue: &ID3D12CommandQueue, fence: &ID3D12Fence, fence_value: u64, fence_event: &HANDLE) -> Result<u64> {

        /*
        Fenceに設定された初期値は0でqueueを通してシグナルを送るとそのqueueのコマンドがGPU上で実行が完了していたときに第２引数の値に更新する
        Fenceには常に前に呼び出されたときの値が入っているため" + 1"して更新してあげると良い感じに待てる
        fenceの中に入ってる値の初期値は0
        */

        unsafe {
            queue.Signal(fence, fence_value)
        }
        .ok()
        //Optionをunwrapしなければいけない？
        .unwrap();

        if unsafe { fence.GetCompletedValue() } < fence_value {
            unsafe {
                fence.SetEventOnCompletion(fence_value, fence_event)
            }
            .ok()
            .unwrap();

            unsafe { WaitForSingleObject(fence_event, INFINITE) };
        }

        Ok(fence_value + 1)
    }

    fn load_shader<'a>(path: impl Into<Cow<'a, str>>) -> Result<ID3DBlob> {
        let path: &str = &path.into();

        Ok(unsafe {
            D3DReadFileToBlob(path)?
        })
    }

    pub fn update(&mut self) {

    }

    pub fn render(&mut self) {
        if cfg!(debug_assertions) {
            println!("render");
        }

        let device = self.device.as_ref().expect("You have to initialize a device");
        let command_list = &self.command_list.as_ref().expect("You have to initialize a command list")[self.frame_index as usize];
        let command_queue = self.command_queue.as_ref().expect("You have to initialize a command queue");
        let global_root_signature = self.global_root_signature.as_ref().expect("You have ot initialize a global root signature");
        let tlas = self.tlas.as_ref().expect("You have to initialize a tlas");
        let result_resource_descriptor = self.result_resource_descriptor.as_ref().expect("You have to initialize a result resource discriptor");
        let state_object = self.state_object.as_ref().expect("You have to initialize a state object");
        let result_buffer = self.result_buffer.as_ref().expect("You have to initialize a result buffer");
        let render_target = &self.render_targets[self.frame_index as usize];
        
        unsafe {

            let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: 1,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                NodeMask: 0,
            };
            
            let descriptor_heaps: [Option<ID3D12DescriptorHeap>; 1] = [
                Some(device.CreateDescriptorHeap(&heap_desc as *const _ as _).unwrap()),
            ];

            //ルートシグニチャとリソースをセット
            command_list.SetComputeRootSignature(global_root_signature);
            command_list.SetDescriptorHeaps(descriptor_heaps.len() as u32, &descriptor_heaps as *const _);
            command_list.SetComputeRootShaderResourceView(0, tlas.GetGPUVirtualAddress());
            command_list.SetComputeRootDescriptorTable(1, result_resource_descriptor.GetGPUDescriptorHandleForHeapStart());
            
            //バリア設定
            let barrier_to_uav = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        //Cloneでも大丈夫か
                        pResource: self.result_buffer.clone(),
                        StateBefore: D3D12_RESOURCE_STATE_COPY_SOURCE,
                        StateAfter: D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    }),
                }
            };

            command_list.ResourceBarrier(1, &barrier_to_uav);

            //レイトレ
            command_list.SetPipelineState1(state_object);
            command_list.DispatchRays(&self.dispatch_ray_desc);

            let barriers = [
                D3D12_RESOURCE_BARRIER {
                    Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                    Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                    Anonymous: D3D12_RESOURCE_BARRIER_0 {
                        Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                            //Cloneでも大丈夫か
                            pResource: self.result_buffer.clone(),
                            StateBefore: D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                            StateAfter: D3D12_RESOURCE_STATE_COPY_SOURCE,
                            Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        }),
                    }
                }, D3D12_RESOURCE_BARRIER {
                    Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                    Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                    Anonymous: D3D12_RESOURCE_BARRIER_0 {
                        Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                            //Cloneでも大丈夫か
                            pResource: Some(render_target.clone()),
                            StateBefore: D3D12_RESOURCE_STATE_PRESENT,
                            StateAfter: D3D12_RESOURCE_STATE_COPY_DEST,
                            Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        }),
                    }
                },
            ];
            command_list.ResourceBarrier(barriers.len() as u32, &barriers as *const _);
            command_list.CopyResource(render_target, result_buffer);

            let barrier_to_present = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        //Cloneでも大丈夫か
                        pResource: Some(render_target.clone()),
                        StateBefore: D3D12_RESOURCE_STATE_COPY_DEST,
                        StateAfter: D3D12_RESOURCE_STATE_PRESENT,
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    }),
                }
            };
            command_list.ResourceBarrier(1, &barrier_to_present);
            command_list.Close().unwrap();

            let command_list: ID3D12CommandList = command_list.cast().unwrap();
            let command_list = Some(command_list);

            command_queue.ExecuteCommandLists(1, &command_list as *const _);

            self.present(1);
        }
    }

    fn present(&mut self, interval: u32) {

        let swap_chain = self.swap_chain.as_ref().unwrap();
        let command_queue = self.command_queue.as_ref().unwrap();
        let fence = self.fence.as_ref().unwrap();
        unsafe { 
            swap_chain.Present(interval, 0).unwrap();

            command_queue.Signal(fence, self.fence_value).unwrap();

            self.fence_value = Self::wait_for_gpu(command_queue, fence, self.fence_value, &self.fence_event).unwrap();

            self.frame_index = swap_chain.GetCurrentBackBufferIndex();
        }

    }
}