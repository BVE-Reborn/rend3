use crate::{
    datatypes::{
        AffineTransform, CameraLocation, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material,
        MaterialChange, MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    RendererOptions,
};
use parking_lot::Mutex;
use std::mem;

pub enum Instruction {
    AddMesh {
        handle: MeshHandle,
        mesh: Mesh,
    },
    RemoveMesh {
        handle: MeshHandle,
    },
    AddTexture2D {
        handle: TextureHandle,
        texture: Texture,
    },
    RemoveTexture2D {
        handle: TextureHandle,
    },
    AddTextureCube {
        handle: TextureHandle,
        texture: Texture,
    },
    RemoveTextureCube {
        handle: TextureHandle,
    },
    AddMaterial {
        handle: MaterialHandle,
        material: Material,
    },
    ChangeMaterial {
        handle: MaterialHandle,
        change: MaterialChange,
    },
    RemoveMaterial {
        handle: MaterialHandle,
    },
    AddObject {
        handle: ObjectHandle,
        object: Object,
    },
    SetObjectTransform {
        handle: ObjectHandle,
        transform: AffineTransform,
    },
    RemoveObject {
        handle: ObjectHandle,
    },
    AddDirectionalLight {
        handle: DirectionalLightHandle,
        light: DirectionalLight,
    },
    ChangeDirectionalLight {
        handle: DirectionalLightHandle,
        change: DirectionalLightChange,
    },
    RemoveDirectionalLight {
        handle: DirectionalLightHandle,
    },
    SetOptions {
        options: RendererOptions,
    },
    SetCameraLocation {
        location: CameraLocation,
    },
    SetBackgroundTexture {
        handle: TextureHandle,
    },
    ClearBackgroundTexture,
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
