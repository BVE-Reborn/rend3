use std::{cell::RefCell, sync::Arc};

use crate::{
    graph::{
        DataHandle, GraphResource, PassthroughDataContainer, PassthroughDataRef, PassthroughDataRefMut, ReadyData,
        RenderGraph, RenderGraphDataStore, RenderGraphEncoderOrPass, RenderPassHandle, RenderPassTargets,
        RenderTargetHandle, RpassTemporaryPool, ShadowArrayHandle, ShadowTargetHandle,
    },
    util::typedefs::SsoString,
    Renderer,
};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DeclaredDependency<Handle> {
    pub(super) handle: Handle,
}

#[allow(clippy::type_complexity)]
pub struct RenderGraphNode<'node> {
    pub(super) inputs: Vec<GraphResource>,
    pub(super) outputs: Vec<GraphResource>,
    pub(super) label: SsoString,
    pub(super) rpass: Option<RenderPassTargets>,
    pub(super) passthrough: PassthroughDataContainer<'node>,
    pub(super) exec: Box<
        dyn for<'b, 'pass> FnOnce(
                &mut PassthroughDataContainer<'pass>,
                &Arc<Renderer>,
                RenderGraphEncoderOrPass<'b, 'pass>,
                &'pass RpassTemporaryPool<'pass>,
                &'pass ReadyData,
                RenderGraphDataStore<'pass>,
            ) + 'node,
    >,
}

pub struct RenderGraphNodeBuilder<'a, 'node> {
    pub(super) graph: &'a mut RenderGraph<'node>,
    pub(super) label: SsoString,
    pub(super) inputs: Vec<GraphResource>,
    pub(super) outputs: Vec<GraphResource>,
    pub(super) passthrough: PassthroughDataContainer<'node>,
    pub(super) rpass: Option<RenderPassTargets>,
}
impl<'a, 'node> RenderGraphNodeBuilder<'a, 'node> {
    pub fn add_render_target_input(&mut self, handle: RenderTargetHandle) -> DeclaredDependency<RenderTargetHandle> {
        self.inputs.push(handle.resource);
        DeclaredDependency { handle }
    }

    pub fn add_render_target_output(&mut self, handle: RenderTargetHandle) -> DeclaredDependency<RenderTargetHandle> {
        self.inputs.push(handle.resource);
        self.outputs.push(handle.resource);
        DeclaredDependency { handle }
    }

    pub fn add_optional_render_target_output(
        &mut self,
        handle: Option<RenderTargetHandle>,
    ) -> Option<DeclaredDependency<RenderTargetHandle>> {
        Some(self.add_render_target_output(handle?))
    }

    pub fn add_renderpass(&mut self, targets: RenderPassTargets) -> RenderPassHandle {
        assert!(
            self.rpass.is_none(),
            "Cannot have more than one graph-associated renderpass per node."
        );
        self.rpass = Some(targets);
        RenderPassHandle
    }

    pub fn add_shadow_array_input(&mut self) -> ShadowArrayHandle {
        for i in &self.graph.shadows {
            let resource = GraphResource::Shadow(*i);
            self.inputs.push(resource);
        }
        ShadowArrayHandle
    }

    pub fn add_shadow_output(&mut self, idx: usize) -> ShadowTargetHandle {
        let resource = GraphResource::Shadow(idx);
        self.graph.shadows.insert(idx);
        self.inputs.push(resource);
        self.outputs.push(resource);
        ShadowTargetHandle { idx }
    }

    pub fn add_data_input<T>(&mut self, handle: DataHandle<T>) -> DeclaredDependency<DataHandle<T>>
    where
        T: 'static,
    {
        self.add_data(handle, false)
    }

    pub fn add_data_output<T>(&mut self, handle: DataHandle<T>) -> DeclaredDependency<DataHandle<T>>
    where
        T: 'static,
    {
        self.add_data(handle, true)
    }

    fn add_data<T>(&mut self, handle: DataHandle<T>, output: bool) -> DeclaredDependency<DataHandle<T>>
    where
        T: 'static,
    {
        let idx = match handle.resource {
            GraphResource::Data(idx) => idx,
            _ => panic!("internal rendergraph error: tried to use a non-data resource as data"),
        };

        // TODO: error handling
        // TODO: move this validation to all types
        self.graph
            .data
            .get(idx)
            .expect("internal rendergraph error: cannot find data handle")
            .downcast_ref::<RefCell<Option<T>>>()
            .expect("used custom data that was previously declared with a different type");

        self.inputs.push(handle.resource);
        if output {
            self.outputs.push(handle.resource);
        }
        DeclaredDependency { handle }
    }

    pub fn add_external_output(&mut self) {
        self.inputs.push(GraphResource::External);
        self.outputs.push(GraphResource::External);
    }

    pub fn passthrough_ref<T: 'node>(&mut self, data: &'node T) -> PassthroughDataRef<T> {
        self.passthrough.add_ref(data)
    }

    pub fn passthrough_ref_mut<T: 'node>(&mut self, data: &'node mut T) -> PassthroughDataRefMut<T> {
        self.passthrough.add_ref_mut(data)
    }

    pub fn build<F>(self, exec: F)
    where
        F: for<'b, 'pass> FnOnce(
                &mut PassthroughDataContainer<'pass>,
                &Arc<Renderer>,
                RenderGraphEncoderOrPass<'b, 'pass>,
                &'pass RpassTemporaryPool<'pass>,
                &'pass ReadyData,
                RenderGraphDataStore<'pass>,
            ) + 'node,
    {
        self.graph.nodes.push(RenderGraphNode {
            label: self.label,
            inputs: self.inputs,
            outputs: self.outputs,
            rpass: self.rpass,
            passthrough: self.passthrough,
            exec: Box::new(exec),
        });
    }
}
