use crate::{
    managers::{MaterialManager, ObjectManager, TextureManager},
    types::{Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Mesh, Object, RawObjectHandle},
    RendererProfile,
};
use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{
    MaterialHandle, MeshHandle, ObjectChange, ObjectHandle, RawDirectionalLightHandle, RawMaterialHandle,
    RawMeshHandle, RawSkeletonHandle, RawTextureHandle, Skeleton, SkeletonHandle, TextureHandle,
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
    AddObject {
        handle: ObjectHandle,
        object: Object,
    },
    AddDirectionalLight {
        handle: DirectionalLightHandle,
        light: DirectionalLight,
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
    ChangeDirectionalLight {
        handle: RawDirectionalLightHandle,
        change: DirectionalLightChange,
    },
    DeleteMesh {
        handle: RawMeshHandle,
    },
    DeleteSkeleton {
        handle: RawSkeletonHandle,
    },
    DeleteTexture {
        handle: RawTextureHandle,
    },
    DeleteMaterial {
        handle: RawMaterialHandle,
    },
    DeleteObject {
        handle: RawObjectHandle,
    },
    DeleteDirectionalLight {
        handle: RawDirectionalLightHandle,
    },
    SetObjectTransform {
        handle: RawObjectHandle,
        transform: Mat4,
    },
    SetSkeletonJointDeltas {
        handle: RawSkeletonHandle,
        joint_matrices: Vec<Mat4>,
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

/// Allows RawResourceHandle<T> to be turned into a delete instruction.
pub(super) trait DeletableRawResourceHandle {
    #[track_caller]
    fn into_delete_instruction_kind(self) -> InstructionKind;
}

impl DeletableRawResourceHandle for RawMeshHandle {
    #[track_caller]
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteMesh { handle: self }
    }
}

impl DeletableRawResourceHandle for RawSkeletonHandle {
    #[track_caller]
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteSkeleton { handle: self }
    }
}

impl DeletableRawResourceHandle for RawTextureHandle {
    #[track_caller]
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteTexture { handle: self }
    }
}

impl DeletableRawResourceHandle for RawMaterialHandle {
    #[track_caller]
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteMaterial { handle: self }
    }
}

impl DeletableRawResourceHandle for RawObjectHandle {
    #[track_caller]
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteObject { handle: self }
    }
}

impl DeletableRawResourceHandle for RawDirectionalLightHandle {
    #[track_caller]
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteDirectionalLight { handle: self }
    }
}
