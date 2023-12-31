pub mod blocks;
mod chunk_queue;
pub mod generation;
mod light;
pub mod mesh;
mod utils;

pub use utils::{get_chunk_and_block_coords, to_chunk_offset, to_local_chunk_coords};

use std::{
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
    ops::{Index, IndexMut},
    rc::Rc,
    time::Instant,
};

use cgmath::{Vector3, Zero};

use self::{
    blocks::{Block, BlockId},
    chunk_queue::ChunkQueue,
    generation::Generator,
    light::recalculate_light,
    mesh::ChunkMeshes,
};
use crate::{
    camera::Camera,
    consts::MAX_UPDATE_TIME,
    context::Context,
    rendering::{
        uniform::Uniform,
        world_renderer::{ChunkGraphics, ChunkGraphicsData, ChunkMesh, Face},
    },
};

pub type LightLevel = u8;

#[derive(Clone, Copy)]
pub struct Cell {
    pub block_id: BlockId,
    pub sun_light: LightLevel,
    pub block_light: LightLevel,
}

impl Cell {
    #[inline]
    pub fn get_block(&self) -> &'static Block {
        Block::by_id(self.block_id)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ChunkStatus {
    NotGenerated,
    LightmapOutdated,
    GraphicsOutdated,
    Ready,
}

pub struct Chunk {
    data: [[[Cell; Self::SIZE as usize]; Self::SIZE as usize]; Self::SIZE as usize],
    graphics: Option<Rc<ChunkGraphics>>,
    status: ChunkStatus,
}

impl Chunk {
    pub const SIZE: i32 = 16;

    fn new() -> Self {
        Chunk {
            data: [[[Cell {
                block_id: BlockId::Air,
                sun_light: 0,
                block_light: 0,
            }; Self::SIZE as usize]; Self::SIZE as usize]; Self::SIZE as usize],
            graphics: None,
            status: ChunkStatus::NotGenerated,
        }
    }

    /// Change the chunk status if the current one is higher.
    fn invalidate(&mut self, new_status: ChunkStatus) {
        self.status = self.status.min(new_status);
    }
}

impl Index<BlockCoords> for Chunk {
    type Output = Cell;

    #[inline]
    fn index(&self, coords: BlockCoords) -> &Self::Output {
        &self.data[coords.x as usize][coords.y as usize][coords.z as usize]
    }
}

impl IndexMut<BlockCoords> for Chunk {
    #[inline]
    fn index_mut(&mut self, coords: BlockCoords) -> &mut Self::Output {
        &mut self.data[coords.x as usize][coords.y as usize][coords.z as usize]
    }
}

pub type ChunkCoords = Vector3<i32>;
pub type BlockCoords = Vector3<i32>;

pub struct World {
    context: Rc<Context>,

    chunks: HashMap<ChunkCoords, Rc<RefCell<Chunk>>>,
    chunk_queue: ChunkQueue,
    generator: Generator,

    render_queue: Vec<Rc<ChunkGraphics>>,

    prev_cam_chunk_coords: ChunkCoords,
    prev_cam_block_coords: BlockCoords,
}

impl World {
    pub fn new(context: Rc<Context>) -> Self {
        World {
            context,

            chunks: HashMap::new(),
            chunk_queue: ChunkQueue::new(),
            generator: Generator::new(0),

            render_queue: Vec::new(),

            prev_cam_block_coords: Vector3::zero(),
            prev_cam_chunk_coords: Vector3::zero(),
        }
    }

    pub fn load_chunk(&mut self, coords: ChunkCoords) {
        if self.chunks.contains_key(&coords) {
            return;
        }

        let chunk = Rc::new(RefCell::new(Chunk::new()));
        self.chunks.insert(coords, chunk.clone());
        self.chunk_queue.insert(coords, chunk);
    }

    pub fn update(&mut self, camera: &Camera) {
        puffin::profile_function!();

        let update_start = Instant::now();

        self.check_what_is_to_sort(camera.position);

        if self.chunk_queue.needs_to_be_sorted() {
            puffin::profile_scope!("Chunk queue sort");

            self.chunk_queue.sort(self.prev_cam_chunk_coords);
        }

        self.chunk_queue.clip_to_frustrum(&camera.get_frustrum());

        for (coords, chunk) in self.chunk_queue.iter() {
            let mut chunk = chunk.borrow_mut();

            if chunk.status == ChunkStatus::NotGenerated {
                self.generator.generate_chunk(&mut chunk, coords);
                recalculate_light(self, &mut chunk, coords);
                self.invalidate_neighbors(coords, ChunkStatus::LightmapOutdated);
                chunk.status = ChunkStatus::LightmapOutdated;
            }

            if chunk.status == ChunkStatus::LightmapOutdated {
                recalculate_light(self, &mut chunk, coords);
                chunk.status = ChunkStatus::GraphicsOutdated;
            }

            if chunk.status == ChunkStatus::GraphicsOutdated {
                let graphics = self.create_chunk_graphics(coords, &chunk);
                chunk.graphics = graphics;
                chunk.status = ChunkStatus::Ready;
            }

            if let Some(graphics) = &chunk.graphics {
                if graphics.needs_water_faces_sorting() {
                    let chunk_offset = to_chunk_offset(coords);
                    let relative_cam_pos = camera.position - chunk_offset;

                    graphics.sort_water_faces(relative_cam_pos);
                }
            }

            let update_time = Instant::now() - update_start;
            if update_time > MAX_UPDATE_TIME {
                break;
            }
        }

        puffin::profile_scope!("Render queue update");

        self.render_queue.clear();
        self.chunk_queue
            .iter_graphics()
            .for_each(|x| self.render_queue.push(x.1));
    }

