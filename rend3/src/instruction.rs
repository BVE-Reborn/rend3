use crate::{
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material, MaterialChange,
        MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    InternalSurfaceOptions,
};
use glam::Mat4;
use parking_lot::Mutex;
use std::mem;

pub enum Instruction {
    AddMesh {
        handle: MeshHandle,
        mesh: Mesh,
    },
    AddTexture2D {
        handle: TextureHandle,
        texture: Texture,
    },
    AddTextureCube {
        handle: TextureHandle,
        texture: Texture,
    },
    AddMaterial {
        handle: MaterialHandle,
        material: Material,
    },
    ChangeMaterial {
        handle: MaterialHandle,
        change: MaterialChange,
    },
    AddObject {
        handle: ObjectHandle,
        object: Object,
    },
    SetObjectTransform {
        handle: ObjectHandle,
        transform: Mat4,
    },
    AddDirectionalLight {
        handle: DirectionalLightHandle,
        light: DirectionalLight,
    },
    ChangeDirectionalLight {
        handle: DirectionalLightHandle,
        change: DirectionalLightChange,
    },
    SetInternalSurfaceOptions {
        options: InternalSurfaceOptions,
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
