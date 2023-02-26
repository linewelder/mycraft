use wgpu::util::DeviceExt;

use crate::{context::Context, rendering::Vertex, utils::as_bytes_slice};

pub struct ChunkMesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub index_count: u32,
}

impl ChunkMesh {
    pub fn new(context: &Context, label: &str, vertices: &[Vertex], indices: &[u32]) -> Self {
        ChunkMesh {
            vertices: context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{} Vertices", label)),
                    usage: wgpu::BufferUsages::VERTEX,
                    contents: as_bytes_slice(vertices),
                }),
            indices: context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{} Indices", label)),
                    usage: wgpu::BufferUsages::INDEX,
                    contents: as_bytes_slice(indices),
                }),
            index_count: indices.len() as u32,
        }
    }
}
