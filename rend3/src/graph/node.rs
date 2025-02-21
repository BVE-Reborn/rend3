use std::{cell::RefCell, marker::PhantomData};

use crate::{
    graph::{
        DataHandle, GraphSubResource, InstructionEvaluationOutput, RenderGraph, RenderGraphDataStore,
        RenderGraphEncoderOrPass, RenderPassHandle, RenderPassTargets, RenderTargetHandle, RpassTemporaryPool,
    },
    util::typedefs::SsoString,
    Renderer, RendererDataCore,
};

/// Wraps a handle proving you have declared it as a dependency.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeclaredDependency<Handle> {
    pub(super) handle: Handle,
}

pub struct NodeExecutionContext<'a, 'pass, 'node: 'pass> {
    /// Reference to the renderer the graph is runningon .
    pub renderer: &'a Renderer,
    /// Reference to the renderer data behind a lock.
    pub data_core: &'pass RendererDataCore,
    /// Either the asked-for renderpass or a command encoder.
    pub encoder_or_pass: RenderGraphEncoderOrPass,
    /// Storage for any temporary data that needs to live as long
    /// as the renderpass.
    pub temps: &'pass RpassTemporaryPool<'pass>,
    /// The result of calling evaluate_instructions on the renderer.
    pub eval_output: &'pass InstructionEvaluationOutput,
    /// Store to get data from
    pub graph_data: RenderGraphDataStore<'pass>,
    pub _phantom: PhantomData<&'node ()>,
}

