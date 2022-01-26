use crate::{
    managers::{MaterialManager, ObjectManager, TextureManager},
    types::{Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Mesh, Object, RawObjectHandle},
    RendererProfile,
};
use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{
    MaterialHandle, MeshHandle, ObjectChange, ObjectHandle, RawDirectionalLightHandle, RawSkeletonHandle, Skeleton,
    SkeletonHandle, TextureHandle,
};
use std::{mem, panic::Location};
use wgpu::{CommandBuffer, Device, Texture, TextureDescriptor, TextureView};

pub struct Instruction {
    pub kind: InstructionKind,
    pub location: Location<'static>,
}

#[allow(clippy::type_complexity)]
pub enum InstructionKind {
    AddMesh {
        handle: MeshHandle,
        mesh: Mesh,
    },
    AddSkeleton {
        handle: SkeletonHandle,
        skeleton: Skeleton,
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
        fill_invoke: Box<
            dyn FnOnce(&mut MaterialManager, &Device, RendererProfile, &mut TextureManager, &MaterialHandle)
                + Send
                + Sync,
        >,
    },
    ChangeMaterial {
        handle: MaterialHandle,
        change_invoke: Box<
            dyn FnOnce(
                    &mut MaterialManager,
                    &Device,
                    RendererProfile,
                    &mut TextureManager,
                    &mut ObjectManager,
                    &MaterialHandle,
                ) + Send
                + Sync,
        >,
    },
    AddObject {
        handle: ObjectHandle,
        object: Object,
    },
    SetObjectTransform {
        handle: RawObjectHandle,
        transform: Mat4,
    },
    SetSkeletonJointDeltas {
        handle: RawSkeletonHandle,
        joint_matrices: Vec<Mat4>,
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
    DuplicateObject {
        src_handle: ObjectHandle,
        dst_handle: ObjectHandle,
        change: ObjectChange,
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

    pub fn push(&self, kind: InstructionKind, location: Location<'static>) {
        self.producer.lock().push(Instruction { kind, location })
    }
}
