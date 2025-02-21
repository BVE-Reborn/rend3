use std::{mem, panic::Location};

use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{
    trait_supertrait_alias, ObjectChange, PointLight, PointLightChange, RawDirectionalLightHandle,
    RawGraphDataHandleUntyped, RawMaterialHandle, RawMeshHandle, RawPointLightHandle, RawSkeletonHandle,
    RawTexture2DHandle, RawTextureCubeHandle, TextureFromTexture, WasmNotSend, WasmNotSync,
};
use wgpu::{CommandBuffer, Device};

use crate::{
    managers::{GraphStorage, InternalSkeleton, InternalTexture, MaterialManager, TextureManager},
    types::{Camera, DirectionalLight, DirectionalLightChange, Object, RawObjectHandle},
    RendererProfile,
};

trait_supertrait_alias!(pub AddMaterialFillInvoke: FnOnce(&mut MaterialManager, &Device, RendererProfile, &mut TextureManager<crate::types::Texture2DTag>, RawMaterialHandle) + WasmNotSend + WasmNotSync);
trait_supertrait_alias!(pub ChangeMaterialChangeInvoke: FnOnce(&mut MaterialManager, &Device, &TextureManager<crate::types::Texture2DTag>, RawMaterialHandle) + WasmNotSend + WasmNotSync);
trait_supertrait_alias!(pub AddGraphDataAddInvoke: FnOnce(&mut GraphStorage) + WasmNotSend);

/// Instructions are the basic operations performed by the renderer.
/// The user queues them up by making requests of Rend3,
/// and they are all executed at the beginning
/// of each frame render.
pub struct Instruction {
    /// The actual instruction item.
    pub kind: InstructionKind,
    /// This field is not used.
    #[allow(dead_code)]
    pub location: Location<'static>,
}

// None of these need strong handles to the resources, as any
// resource deletions will also be instructions, added after the given instruction is added.
pub enum InstructionKind {
    AddSkeleton {
        handle: RawSkeletonHandle,
        // Boxed for size
        skeleton: Box<InternalSkeleton>,
    },
    AddTexture2D {
        handle: RawTexture2DHandle,
        internal_texture: InternalTexture,
        cmd_buf: Option<CommandBuffer>,
    },
    AddTexture2DFromTexture {
        handle: RawTexture2DHandle,
        texture: TextureFromTexture,
    },
    AddTextureCube {
        handle: RawTextureCubeHandle,
        internal_texture: InternalTexture,
        cmd_buf: Option<CommandBuffer>,
    },
    AddMaterial {
        handle: RawMaterialHandle,
        fill_invoke: Box<dyn AddMaterialFillInvoke>,
    },
    AddObject {
        handle: RawObjectHandle,
        object: Object,
    },
    AddDirectionalLight {
        handle: RawDirectionalLightHandle,
        light: DirectionalLight,
    },
    AddPointLight {
        handle: RawPointLightHandle,
        light: PointLight,
    },
    AddGraphData {
        add_invoke: Box<dyn AddGraphDataAddInvoke>,
    },
    ChangeMaterial {
        handle: RawMaterialHandle,
        change_invoke: Box<dyn ChangeMaterialChangeInvoke>,
    },
    ChangeDirectionalLight {
        handle: RawDirectionalLightHandle,
        change: DirectionalLightChange,
    },
    ChangePointLight {
        handle: RawPointLightHandle,
        change: PointLightChange,
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
    DeletePointLight {
        handle: RawPointLightHandle,
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
        src_handle: RawObjectHandle,
        dst_handle: RawObjectHandle,
        change: ObjectChange,
    },
}

pub struct InstructionStreamPair {
    pub producer: Mutex<Vec<Instruction>>,
    pub consumer: Mutex<Vec<Instruction>>,
}
impl InstructionStreamPair {
    pub fn new() -> Self {
        Self { producer: Mutex::new(Vec::new()), consumer: Mutex::new(Vec::new()) }
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

impl DeletableRawResourceHandle for RawPointLightHandle {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeletePointLight { handle: self }
    }
}

impl DeletableRawResourceHandle for RawGraphDataHandleUntyped {
    fn into_delete_instruction_kind(self) -> InstructionKind {
        InstructionKind::DeleteGraphData { handle: self }
    }
}
