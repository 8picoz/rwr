use std::{borrow::Cow, ffi::c_void};

use windows::{
    core::*, Win32::Foundation::*, Win32::Graphics::Direct3D::Fxc::*, Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D12::*, Win32::Graphics::Dxgi::Common::*, Win32::Graphics::Dxgi::*,
    Win32::System::LibraryLoader::*, Win32::System::Threading::*,
    Win32::System::WindowsProgramming::*, Win32::UI::WindowsAndMessaging::*,
};

use crate::vertex::Vertex;
pub struct Dx12 {
    width: u32,
    height: u32,
    frame_count: u32,
    device: Option<ID3D12Device>,
    command_queue: Option<ID3D12CommandQueue>,
    dxgi_factory: Option<IDXGIFactory4>,
    swap_chain: Option<IDXGISwapChain3>,
    command_allocator: Option<Vec<ID3D12CommandAllocator>>,
    command_list: Option<Vec<ID3D12GraphicsCommandList>>,
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

    heap: Option<ID3D12DescriptorHeap>,

    //Fence
    fence: Option<ID3D12Fence>,
    fence_value: u64,
    fence_event: HANDLE,

    //shader symbols
    ray_gen_symbol: &'static str,
    miss_symbol: &'static str,
    closest_hit_symbol: &'static str,

    //hit group
    default_hit_group_symbol: &'static str,

    //shader
    ray_shader_blob: ID3DBlob,
}