    fn check_what_is_to_sort(&mut self, camera_position: Vector3<f32>) {
        let (cam_chunk_coords, cam_block_coords) = get_chunk_and_block_coords(camera_position);
        if cam_chunk_coords != self.prev_cam_chunk_coords {
            self.chunk_queue.mark_unsorted();
            self.prev_cam_chunk_coords = cam_chunk_coords;
        }

        if cam_block_coords != self.prev_cam_block_coords {
            for graphics in self.render_queue.iter() {
                graphics.graphics_data.borrow_mut().water_faces_unsorted = true;
            }
            self.prev_cam_block_coords = cam_block_coords;
        }
    }

    fn create_chunk_graphics(
        &self,
        coords: ChunkCoords,
        chunk: &Chunk,
    ) -> Option<Rc<ChunkGraphics>> {
        puffin::profile_function!();

        let meshes = ChunkMeshes::generate(self, chunk, coords);
        if meshes.water_vertices.is_empty() && meshes.solid_vertices.is_empty() {
            return None;
        }

        let solid_mesh = ChunkMesh::new(
            self.context.clone(),
            "Solid Chunk Mesh",
            &meshes.solid_vertices,
            &Face::generate_default_indices(meshes.solid_vertices.len() * 4),
        );
        let water_mesh = ChunkMesh::new(
            self.context.clone(),
            "Water Chunk Mesh",
            &meshes.water_vertices,
            &Face::generate_indices(&meshes.water_faces),
        );

        let offset = Uniform::new(
            self.context.clone(),
            "Chunk Offset",
            to_chunk_offset(coords),
        );

        Some(Rc::new(ChunkGraphics {
            solid_mesh,
            water_mesh,
            offset,

            graphics_data: RefCell::new(ChunkGraphicsData {
                water_faces: meshes.water_faces,
                water_faces_unsorted: true,
            }),
        }))
    }

    #[inline]
    pub fn borrow_chunk(&self, coords: ChunkCoords) -> Option<Ref<Chunk>> {
        Some(self.chunks.get(&coords)?.borrow())
    }

    #[inline]
    pub fn borrow_mut_chunk(&self, coords: ChunkCoords) -> Option<RefMut<Chunk>> {
        Some(self.chunks.get(&coords)?.borrow_mut())
    }

    pub fn get_block(&self, coords: BlockCoords) -> Option<&'static Block> {
        let (chunk_coords, block_coords) = to_local_chunk_coords(coords);
        self.borrow_chunk(chunk_coords)
            .map(|chunk| chunk[block_coords].get_block())
    }

    fn invalidate_neighbors(&self, chunk_coords: ChunkCoords, new_status: ChunkStatus) {
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if x != 0 || y != 0 || z != 0 {
                        let coords = chunk_coords + ChunkCoords { x, y, z };
                        if let Some(mut chunk) = self.borrow_mut_chunk(coords) {
                            chunk.invalidate(new_status);
                        }
                    }
                }
            }
        }
    }

    pub fn set_block(&mut self, coords: BlockCoords, block_id: BlockId) {
        let (chunk_coords, block_coords) = to_local_chunk_coords(coords);
        if let Some(mut chunk) = self.borrow_mut_chunk(chunk_coords) {
            chunk[block_coords].block_id = block_id;
            chunk.invalidate(ChunkStatus::LightmapOutdated);
            self.invalidate_neighbors(chunk_coords, ChunkStatus::LightmapOutdated);
        }
    }

    pub fn render_queue_iter(&self) -> impl Iterator<Item = &ChunkGraphics> + Clone {
        self.render_queue.iter().map(|x| x.as_ref())
    }

    pub fn num_chunks_loaded(&self) -> usize {
        self.chunks.len()
    }

    pub fn num_chunks_rendered(&self) -> usize {
        self.render_queue.len()
    }
}
