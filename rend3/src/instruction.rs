use crate::{
    resources::{MaterialManager, TextureManager},
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Mesh,
        Object, RawMaterialHandle, RawObjectHandle,
    },
    RendererMode,
};
use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{MaterialHandle, MeshHandle, ObjectHandle, RawDirectionalLightHandle, TextureHandle};
use std::mem;
use wgpu::{CommandBuffer, Device, Texture, TextureDescriptor, TextureView};

pub enum Instruction {
    AddMesh {
        handle: MeshHandle,
        mesh: Mesh,
    },
    AddTexture {
        handle: TextureHandle,
        desc: TextureDescriptor<'static>,
        texture: Texture,
        view: TextureView,
        buffer: Option<CommandBuffer>,
        cube: bool,
    },
    AddMaterial {
        handle: MaterialHandle,
        fill_invoke: Box<dyn FnOnce(&mut MaterialManager, &Device, RendererMode, &mut TextureManager, &MaterialHandle)>,
    },
    ChangeMaterial {
        handle: RawMaterialHandle,
        change_invoke: Box<dyn FnOnce(&mut MaterialManager, &Device, RendererMode, &mut TextureManager, &MaterialHandle)>,
    },
    AddObject {
        handle: ObjectHandle,
        object: Object,
    },
    SetObjectTransform {
        handle: RawObjectHandle,
        transform: Mat4,
    },
    AddDirectionalLight {
        handle: DirectionalLightHandle,
        light: DirectionalLight,
    },
    ChangeDirectionalLight {
        handle: RawDirectionalLightHandle,
        change: DirectionalLightChange,
    },
    SetAspectRatio {
        ratio: f32,
    },
    SetCameraData {
        data: Camera,
    },
}

pub struct InstructionStreamPair {
    pub producer: Mutex<Vec<Instruction>>,
    pub consumer: Mutex<Vec<Instruction>>,
}
impl InstructionStreamPair {
    pub fn new() -> Self {
        Self {
            producer: Mutex::new(Vec::new()),
            consumer: Mutex::new(Vec::new()),
        }
    }

    pub fn swap(&self) {
        let mut produce = self.producer.lock();
        let mut consume = self.consumer.lock();

        mem::swap(&mut *produce, &mut *consume);
    }
}