impl Dx12 {
    pub fn new(width: u32, height: u32, frame_count: u32) -> Self {

        //[TODO]: argsで受け取れるように
        let ray_shader_blob = Self::load_shader("./ray_shader.cso").expect("Failed to load ray shader");

        Dx12 { 
            width, 
            height, 
            frame_count, 
            command_queue: None, 
            device: None,
            dxgi_factory: None, 
            swap_chain: None, 
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
            heap: None,
            fence: None,
            fence_value: 1,
            fence_event: unsafe { CreateEventA(std::ptr::null(), false, false, None) },
            ray_gen_symbol: "MainRayGen",
            miss_symbol: "MainMiss",
            closest_hit_symbol: "MainClosestHit",
            default_hit_group_symbol: "DefaultHitGroup",
            ray_shader_blob,
        }
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

        let factory = self.dxgi_factory.as_ref().expect("You have to initialize a factory");
        let command_queue = self.command_queue.as_ref().expect("You have to initialize a command queue");

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

        let mut cmd_lists: Vec<ID3D12GraphicsCommandList> = vec![];
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
        
        let mut ops = D3D12_FEATURE_DATA_D3D12_OPTIONS5 {
            ..Default::default()
        };
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

        let heap_prop = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_UPLOAD,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        let resource_desc = D3D12_RESOURCE_DESC {
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
                &heap_prop as *const _ as _, 
                D3D12_HEAP_FLAG_NONE, 
                &resource_desc as *const _ as _, 
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

        let mut blas_pre_build = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO {
            ..Default::default()
        };

        let device5: ID3D12Device5 = device.cast()?;

        unsafe { 
            //必要なメモリ量を求める
            device5.GetRaytracingAccelerationStructurePrebuildInfo(
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
        let command_list4: ID3D12GraphicsCommandList4 = command_list.cast()?;
        //もしかしてCopyしてるから反映されない？

        //BLASを実際にビルド
        //疑問: TLASと一緒にコマンドリストに登録sh知恵ビルドではなく一回リセットを挟んでからでも良いのか？

        unsafe {
            command_list4.BuildRaytracingAccelerationStructure(
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
            command_list4.ResourceBarrier(1, &uav_barrier);
            command_list4.Close()?;
            let cmd_lists = ID3D12CommandList::from(&command_list4);
            queue.ExecuteCommandLists(1, &Some(cmd_lists));
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
            _bitfield2: D3D12_RAYTRACING_INSTANCE_FLAG_NONE.0, //0x0000_0000 + D3D12_RAYTRACING_INSTANCE_FLAG_NONE.0
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

        let mut tlas_pre_build = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO {
            ..Default::default()
        };

        let device5: ID3D12Device5 = device.cast()?;

        unsafe {
            device5.GetRaytracingAccelerationStructurePrebuildInfo(
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

        let command_list4: ID3D12GraphicsCommandList4 = command_list.cast()?;

        unsafe {
            command_list4.BuildRaytracingAccelerationStructure(
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
            command_list4.ResourceBarrier(1, &uav_barrier);
            command_list4.Close()?;
            let cmd_lists = ID3D12CommandList::from(&command_list4);
            queue.ExecuteCommandLists(1, &Some(cmd_lists));
        };
        
        self.fence_value = Self::wait_for_gpu(queue, fence, self.fence_value, &self.fence_event)?;

        //D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAVの確保
        let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            NumDescriptors: 1024,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
            NodeMask: 0,
        };

        self.heap = Some(unsafe {
            device5.CreateDescriptorHeap(&heap_desc)?
        });

        let heap = self.heap.as_ref().unwrap();

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

        let device: ID3D12Device5 = self.device.as_ref().expect("You have to initialize a device").cast()?;

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
            let mut blob: Option<ID3DBlob> = Some(D3DCreateBlob(1024)?);
            let mut err_blob: Option<ID3DBlob> = Some(D3DCreateBlob(1024)?);

            D3D12SerializeRootSignature(
                &root_sig_desc, 
                D3D_ROOT_SIGNATURE_VERSION_1, 
                &mut blob, 
                &mut err_blob
            )?;

            let blob = blob.unwrap();
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

        let device: ID3D12Device5 = self.device.as_ref().expect("You have to initialize a device").cast()?;

        //global root sigantureの生成とメソッドを分けるべきか？

        let mut sub_objects = vec![];

        //シンボルをエクスポート

        let mut exports = [
            D3D12_EXPORT_DESC { 
                Name: PWSTR(self.ray_gen_symbol.encode_utf16().collect::<Vec<u16>>().as_mut_ptr()),
                ExportToRename: PWSTR(std::ptr::null_mut()),
                Flags: D3D12_EXPORT_FLAG_NONE
            },
            D3D12_EXPORT_DESC { 
                Name: PWSTR(self.miss_symbol.encode_utf16().collect::<Vec<u16>>().as_mut_ptr()),
                ExportToRename: PWSTR(std::ptr::null_mut()),
                Flags: D3D12_EXPORT_FLAG_NONE
            },
            D3D12_EXPORT_DESC { 
                Name: PWSTR(self.closest_hit_symbol.encode_utf16().collect::<Vec<u16>>().as_mut_ptr()),
                ExportToRename: PWSTR(std::ptr::null_mut()),
                Flags: D3D12_EXPORT_FLAG_NONE
            },
        ];

        let mut dxil_lib_desc = D3D12_DXIL_LIBRARY_DESC {
            DXILLibrary: D3D12_SHADER_BYTECODE {
                pShaderBytecode: unsafe { self.ray_shader_blob.GetBufferPointer() },
                BytecodeLength: unsafe { self.ray_shader_blob.GetBufferSize() },
            },
            pExports: &mut exports as *mut _ as _,
            NumExports: exports.len() as u32,
        };

        sub_objects.push(
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_DXIL_LIBRARY,
                pDesc: &mut dxil_lib_desc as *mut _ as _,
            }
        );

        //ヒットグループの生成

        let mut hit_group_desc = D3D12_HIT_GROUP_DESC {
            Type: D3D12_HIT_GROUP_TYPE_TRIANGLES,
            ClosestHitShaderImport: PWSTR(self.closest_hit_symbol.encode_utf16().collect::<Vec<u16>>().as_mut_ptr()),
            HitGroupExport: PWSTR(self.default_hit_group_symbol.encode_utf16().collect::<Vec<u16>>().as_mut_ptr()),
            ..Default::default()
        };

        sub_objects.push(
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_HIT_GROUP,
                pDesc: &mut hit_group_desc as *mut _ as _,
            }
        );

        //グローバルルートシグニチャをsubobjectに追加

        let mut global_root_signature = D3D12_GLOBAL_ROOT_SIGNATURE {
            //cloneしても良いか？
            pGlobalRootSignature: self.global_root_signature.clone(),
        };

        sub_objects.push(
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_GLOBAL_ROOT_SIGNATURE,
                pDesc: &mut global_root_signature as *mut _ as _,
            }
        );

        //シェーダーのペイロードやアトリビュートの設定

        let mut shader_config = D3D12_RAYTRACING_SHADER_CONFIG {
            //XMFLOAT3などの大きさの指定はこれで大丈夫か？
            MaxPayloadSizeInBytes: std::mem::size_of::<[f32; 3]>() as u32,
            MaxAttributeSizeInBytes: std::mem::size_of::<[f32; 2]>() as u32,
        };

        sub_objects.push(
            D3D12_STATE_SUBOBJECT { 
                Type: D3D12_STATE_SUBOBJECT_TYPE_RAYTRACING_SHADER_CONFIG, 
                pDesc: &mut shader_config as *mut _ as _,
            }
        );

        //レイトレ中のデプス設定

        let mut raytracing_pipeline_config = D3D12_RAYTRACING_PIPELINE_CONFIG {
            MaxTraceRecursionDepth: 1,
        };

        sub_objects.push(
            D3D12_STATE_SUBOBJECT {
                Type: D3D12_STATE_SUBOBJECT_TYPE_RAYTRACING_PIPELINE_CONFIG,
                pDesc: &mut raytracing_pipeline_config as *mut _ as _,
            }
        );

        let state_object_desc = D3D12_STATE_OBJECT_DESC {
            Type: D3D12_STATE_OBJECT_TYPE_RAYTRACING_PIPELINE,
            NumSubobjects: sub_objects.len() as u32,
            pSubobjects: &mut sub_objects as *mut _ as _,
        };

        self.state_object = Some(unsafe {
            device.CreateStateObject(
                &state_object_desc
            )?
        });

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

    fn present(&mut self) {

    }

    pub fn update(&mut self) {

    }

    pub fn render(&mut self) {
        
    }
}