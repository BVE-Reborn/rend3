use crate::{
    datatypes::{
        AffineTransform, Material, MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
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
        mesh: MeshHandle,
    },
    AddTexture {
        handle: TextureHandle,
        texture: Texture,
    },
    RemoveTexture {
        texture: TextureHandle,
    },
    AddMaterial {
        handle: MaterialHandle,
        material: Material,
    },
    RemoveMaterial {
        material: MaterialHandle,
    },
    AddObject {
        handle: ObjectHandle,
        object: Object,
    },
    SetObjectTransform {
        object: ObjectHandle,
        transform: AffineTransform,
    },
    RemoveObject {
        object: ObjectHandle,
    },
    SetOptions {
        options: RendererOptions,
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
