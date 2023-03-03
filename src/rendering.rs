pub mod chunk_mesh;
pub mod solid_block_renderer;
pub mod texture;
pub mod uniform;
pub mod water_renderer;

use std::{cell::RefCell, cmp::Reverse, rc::Rc};

use cgmath::{Matrix4, MetricSpace, Vector2, Vector3};

use crate::{
    context::Context,
    rendering::{chunk_mesh::ChunkMesh, uniform::Uniform},
    world::ChunkCoords,
};

#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: Vector3<f32>,
    pub tex: Vector2<f32>,
    pub normal: Vector3<f32>,
}

impl Vertex {
    const BUFFER_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3],
    };
}

pub struct Face {
    pub base_index: u32,
    pub center: Vector3<f32>,
    pub distance: f32,
}

impl Face {
    const VERTEX_INDICES: [u32; 6] = [0, 1, 2, 2, 1, 3];

    pub fn generate_default_indices(face_count: usize) -> Vec<u32> {
        Self::VERTEX_INDICES
            .iter()
            .cycle()
            .enumerate()
            .map(|(i, x)| x + (i as u32 / 6) * 4)
            .take(face_count * 6)
            .collect()
    }

    pub fn generate_indices(faces: &[Face]) -> Vec<u32> {
        faces
            .iter()
            .flat_map(|face| Self::VERTEX_INDICES.iter().map(|x| x + face.base_index))
            .collect()
    }
}

pub struct ChunkRendererTarget<'a> {
    pub output: &'a wgpu::TextureView,
    pub depth_buffer: &'a wgpu::TextureView,
}

pub struct ChunkGraphicsData {
    pub water_faces: Vec<Face>,
    pub needs_update: bool,
}

pub struct ChunkGraphics {
    pub solid_mesh: ChunkMesh,
    pub water_mesh: ChunkMesh,
    pub transform: Uniform<Matrix4<f32>>,

    pub graphics_data: RefCell<ChunkGraphicsData>,
}

impl ChunkGraphics {
    pub fn sort_water_geometry(&self, context: &mut Context, relative_cam_pos: Vector3<f32>) {
        let mut data = self.graphics_data.borrow_mut();

        for face in data.water_faces.iter_mut() {
            face.distance = relative_cam_pos.distance2(face.center);
        }

        data.water_faces
            .sort_by(|x, y| y.distance.total_cmp(&x.distance));
        self.water_mesh
            .write_indices(context, &Face::generate_indices(&data.water_faces));
    }
}

pub struct RenderQueue {
    queue: Vec<(ChunkCoords, Rc<ChunkGraphics>)>,
    needs_sort: bool,
}

impl RenderQueue {
    pub fn new() -> RenderQueue {
        RenderQueue {
            queue: vec![],
            needs_sort: false,
        }
    }

    pub fn insert(&mut self, coords: ChunkCoords, graphics: Rc<ChunkGraphics>) {
        if let Some(exist) = self.queue.iter_mut().find(|x| x.0 == coords) {
            exist.1 = graphics;
        } else {
            self.queue.push((coords, graphics));
            self.needs_sort = true;
        }
    }

    pub fn mark_unsorted(&mut self) {
        self.needs_sort = true;
    }

    pub fn sort_if_needed(&mut self, cam_chunk_coords: ChunkCoords) {
        if !self.needs_sort {
            return;
        }

        self.queue
            .sort_unstable_by_key(|x| Reverse(cam_chunk_coords.distance2(x.0)));
        self.needs_sort = false;
    }

    pub fn iter(&self) -> impl Iterator<Item = &ChunkGraphics> {
        self.queue.iter().map(|x| x.1.as_ref())
    }

    pub fn iter_with_coords(&self) -> impl Iterator<Item = (ChunkCoords, &ChunkGraphics)> {
        self.queue.iter().map(|x| (x.0, x.1.as_ref()))
    }
}
