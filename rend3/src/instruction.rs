use crate::types::{
    Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material, MaterialChange, Mesh, Object,
    RawMaterialHandle, RawObjectHandle,
};
use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{MaterialHandle, MeshHandle, ObjectHandle, RawDirectionalLightHandle, TextureHandle};
use std::mem;
use wgpu::{CommandBuffer, Texture, TextureDescriptor, TextureView};

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
        material: Material,
    },
    ChangeMaterial {
        handle: RawMaterialHandle,
        change: MaterialChange,
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
