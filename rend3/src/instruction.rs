use crate::{
    managers::{GraphStorage, MaterialManager, TextureManager},
    types::{Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Mesh, Object, RawObjectHandle},
    RendererProfile,
};
use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{
    MaterialHandle, MeshHandle, ObjectChange, ObjectHandle, RawDirectionalLightHandle, RawGraphDataHandleUntyped,
    RawMaterialHandle, RawMeshHandle, RawSkeletonHandle, RawTexture2DHandle, RawTextureCubeHandle, Skeleton,
    SkeletonHandle, Texture2DHandle, TextureCubeHandle,
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
    AddTexture2D {
        handle: Texture2DHandle,
        desc: TextureDescriptor<'static>,
        texture: Texture,
        view: TextureView,
        buffer: Option<CommandBuffer>,
    },
    AddTextureCube {
        handle: TextureCubeHandle,
        desc: TextureDescriptor<'static>,
        texture: Texture,
        view: TextureView,
        buffer: Option<CommandBuffer>,
    },
    AddMaterial {
        handle: MaterialHandle,
        fill_invoke: Box<
            dyn FnOnce(
                    &mut MaterialManager,
                    &Device,
                    RendererProfile,
                    &mut TextureManager<crate::types::Texture2DTag>,
                    &MaterialHandle,
                ) + Send
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
    AddGraphData {
        add_invoke: Box<dyn FnOnce(&mut GraphStorage) + Send>,
    },
    ChangeMaterial {
        handle: MaterialHandle,
        change_invoke: Box<
            dyn FnOnce(&mut MaterialManager, &Device, &TextureManager<crate::types::Texture2DTag>, &MaterialHandle)
                + Send
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
    DeleteTexture2D {
        handle: RawTexture2DHandle,
    },
    DeleteTextureCube {
        handle: RawTextureCubeHandle,
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
    DeleteGraphData {
        handle: RawGraphDataHandleUntyped,
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
    fn into_delete_instruction_kind(self) -> InstructionKind;
}

impl DeletableRawResourceHandle for RawMeshHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteMesh { handle: self }
    }
}

impl DeletableRawResourceHandle for RawSkeletonHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteSkeleton { handle: self }
    }
}

impl DeletableRawResourceHandle for RawTexture2DHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteTexture2D { handle: self }
    }
}

impl DeletableRawResourceHandle for RawTextureCubeHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteTextureCube { handle: self }
    }
}

impl DeletableRawResourceHandle for RawMaterialHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteMaterial { handle: self }
    }
}

impl DeletableRawResourceHandle for RawObjectHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteObject { handle: self }
    }
}

impl DeletableRawResourceHandle for RawDirectionalLightHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteDirectionalLight { handle: self }
    }
}

impl DeletableRawResourceHandle for RawGraphDataHandleUntyped {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteGraphData { handle: self }
    }
}
