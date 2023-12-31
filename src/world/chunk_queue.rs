use std::{cell::RefCell, rc::Rc};

use cgmath::{MetricSpace, Zero};

use crate::{
    rendering::{frustrum::Frustrum, world_renderer::ChunkGraphics},
    utils::aabb::Aabb,
};

use super::{Chunk, ChunkCoords};

struct ChunkQueueItem {
    coords: ChunkCoords,
    chunk: Rc<RefCell<Chunk>>,
    in_frustrum: bool,
}

pub struct ChunkQueue {
    queue: Vec<ChunkQueueItem>,
    needs_sort: bool,
    point_of_view: ChunkCoords,
}

fn chunk_aabb(coords: ChunkCoords) -> Aabb {
    Aabb {
        start: (coords * Chunk::SIZE).map(|x| x as f32),
        size: [Chunk::SIZE as f32; 3].into(),
    }
}

impl ChunkQueue {
    pub fn new() -> Self {
        ChunkQueue {
            queue: vec![],
            needs_sort: false,
            point_of_view: ChunkCoords::zero(),
        }
    }

    pub fn insert(&mut self, coords: ChunkCoords, chunk: Rc<RefCell<Chunk>>) {
        if let Some(exist) = self.queue.iter_mut().find(|x| x.coords == coords) {
            exist.chunk = chunk;
        } else {
            self.queue.push(ChunkQueueItem {
                coords,
                chunk,
                in_frustrum: false,
            });
            self.needs_sort = true;
        }
    }

    pub fn mark_unsorted(&mut self) {
        self.needs_sort = true;
    }

    pub fn needs_to_be_sorted(&self) -> bool {
        self.needs_sort
    }

    pub fn sort(&mut self, cam_chunk_coords: ChunkCoords) {
        self.point_of_view = cam_chunk_coords;
        self.queue
            .sort_unstable_by_key(|x| cam_chunk_coords.distance2(x.coords));
        self.needs_sort = false;
    }

    pub fn clip_to_frustrum(&mut self, frustrum: &Frustrum) {
        for item in &mut self.queue {
            item.in_frustrum = frustrum.intersects_with_aabb(&chunk_aabb(item.coords));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (ChunkCoords, &RefCell<Chunk>)> {
        self.queue
            .iter()
            .filter(|x| x.in_frustrum)
            .map(|x| (x.coords, x.chunk.as_ref()))
    }

    pub fn iter_graphics(&self) -> impl Iterator<Item = (ChunkCoords, Rc<ChunkGraphics>)> + '_ {
        self.queue
            .iter()
            .rev()
            .filter(|x| x.in_frustrum)
            .filter_map(|x| Some((x.coords, x.chunk.borrow().graphics.as_ref()?.clone())))
    }
}
