use std::ffi::c_void;

use cgmath::{Matrix, Matrix4, SquareMatrix, Vector2, Vector3};
use image::{GenericImageView, ImageBuffer, Rgb, Rgba};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_DEBUG, D3DCOMPILE_SKIP_OPTIMIZATION},
    Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D12::*,
    Win32::Graphics::Dxgi::Common::*,
    Win32::Graphics::{Direct3D::Fxc::D3DCompileFromFile, Dxgi::*},
    Win32::System::Threading::*,
    Win32::UI::WindowsAndMessaging::*,
};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    platform::{run_return::EventLoopExtRunReturn, windows::WindowExtWindows},
    window::WindowBuilder,
};

use rand::prelude::*;

const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 720;

fn main() -> Result<()> {
    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .build(&event_loop)
        .unwrap();

    let hwnd = HWND(window.hwnd());
    enable_debug_layer().unwrap();

    let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_DEBUG)? };

    let adapter = 'outer: {
        let mut i = 0;
        loop {
            let adapter = unsafe { factory.EnumAdapters1(i) }?;
            i += 1;

            let mut desc = Default::default();
            unsafe { adapter.GetDesc1(&mut desc) }.unwrap();

            let device_name = String::from_utf16_lossy(&desc.Description);
            println!("{}", device_name);

            if device_name.contains("NVIDIA") {
                break 'outer adapter;
            }
        }
    };

    let levels = [
        D3D_FEATURE_LEVEL_12_2,
        D3D_FEATURE_LEVEL_12_1,
        D3D_FEATURE_LEVEL_12_0,
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
    ];

    let mut device: Option<ID3D12Device> = None;
    for lv in levels {
        if unsafe { D3D12CreateDevice(&adapter, lv, &mut device) }.is_ok() {
            break;
        }
    }
    let device = device.unwrap();

    let command_allocator: ID3D12CommandAllocator =
        unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }.unwrap();
    let command_list: ID3D12GraphicsCommandList = unsafe {
        device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &command_allocator, None)
    }
    .unwrap();
    let command_queue_desc = D3D12_COMMAND_QUEUE_DESC {
        Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
        NodeMask: 0,
        Priority: D3D12_COMMAND_QUEUE_PRIORITY_NORMAL.0,
        Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
    };

    let command_queue: ID3D12CommandQueue =
        unsafe { device.CreateCommandQueue(&command_queue_desc) }?;

    let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
        BufferCount: 2,
        Width: WINDOW_WIDTH,
        Height: WINDOW_HEIGHT,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        Stereo: false.into(),
        BufferUsage: DXGI_USAGE_BACK_BUFFER,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        Scaling: DXGI_SCALING_STRETCH,
        AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
        Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH.0 as u32,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
    };

    let swap_chain: IDXGISwapChain4 = unsafe {
        factory.CreateSwapChainForHwnd(&command_queue, hwnd, &swap_chain_desc, None, None)?
    }
    .cast()?;

    let rtv_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
        Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        NodeMask: 0,
        NumDescriptors: 2,
        Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
    };
    let rtv_heap: ID3D12DescriptorHeap = unsafe { device.CreateDescriptorHeap(&rtv_heap_desc) }?;

    let rtv_descpter_size =
        unsafe { device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) } as usize;

    let rtv_handle = unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() };

    let render_target_view_desc = D3D12_RENDER_TARGET_VIEW_DESC {
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        ViewDimension: D3D12_RTV_DIMENSION_TEXTURE2D,
        ..Default::default()
    };

    let back_buffer: [ID3D12Resource; 2] =
        array_init::try_array_init(|i| -> Result<ID3D12Resource> {
            let render_target: ID3D12Resource = unsafe { swap_chain.GetBuffer(i as u32) }?;
            unsafe {
                device.CreateRenderTargetView(
                    &render_target,
                    Some(&render_target_view_desc),
                    D3D12_CPU_DESCRIPTOR_HANDLE {
                        ptr: rtv_handle.ptr + i * rtv_descpter_size,
                    },
                )
            }
            Ok(render_target)
        })?;

    let fence: ID3D12Fence = unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.unwrap();

    let mut fence_val = 1;

    unsafe {
        command_list.Close()?;
    };
    let mut closed = false;

    let vertices = [
        Vertex {
            pos: Vector3::new(-0.4, -0.7, 0.),
            uv: Vector2::new(0., 1.),
        },
        Vertex {
            pos: Vector3::new(-0.4f32, 0.7, 0.),
            uv: Vector2::new(0., 0.),
        },
        Vertex {
            pos: Vector3::new(0.4f32, -0.7, 0.),
            uv: Vector2::new(1., 1.),
        },
        Vertex {
            pos: Vector3::new(0.4f32, 0.7, 0.),
            uv: Vector2::new(1., 0.),
        },
    ];

    let vertex_heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_UPLOAD,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        ..Default::default()
    };
    let vertex_resource_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: std::mem::size_of_val(&vertices) as u64,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            ..Default::default()
        },
        Flags: D3D12_RESOURCE_FLAG_NONE,
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        ..Default::default()
    };

    let mut vertex_buffer: Option<ID3D12Resource> = None;
    unsafe {
        device.CreateCommittedResource(
            &vertex_heap_properties,
            D3D12_HEAP_FLAG_NONE,
            &vertex_resource_desc,
            D3D12_RESOURCE_STATE_GENERIC_READ,
            None,
            &mut vertex_buffer,
        )
    }
    .unwrap();
    let vertex_buffer = vertex_buffer.unwrap();
    unsafe {
        let mut vertex_map = std::ptr::null_mut();
        vertex_buffer.Map(0, None, Some(&mut vertex_map)).unwrap();
        std::ptr::copy_nonoverlapping(vertices.as_ptr(), vertex_map as *mut Vertex, vertices.len());
        vertex_buffer.Unmap(0, None);
    }

    let vertex_buffer_view = D3D12_VERTEX_BUFFER_VIEW {
        BufferLocation: unsafe { vertex_buffer.GetGPUVirtualAddress() },
        SizeInBytes: std::mem::size_of_val(&vertices) as u32,
        StrideInBytes: std::mem::size_of_val(&vertices[0]) as u32,
    };

    let mut vertex_shader = None;
    let error_blob = None;

    let exe_path = std::env::current_exe().ok().unwrap();
    let asset_path = exe_path.parent().unwrap();
    let vertex_shaders_hlsl_path = asset_path.join("BasicVertexShader.hlsl");
    let vertex_shaders_hlsl = vertex_shaders_hlsl_path.to_str().unwrap();
    let vertex_shaders_hlsl: HSTRING = vertex_shaders_hlsl.into();
    unsafe {
        D3DCompileFromFile(
            &vertex_shaders_hlsl,
            None,
            None,
            s!("BasicVS"),
            s!("vs_5_0"),
            D3DCOMPILE_DEBUG | D3DCOMPILE_SKIP_OPTIMIZATION,
            0,
            &mut vertex_shader,
            error_blob,
        )
    }
    .unwrap();
    let vertex_shader = vertex_shader.unwrap();

    if let Some(e_option) = error_blob {
        let e_option_ptr = e_option.cast_const();
        let error_blob = unsafe { std::ptr::read(e_option_ptr) };
        println!("{:?}", error_blob);
    }

    let mut pixel_shader = None;
    let pixel_shaders_hlsl_path = asset_path.join("BasicPixelShader.hlsl");
    let pixel_shaders_hlsl = pixel_shaders_hlsl_path.to_str().unwrap();
    let pixel_shaders_hlsl: HSTRING = pixel_shaders_hlsl.into();
    unsafe {
        D3DCompileFromFile(
            &pixel_shaders_hlsl,
            None,
            None,
            s!("BasicPS"),
            s!("ps_5_0"),
            D3DCOMPILE_DEBUG | D3DCOMPILE_SKIP_OPTIMIZATION,
            0,
            &mut pixel_shader,
            error_blob,
        )
    }
    .unwrap();
    let pixel_shader = pixel_shader.unwrap();

    if let Some(e_option) = error_blob {
        let e_option_ptr = e_option.cast_const();
        let error_blob = unsafe { std::ptr::read(e_option_ptr) };
        println!("{:?}", error_blob);
    }

    let indices = [0u16, 1, 2, 2, 1, 3];
    let mut index_buffer: Option<ID3D12Resource> = None;

    let index_heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_UPLOAD,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        ..Default::default()
    };
    let index_resource_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: std::mem::size_of_val(&indices) as u64,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            ..Default::default()
        },
        Flags: D3D12_RESOURCE_FLAG_NONE,
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        ..Default::default()
    };

    unsafe {
        device.CreateCommittedResource(
            &index_heap_properties,
            D3D12_HEAP_FLAG_NONE,
            &index_resource_desc,
            D3D12_RESOURCE_STATE_GENERIC_READ,
            None,
            &mut index_buffer,
        )
    }
    .unwrap();
    let index_buffer = index_buffer.unwrap();

    unsafe {
        let mut data = std::ptr::null_mut();
        index_buffer.Map(0, None, Some(&mut data)).unwrap();
        std::ptr::copy_nonoverlapping(indices.as_ptr(), data as *mut u16, indices.len());
        index_buffer.Unmap(0, None);
    }

    let index_view = D3D12_INDEX_BUFFER_VIEW {
        BufferLocation: unsafe { index_buffer.GetGPUVirtualAddress() },
        Format: DXGI_FORMAT_R16_UINT,
        SizeInBytes: std::mem::size_of_val(&indices) as u32,
    };

    let bytes = include_bytes!("./img/textest200x200.png");
    let png_image = image::load_from_memory(bytes).unwrap();
    let rgba_texture = png_image.to_rgba8();

    let row_pictch =
        std::mem::size_of::<u8>() * (rgba_texture.len() / rgba_texture.height() as usize);

    let size = alignmented_size(row_pictch, D3D12_TEXTURE_DATA_PITCH_ALIGNMENT as usize);

    let mut new_image = vec![];
    for r in rgba_texture.rows() {
        let mut new_row = vec![];
        for p in r {
            let mut tmp = p.0.clone().to_vec();
            new_row.append(&mut tmp);
        }
        new_row.resize(size, 0);
        new_image.append(&mut new_row);
    }

    let mut rng = rand::thread_rng();

    let _ = rng.gen_range(0..255);
    let _ = rng.gen_range(0..255);
    let _ = rng.gen_range(0..255);
    let mut texture_data = Vec::new();
    for _ in 0..(256 * 256) {
        let texture = TexRGBA {
            r: rng.gen_range(0..255),
            g: rng.gen_range(0..255),
            b: rng.gen_range(0..255),
            a: 255,
        };
        texture_data.push(texture);
    }

    let upload_heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_UPLOAD,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 0,
        VisibleNodeMask: 0,
    };

    let upload_resource_desc = D3D12_RESOURCE_DESC {
        Format: DXGI_FORMAT_UNKNOWN,
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: (size * rgba_texture.height() as usize) as u64,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_NONE,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        ..Default::default()
    };

    let mut upload_buffer: Option<ID3D12Resource> = None;
    unsafe {
        device.CreateCommittedResource(
            &upload_heap_properties,
            D3D12_HEAP_FLAG_NONE,
            &upload_resource_desc,
            D3D12_RESOURCE_STATE_GENERIC_READ,
            None,
            &mut upload_buffer,
        )
    }
    .unwrap();
    let upload_buffer = upload_buffer.unwrap();

    unsafe {
        let mut map_for_image = std::ptr::null_mut();
        upload_buffer
            .Map(0, None, Some(&mut map_for_image))
            .unwrap();
        std::ptr::copy_nonoverlapping(
            new_image.as_ptr(),
            map_for_image as *mut u8,
            new_image.len(),
        );
        upload_buffer.Unmap(0, None);
    }

    let src = D3D12_TEXTURE_COPY_LOCATION {
        pResource: std::mem::ManuallyDrop::new(Some(upload_buffer)),
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                Offset: 0,
                Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    Width: rgba_texture.width(),
                    Height: rgba_texture.height(),
                    Depth: 1,
                    RowPitch: size as u32,
                },
            },
        },
    };

    let texture_heap_prop = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_CUSTOM,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_WRITE_BACK,
        MemoryPoolPreference: D3D12_MEMORY_POOL_L0,
        CreationNodeMask: 0,
        VisibleNodeMask: 0,
    };

    let texture_resource_desc = D3D12_RESOURCE_DESC {
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        Width: png_image.width() as u64,
        Height: png_image.height() as u32,
        DepthOrArraySize: 1,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        MipLevels: 1,
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        Flags: D3D12_RESOURCE_FLAG_NONE,
        ..Default::default()
    };

    let mut texture_buffer: Option<ID3D12Resource> = None;

    unsafe {
        device.CreateCommittedResource(
            &texture_heap_prop,
            D3D12_HEAP_FLAG_NONE,
            &texture_resource_desc,
            D3D12_RESOURCE_STATE_COPY_DEST,
            None,
            &mut texture_buffer,
        )
    }
    .unwrap();
    let texture_buffer = texture_buffer.unwrap();

    let dst = D3D12_TEXTURE_COPY_LOCATION {
        pResource: std::mem::ManuallyDrop::new(Some(texture_buffer.can_clone_into())),
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            SubresourceIndex: 0,
        },
    };

    unsafe {
        command_allocator.Reset().unwrap();
        command_list.Reset(&command_allocator, None).unwrap();
    }

    unsafe { command_list.CopyTextureRegion(&dst, 0, 0, 0, &src, None) }
    let texuture_barrier = transition_barrier(
        &texture_buffer,
        D3D12_RESOURCE_STATE_COPY_DEST,
        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
    );

    unsafe { command_list.ResourceBarrier(&[texuture_barrier]) };
    unsafe { command_list.Close() }.unwrap();

    let command_lists: [Option<ID3D12CommandList>; 1] = [Some(command_list.can_clone_into())];

    unsafe { command_queue.ExecuteCommandLists(&command_lists) };
    unsafe { swap_chain.Present(1, 0) }.unwrap();

    unsafe { command_queue.Signal(&fence, fence_val) }.unwrap();

    if unsafe { fence.GetCompletedValue() } < fence_val {
        unsafe {
            let fence_event = CreateEventA(None, false, false, None).unwrap();
            fence.SetEventOnCompletion(fence_val, fence_event).unwrap();
            WaitForSingleObject(fence_event, INFINITE);
            CloseHandle(fence_event);
        }
    }
    fence_val += 1;

    let basic_descriptor_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
        Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        NodeMask: 0,
        NumDescriptors: 2,
        Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
    };

    let basic_descriptor_heap: ID3D12DescriptorHeap =
        unsafe { device.CreateDescriptorHeap(&basic_descriptor_heap_desc) }.unwrap();

    let mut matrix = Matrix4::identity();

    let const_heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_UPLOAD,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        ..Default::default()
    };
    let const_resource_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: alignmented_size(std::mem::size_of_val(&matrix), 256) as u64,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            ..Default::default()
        },
        Flags: D3D12_RESOURCE_FLAG_NONE,
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        ..Default::default()
    };

    let mut const_buffer: Option<ID3D12Resource> = None;
    unsafe {
        device.CreateCommittedResource(
            &const_heap_properties,
            D3D12_HEAP_FLAG_NONE,
            &const_resource_desc,
            D3D12_RESOURCE_STATE_GENERIC_READ,
            None,
            &mut const_buffer,
        )
    }
    .unwrap();
    let const_buffer = const_buffer.unwrap();

    unsafe {
        let mut const_map = std::ptr::null_mut();
        const_buffer.Map(0, None, Some(&mut const_map)).unwrap();
        std::ptr::copy_nonoverlapping(matrix.as_mut_ptr(), const_map as *mut f32, 16);
    }

    let shader_resource_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_SRV {
                MipLevels: 1,
                ..Default::default()
            },
        },
    };
    let basic_heap_handle = unsafe { basic_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };

    unsafe {
        device.CreateShaderResourceView(
            &texture_buffer,
            Some(&shader_resource_desc),
            basic_heap_handle,
        )
    };

    let basic_heap_handle = D3D12_CPU_DESCRIPTOR_HANDLE {
        ptr: basic_heap_handle.ptr
            + unsafe {
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
                    as usize
            },
    };

    let const_buffer_view_desc = unsafe {
        D3D12_CONSTANT_BUFFER_VIEW_DESC {
            BufferLocation: const_buffer.GetGPUVirtualAddress(),
            SizeInBytes: const_buffer.GetDesc().Width as u32,
        }
    };

    unsafe { device.CreateConstantBufferView(Some(&const_buffer_view_desc), basic_heap_handle) };

    let mut descriptor_ranges = [D3D12_DESCRIPTOR_RANGE::default(); 2];

    descriptor_ranges[0] = D3D12_DESCRIPTOR_RANGE {
        NumDescriptors: 1,
        RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
        BaseShaderRegister: 0,
        OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        ..Default::default()
    };

    descriptor_ranges[1] = D3D12_DESCRIPTOR_RANGE {
        NumDescriptors: 1,
        RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_CBV,
        BaseShaderRegister: 0,
        OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        ..Default::default()
    };

    let root_parameter = D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                NumDescriptorRanges: 2,
                pDescriptorRanges: descriptor_ranges.as_ptr(),
            },
        },
    };

    //root_parameters[1] = D3D12_ROOT_PARAMETER {
    //    ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
    //    ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
    //    Anonymous: D3D12_ROOT_PARAMETER_0 {
    //        DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
    //            NumDescriptorRanges: 1,
    //            pDescriptorRanges: &descriptor_ranges[1],
    //        },
    //    },
    //};

    let sampler_desc = D3D12_STATIC_SAMPLER_DESC {
        AddressU: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
        AddressV: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
        AddressW: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
        BorderColor: D3D12_STATIC_BORDER_COLOR_TRANSPARENT_BLACK,
        Filter: D3D12_FILTER_MIN_MAG_MIP_LINEAR,
        MaxLOD: D3D12_FLOAT32_MAX,
        MinLOD: 0.0f32,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
        ..Default::default()
    };

    let input_layout = [
        D3D12_INPUT_ELEMENT_DESC {
            SemanticName: s!("POSITION"),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32B32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        D3D12_INPUT_ELEMENT_DESC {
            SemanticName: s!("TEXCOORD"),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
    ];

    let mut render_target_blend_descs = [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8];
    render_target_blend_descs[0] = D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: false.into(),
        LogicOpEnable: false.into(),
        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
        ..Default::default()
    };

    let root_signature_desc = D3D12_ROOT_SIGNATURE_DESC {
        Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        pParameters: &root_parameter,
        NumParameters: 1,
        pStaticSamplers: &sampler_desc,
        NumStaticSamplers: 1,
        ..Default::default()
    };

    let mut root_signature_blob = None;
    unsafe {
        D3D12SerializeRootSignature(
            &root_signature_desc,
            D3D_ROOT_SIGNATURE_VERSION_1_0,
            &mut root_signature_blob,
            error_blob,
        )
    }
    .unwrap();

    let root_signature_blob = root_signature_blob.unwrap();
    let root_signature: ID3D12RootSignature = unsafe {
        device.CreateRootSignature(
            0,
            &std::slice::from_raw_parts(
                root_signature_blob.GetBufferPointer() as _,
                root_signature_blob.GetBufferSize(),
            ),
        )
    }
    .unwrap();

    let mut graphic_pipeline_state_desc = unsafe {
        D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::ManuallyDrop::new(Some(std::mem::transmute_copy(
                &root_signature,
            ))),
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vertex_shader.GetBufferPointer(),
                BytecodeLength: vertex_shader.GetBufferSize(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: pixel_shader.GetBufferPointer(),
                BytecodeLength: pixel_shader.GetBufferSize(),
            },
            SampleMask: D3D12_DEFAULT_SAMPLE_MASK,
            RasterizerState: D3D12_RASTERIZER_DESC {
                MultisampleEnable: false.into(),
                CullMode: D3D12_CULL_MODE_NONE,
                FillMode: D3D12_FILL_MODE_SOLID,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            BlendState: D3D12_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: render_target_blend_descs,
            },
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: input_layout.as_ptr(),
                NumElements: input_layout.len() as u32,
            },
            IBStripCutValue: D3D12_INDEX_BUFFER_STRIP_CUT_VALUE_DISABLED,
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        }
    };
    graphic_pipeline_state_desc.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
    let graphic_pipeline_state: ID3D12PipelineState =
        unsafe { device.CreateGraphicsPipelineState(&graphic_pipeline_state_desc) }.unwrap();

    let view_port = D3D12_VIEWPORT {
        Width: WINDOW_WIDTH as f32,
        Height: WINDOW_HEIGHT as f32,
        TopLeftX: 0.,
        TopLeftY: 0.,
        MaxDepth: 1.,
        MinDepth: 0.,
    };

    let scissor_rect = RECT {
        top: 0,
        left: 0,
        right: WINDOW_WIDTH as i32,
        bottom: WINDOW_HEIGHT as i32,
    };

    loop {
        event_loop.run_return(|event, _, control_flow| {
            control_flow.set_poll();

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    println!("The close button was pressed; stopping");
                    closed = true;
                    unsafe { PostQuitMessage(0) };
                    control_flow.set_exit();
                }
                Event::MainEventsCleared if !closed => {
                    let bb_idx = unsafe { swap_chain.GetCurrentBackBufferIndex() } as usize;
                    unsafe { command_allocator.Reset().unwrap() };
                    unsafe { command_list.Reset(&command_allocator, None) }.unwrap();

                    let barrier = transition_barrier(
                        &back_buffer[bb_idx],
                        D3D12_RESOURCE_STATE_PRESENT,
                        D3D12_RESOURCE_STATE_RENDER_TARGET,
                    );

                    unsafe { command_list.ResourceBarrier(&[barrier]) };

                    unsafe { command_list.SetPipelineState(&graphic_pipeline_state) };
                    let rtv_handle = D3D12_CPU_DESCRIPTOR_HANDLE {
                        ptr: unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() }.ptr
                            + bb_idx * rtv_descpter_size,
                    };

                    unsafe { command_list.OMSetRenderTargets(1, Some(&rtv_handle), true, None) };

                    let clear_color = [1.0_f32, 1.0, 0.0, 1.0];
                    unsafe {
                        command_list.ClearRenderTargetView(
                            rtv_handle,
                            &*clear_color.as_ptr(),
                            None,
                        );
                    }

                    unsafe { command_list.SetGraphicsRootSignature(&root_signature) };
                    unsafe {
                        let basic_descriptor_heaps: [Option<ID3D12DescriptorHeap>; 1] =
                            [Some(basic_descriptor_heap.can_clone_into())];
                        command_list.SetDescriptorHeaps(&basic_descriptor_heaps);
                    }
                    let heap_handle =
                        unsafe { basic_descriptor_heap.GetGPUDescriptorHandleForHeapStart() };
                    unsafe { command_list.SetGraphicsRootDescriptorTable(0, heap_handle) };
                    //let heap_handle = D3D12_GPU_DESCRIPTOR_HANDLE {
                    //    ptr: heap_handle.ptr
                    //        + unsafe {
                    //            device.GetDescriptorHandleIncrementSize(
                    //                D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    //            )
                    //        } as u64,
                    //};
                    //unsafe { command_list.SetGraphicsRootDescriptorTable(1, heap_handle) };

                    unsafe { command_list.RSSetViewports(&[view_port]) }
                    unsafe { command_list.RSSetScissorRects(&[scissor_rect]) }
                    unsafe {
                        command_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST)
                    };

                    unsafe {
                        command_list.IASetVertexBuffers(0, Some(&[vertex_buffer_view]));
                    }
                    unsafe {
                        command_list.IASetIndexBuffer(Some(&index_view));
                    }

                    unsafe {
                        command_list.DrawIndexedInstanced(6, 1, 0, 0, 0);
                    }

                    unsafe {
                        command_list.ResourceBarrier(&[transition_barrier(
                            &back_buffer[bb_idx],
                            D3D12_RESOURCE_STATE_RENDER_TARGET,
                            D3D12_RESOURCE_STATE_PRESENT,
                        )])
                    };

                    unsafe { command_list.Close() }.unwrap();
                    let command_lists: [Option<ID3D12CommandList>; 1] =
                        [Some(command_list.can_clone_into())];

                    unsafe { command_queue.ExecuteCommandLists(&command_lists) };
                    unsafe { swap_chain.Present(1, 0) }.unwrap();

                    unsafe { command_queue.Signal(&fence, fence_val) }.unwrap();

                    if unsafe { fence.GetCompletedValue() } < fence_val {
                        unsafe {
                            let fence_event = CreateEventA(None, false, false, None).unwrap();
                            fence.SetEventOnCompletion(fence_val, fence_event).unwrap();
                            WaitForSingleObject(fence_event, INFINITE);
                            CloseHandle(fence_event);
                        }
                    }
                    fence_val += 1;
                }
                _ => {}
            }
        });
        break;
    }
    Ok(())
}

fn transition_barrier(
    resource: &ID3D12Resource,
    state_before: D3D12_RESOURCE_STATES,
    state_after: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: unsafe { std::mem::transmute_copy(resource) },
                StateBefore: state_before,
                StateAfter: state_after,
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
            }),
        },
    }
}

fn enable_debug_layer() -> Option<()> {
    let mut debug: Option<ID3D12Debug> = None;
    unsafe {
        D3D12GetDebugInterface(&mut debug).unwrap();
        debug?.EnableDebugLayer();
    }
    Some(())
}

#[repr(C)]
struct Vertex {
    pos: Vector3<f32>,
    uv: Vector2<f32>,
}

#[repr(C)]
struct TexRGBA {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

fn alignmented_size(size: usize, alignment: usize) -> usize {
    let alignment = alignment - 1;
    (size + alignment) & !alignment
}
