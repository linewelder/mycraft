use cgmath::{Vector2, Vector3};
use winit::{
    event::{DeviceEvent, Event, WindowEvent},
    window::CursorGrabMode,
};

use crate::{
    camera::Camera,
    consts::*,
    context::Context,
    input1d::Input1d,
    rendering::{
        solid_block_renderer::SolidBlockRenderer,
        texture::{create_depth_buffer, Texture},
        water_renderer::WaterRenderer,
        ChunkRendererTarget,
    },
    world::{ChunkCoords, World},
};

pub struct Mycraft {
    depth_buffer: wgpu::TextureView,
    solid_block_renderer: SolidBlockRenderer,
    water_renderer: WaterRenderer,
    camera: Camera,

    movement_x_input: Input1d,
    movement_y_input: Input1d,
    movement_z_input: Input1d,

    world: World,
    test_texture: Texture,
}

impl Mycraft {
    pub fn new(context: &mut Context) -> Self {
        let mut world = World::new();
        for x in -5..5 {
            for y in -5..5 {
                world.load_chunk(ChunkCoords { x, y });
            }
        }
        world.update_chunk_graphics(context);

        let image = image::load_from_memory(include_bytes!("blocks.png")).unwrap();
        let test_texture = Texture::new(context, "Cube Texture", image);

        let depth_buffer = create_depth_buffer(
            context,
            "Block Depth Buffer",
            context.surface_config.width,
            context.surface_config.height,
        );
        let solid_block_renderer = SolidBlockRenderer::new(context);
        let water_renderer = WaterRenderer::new(context);
        let camera = Camera::new(context, "Camera");

        use winit::event::VirtualKeyCode::*;
        let movement_x_input = Input1d::new(D, A);
        let movement_y_input = Input1d::new(E, Q);
        let movement_z_input = Input1d::new(W, S);

        Mycraft {
            depth_buffer,
            solid_block_renderer,
            water_renderer,
            camera,

            movement_x_input,
            movement_y_input,
            movement_z_input,

            world,
            test_texture,
        }
    }

    pub fn event(&mut self, context: &mut Context, event: &Event<()>) {
        match event {
            Event::WindowEvent { window_id, event } if *window_id == context.window.id() => {
                match event {
                    WindowEvent::CursorEntered { .. } => {
                        let _ = context.window.set_cursor_grab(CursorGrabMode::Confined);
                        context.window.set_cursor_visible(false);
                    }

                    WindowEvent::CursorLeft { .. } => {
                        let _ = context.window.set_cursor_grab(CursorGrabMode::None);
                        context.window.set_cursor_visible(true);
                    }

                    WindowEvent::KeyboardInput { input, .. } => {
                        self.movement_x_input.update(input);
                        self.movement_y_input.update(input);
                        self.movement_z_input.update(input);
                    }

                    _ => {}
                }
            }

            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion { delta },
                ..
            } => {
                self.camera.rotate(
                    Vector2 {
                        x: delta.0 as f32,
                        y: -delta.1 as f32,
                    } * MOUSE_SENSITIVITY,
                );
            }

            _ => {}
        }
    }

    pub fn resize(&mut self, context: &mut Context, size: Vector2<u32>) {
        self.camera.resize_projection(size.x as f32 / size.y as f32);
        self.depth_buffer = create_depth_buffer(context, "Block Depth Buffer", size.x, size.y);
    }

    pub fn update(&mut self, context: &mut Context, delta: std::time::Duration) {
        let delta_secs = delta.as_secs_f32();

        let movement = Vector3 {
            x: self.movement_x_input.get_value(),
            y: self.movement_y_input.get_value(),
            z: -self.movement_z_input.get_value(),
        } * CAMERA_MOVEMENT_SPEED
            * delta_secs;

        self.camera.move_relative_to_view(movement);
        self.camera.update_matrix(context);
    }

    pub fn render(&mut self, context: &Context, target: &wgpu::TextureView) {
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let chunk_graphics = self.world.chunk_graphics.values();
        self.solid_block_renderer.draw(
            &mut encoder,
            ChunkRendererTarget {
                output: target,
                depth_buffer: &self.depth_buffer,
            },
            &self.camera,
            chunk_graphics.clone(),
            &self.test_texture,
        );
        self.water_renderer.draw(
            &mut encoder,
            ChunkRendererTarget {
                output: target,
                depth_buffer: &self.depth_buffer,
            },
            &self.camera,
            chunk_graphics,
            &self.test_texture,
        );

        context.queue.submit(std::iter::once(encoder.finish()));
    }
}
