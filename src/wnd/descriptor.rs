use windows::{
    core::*, Win32::Foundation::*, Win32::Graphics::Direct3D::Fxc::*, Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D12::*, Win32::Graphics::Dxgi::Common::*, Win32::Graphics::Dxgi::*,
    Win32::System::LibraryLoader::*, Win32::System::Threading::*,
    Win32::System::WindowsProgramming::*, Win32::UI::WindowsAndMessaging::*,
};

pub struct Descriptor {
    pub offset: usize,
    pub h_cpu: D3D12_CPU_DESCRIPTOR_HANDLE,
    pub h_gpu: D3D12_GPU_DESCRIPTOR_HANDLE,
    pub heap_type: D3D12_DESCRIPTOR_HEAP_TYPE,
}

impl Descriptor {
    pub fn new(offset: usize, h_cpu: D3D12_CPU_DESCRIPTOR_HANDLE, h_gpu: D3D12_GPU_DESCRIPTOR_HANDLE, heap_type: D3D12_DESCRIPTOR_HEAP_TYPE) -> Self {
        Descriptor { offset, h_cpu, h_gpu, heap_type }
    } 
}

pub struct DescriptorHeapManager {
    heap: ID3D12DescriptorHeap,
    heap_desc: D3D12_DESCRIPTOR_HEAP_DESC,
    inc_size: u32,
    allocate_index: u32,
}

impl DescriptorHeapManager {
    pub fn new(heap: ID3D12DescriptorHeap, heap_desc: D3D12_DESCRIPTOR_HEAP_DESC, inc_size: u32) -> Self {
        DescriptorHeapManager { heap, heap_desc, inc_size, allocate_index: 0 }
    }

    pub fn allocate(&self) -> core::result::Result<Descriptor, &'static str> {
        let mut h_cpu = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
        let mut h_gpu = unsafe { self.heap.GetGPUDescriptorHandleForHeapStart() };

        if self.allocate_index < self.heap_desc.NumDescriptors {
            let offset = self.inc_size as usize * self.allocate_index as usize;
            h_cpu.ptr += offset;
            h_gpu.ptr += offset as u64;

            Ok(Descriptor::new(offset, h_cpu, h_gpu, self.heap_desc.Type))
        } else {
            Err("Failed to allocate descriptor")
        }
    }
}