pub(super) struct RenderGraphNode<'node> {
    pub inputs: Vec<GraphSubResource>,
    pub outputs: Vec<GraphSubResource>,
    pub references: Vec<GraphSubResource>,
    pub label: SsoString,
    pub rpass: Option<RenderPassTargets>,
    pub exec: Box<dyn for<'a, 'pass> FnOnce(NodeExecutionContext<'a, 'pass, 'node>) + 'node>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeResourceUsage {
    /// Doesn't access the resource at all, just need access to the resource.
    Reference,
    /// Only reads the resource.
    Input,
    /// Only writes to the resource
    Output,
    /// Reads and writes to the resource.
    InputOutput,
}

/// Builder for a graph node.
///
/// Calling build will automatically add the node to the rendergraph.
pub struct RenderGraphNodeBuilder<'a, 'node> {
    pub(super) graph: &'a mut RenderGraph<'node>,
    pub(super) label: SsoString,
    pub(super) inputs: Vec<GraphSubResource>,
    pub(super) outputs: Vec<GraphSubResource>,
    pub(super) references: Vec<GraphSubResource>,
    pub(super) rpass: Option<RenderPassTargets>,
}
impl<'a, 'node> RenderGraphNodeBuilder<'a, 'node> {
    /// Declares a rendertarget to be read from but not writen to.
    pub fn add_render_target(
        &mut self,
        handle: RenderTargetHandle,
        usage: NodeResourceUsage,
    ) -> DeclaredDependency<RenderTargetHandle> {
        match usage {
            NodeResourceUsage::Reference => self.references.push(handle.resource),
            NodeResourceUsage::Input => self.inputs.push(handle.resource),
            NodeResourceUsage::Output => self.outputs.push(handle.resource),
            NodeResourceUsage::InputOutput => {
                self.inputs.push(handle.resource);
                self.outputs.push(handle.resource)
            }
        }
        DeclaredDependency { handle }
    }

    /// Sugar over [add_render_target] which makes it easy to
    /// declare optional textures.
    ///
    /// [add_render_target]: RenderGraphNodeBuilder::add_render_target
    pub fn add_optional_render_target(
        &mut self,
        handle: Option<RenderTargetHandle>,
        usage: NodeResourceUsage,
    ) -> Option<DeclaredDependency<RenderTargetHandle>> {
        Some(self.add_render_target(handle?, usage))
    }

    /// Declares a renderpass that will be written to. Declaring a renderpass
    /// will prevent access to an encoder in the node.
    pub fn add_renderpass(
        &mut self,
        targets: RenderPassTargets,
        usage: NodeResourceUsage,
    ) -> DeclaredDependency<RenderPassHandle> {
        assert!(self.rpass.is_none(), "Cannot have more than one graph-associated renderpass per node.");
        for targets in &targets.targets {
            self.add_render_target(targets.color, usage);
            self.add_optional_render_target(targets.resolve, usage);
        }
        if let Some(depth_stencil) = &targets.depth_stencil {
            self.add_render_target(depth_stencil.target, usage);
        }
        self.rpass = Some(targets);
        DeclaredDependency { handle: RenderPassHandle }
    }

    /// Declares use of a data handle for reading.
    pub fn add_data<T>(&mut self, handle: DataHandle<T>, usage: NodeResourceUsage) -> DeclaredDependency<DataHandle<T>>
    where
        T: 'static,
    {
        // TODO: error handling
        // TODO: move this validation to all types
        self.graph
            .data
            .get(handle.idx)
            .expect("internal rendergraph error: cannot find data handle")
            .inner
            .downcast_ref::<RefCell<Option<T>>>()
            .expect("used custom data that was previously declared with a different type");

        let subresource = GraphSubResource::Data(handle.idx);
        match usage {
            NodeResourceUsage::Reference => self.references.push(subresource),
            NodeResourceUsage::Input => self.inputs.push(subresource),
            NodeResourceUsage::Output => self.outputs.push(subresource),
            NodeResourceUsage::InputOutput => {
                self.inputs.push(subresource);
                self.outputs.push(subresource)
            }
        }

        DeclaredDependency { handle }
    }

    /// Sugar over [add_data] which makes it easy to
    /// declare optional textures.
    ///
    /// [add_data]: RenderGraphNodeBuilder::add_data
    pub fn add_optional_data<T>(
        &mut self,
        handle: Option<DataHandle<T>>,
        usage: NodeResourceUsage,
    ) -> Option<DeclaredDependency<DataHandle<T>>>
    where
        T: 'static,
    {
        Some(self.add_data(handle?, usage))
    }

    /// Declares a data handle as having the given render targets
    pub fn add_dependencies_to_render_targets<T>(
        &mut self,
        handle: DataHandle<T>,
        render_targets: impl IntoIterator<Item = RenderTargetHandle>,
    ) {
        self.graph
            .data
            .get_mut(handle.idx)
            .expect("internal rendergraph error: cannot find data handle")
            .dependencies
            .extend(render_targets.into_iter().map(|rt| rt.resource));
    }

    /// Declares a data handle as having the given data handles
    pub fn add_dependencies_to_data<T, U>(
        &mut self,
        handle: DataHandle<T>,
        render_targets: impl IntoIterator<Item = DataHandle<U>>,
    ) {
        self.graph
            .data
            .get_mut(handle.idx)
            .expect("internal rendergraph error: cannot find data handle")
            .dependencies
            .extend(render_targets.into_iter().map(|hdl| GraphSubResource::Data(hdl.idx)));
    }

    /// Declares that this node has some unknowable side effect, so can't be removed.
    pub fn add_side_effect(&mut self) {
        self.inputs.push(GraphSubResource::External);
        self.outputs.push(GraphSubResource::External);
    }

    /// Builds the rendergraph node and adds it into the rendergraph.
    ///
    /// Takes a function that is the body of the node. Nodes will only run if a
    /// following node consumes the output. See module level docs for more
    /// details.
    pub fn build<F>(self, exec: F)
    where
        F: for<'b, 'pass> FnOnce(NodeExecutionContext<'b, 'pass, 'node>) + 'node,
    {
        self.graph.nodes.push(RenderGraphNode {
            label: self.label,
            inputs: self.inputs,
            outputs: self.outputs,
            references: self.references,
            rpass: self.rpass,
            exec: Box::new(exec),
        });
    }
}
