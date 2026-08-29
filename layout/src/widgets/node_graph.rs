//! Interactive node graph editor widget.
//!
//! Provides the [`NodeGraph`] widget for building visual node-based editors
//! (e.g. shader graphs, data-flow pipelines). Key types:
//!
//! - [`NodeGraph`] — top-level widget holding nodes, types, and callbacks
//! - [`Node`] — a single node with typed input/output connections and editable fields
//! - [`NodeTypeInfo`] / [`InputOutputInfo`] — metadata describing node types and their I/O ports
//! - [`NodeGraphCallbacks`] — user-provided callbacks for add, remove, drag, connect, etc.
//!
//! **Known limitation:** Connection curves between nodes are currently not rendered
//! (`draw_connection` returns a null image pending `RenderImageCallbackInfo` support).

use alloc::vec::Vec;
use core::fmt;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    geom::{LogicalPosition, LogicalRect, LogicalSize, PhysicalSizeU32},
    gl::Texture,
    menu::{Menu, MenuItem, StringMenuItem},
    refany::{OptionRefAny, RefAny},
    resources::{ImageRef, RawImageFormat},
    svg::{SvgPath, SvgPathElement, SvgStrokeStyle, TessellatedGPUSvgNode},
    window::CursorPosition::InWindow,
};
use azul_css::css::BoxOrStatic;
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState},
        color_input::{ColorInput, ColorInputOnValueChangeCallbackType, ColorInputState},
        file_input::{FileInput, FileInputOnPathChangeCallbackType, FileInputState},
        number_input::{NumberInput, NumberInputOnFocusLostCallbackType, NumberInputState},
        text_input::{TextInput, TextInputOnFocusLostCallbackType, TextInputState},
    },
};

/// Interactive node graph editor widget with typed input/output connections.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeGraph {
    pub node_types: NodeTypeIdInfoMapVec,
    pub input_output_types: InputOutputTypeIdInfoMapVec,
    pub nodes: NodeIdNodeMapVec,
    pub allow_multiple_root_nodes: bool,
    pub offset: LogicalPosition,
    pub style: NodeGraphStyle,
    pub callbacks: NodeGraphCallbacks,
    pub add_node_str: AzString,
    pub scale_factor: f32,
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            node_types: NodeTypeIdInfoMapVec::from_const_slice(&[]),
            input_output_types: InputOutputTypeIdInfoMapVec::from_const_slice(&[]),
            nodes: NodeIdNodeMapVec::from_const_slice(&[]),
            allow_multiple_root_nodes: false,
            offset: LogicalPosition::zero(),
            style: NodeGraphStyle::Default,
            callbacks: NodeGraphCallbacks::default(),
            add_node_str: AzString::from_const_str(""),
            scale_factor: 1.0,
        }
    }
}

impl NodeGraph {
    /// Generates a new `NodeId` that is unique in the graph
    #[must_use]
    pub fn generate_unique_node_id(&self) -> NodeGraphNodeId {
        NodeGraphNodeId {
            inner: self
                .nodes
                .iter()
                .map(|i| i.node_id.inner)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        }
    }
}

/// Maps a [`NodeTypeId`] to its [`NodeTypeInfo`] metadata.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeTypeIdInfoMap {
    pub node_type_id: NodeTypeId,
    pub node_type_info: NodeTypeInfo,
}

impl_option!(
    NodeTypeIdInfoMap,
    OptionNodeTypeIdInfoMap,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    NodeTypeIdInfoMap,
    NodeTypeIdInfoMapVec,
    NodeTypeIdInfoMapVecDestructor,
    NodeTypeIdInfoMapVecDestructorType,
    NodeTypeIdInfoMapVecSlice,
    OptionNodeTypeIdInfoMap
);
impl_vec_clone!(
    NodeTypeIdInfoMap,
    NodeTypeIdInfoMapVec,
    NodeTypeIdInfoMapVecDestructor
);
impl_vec_mut!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec);
impl_vec_debug!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec);

/// Maps an [`InputOutputTypeId`] to its [`InputOutputInfo`] metadata.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputOutputTypeIdInfoMap {
    pub io_type_id: InputOutputTypeId,
    pub io_info: InputOutputInfo,
}

impl_option!(
    InputOutputTypeIdInfoMap,
    OptionInputOutputTypeIdInfoMap,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    InputOutputTypeIdInfoMap,
    InputOutputTypeIdInfoMapVec,
    InputOutputTypeIdInfoMapVecDestructor,
    InputOutputTypeIdInfoMapVecDestructorType,
    InputOutputTypeIdInfoMapVecSlice,
    OptionInputOutputTypeIdInfoMap
);
impl_vec_clone!(
    InputOutputTypeIdInfoMap,
    InputOutputTypeIdInfoMapVec,
    InputOutputTypeIdInfoMapVecDestructor
);
impl_vec_mut!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec);
impl_vec_debug!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec);

/// Maps a [`NodeGraphNodeId`] to its [`Node`] data.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeIdNodeMap {
    pub node_id: NodeGraphNodeId,
    pub node: Node,
}

impl_option!(
    NodeIdNodeMap,
    OptionNodeIdNodeMap,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    NodeIdNodeMap,
    NodeIdNodeMapVec,
    NodeIdNodeMapVecDestructor,
    NodeIdNodeMapVecDestructorType,
    NodeIdNodeMapVecSlice,
    OptionNodeIdNodeMap
);
impl_vec_clone!(NodeIdNodeMap, NodeIdNodeMapVec, NodeIdNodeMapVecDestructor);
impl_vec_mut!(NodeIdNodeMap, NodeIdNodeMapVec);
impl_vec_debug!(NodeIdNodeMap, NodeIdNodeMapVec);

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub enum NodeGraphStyle {
    Default,
    // to be extended
}

/// User-provided callbacks for node graph interaction events.
#[derive(Default, Debug, Clone)]
#[repr(C)]
pub struct NodeGraphCallbacks {
    pub on_node_added: OptionOnNodeAdded,
    pub on_node_removed: OptionOnNodeRemoved,
    pub on_node_dragged: OptionOnNodeDragged,
    pub on_node_graph_dragged: OptionOnNodeGraphDragged,
    pub on_node_connected: OptionOnNodeConnected,
    pub on_node_input_disconnected: OptionOnNodeInputDisconnected,
    pub on_node_output_disconnected: OptionOnNodeOutputDisconnected,
    pub on_node_field_edited: OptionOnNodeFieldEdited,
}

pub type OnNodeAddedCallbackType = extern "C" fn(
    refany: RefAny,
    info: CallbackInfo,
    new_node_type: NodeTypeId,
    new_node_id: NodeGraphNodeId,
    new_node_position: NodeGraphNodePosition,
) -> Update;
impl_widget_callback!(
    OnNodeAdded,
    OptionOnNodeAdded,
    OnNodeAddedCallback,
    OnNodeAddedCallbackType
);

pub type OnNodeRemovedCallbackType =
    extern "C" fn(refany: RefAny, info: CallbackInfo, node_id_to_remove: NodeGraphNodeId) -> Update;
impl_widget_callback!(
    OnNodeRemoved,
    OptionOnNodeRemoved,
    OnNodeRemovedCallback,
    OnNodeRemovedCallbackType
);

pub type OnNodeGraphDraggedCallbackType =
    extern "C" fn(refany: RefAny, info: CallbackInfo, drag_amount: GraphDragAmount) -> Update;
impl_widget_callback!(
    OnNodeGraphDragged,
    OptionOnNodeGraphDragged,
    OnNodeGraphDraggedCallback,
    OnNodeGraphDraggedCallbackType
);

pub type OnNodeDraggedCallbackType = extern "C" fn(
    refany: RefAny,
    info: CallbackInfo,
    node_dragged: NodeGraphNodeId,
    drag_amount: NodeDragAmount,
) -> Update;
impl_widget_callback!(
    OnNodeDragged,
    OptionOnNodeDragged,
    OnNodeDraggedCallback,
    OnNodeDraggedCallbackType
);

pub type OnNodeConnectedCallbackType = extern "C" fn(
    refany: RefAny,
    info: CallbackInfo,
    input: NodeGraphNodeId,
    input_index: usize,
    output: NodeGraphNodeId,
    output_index: usize,
) -> Update;
impl_widget_callback!(
    OnNodeConnected,
    OptionOnNodeConnected,
    OnNodeConnectedCallback,
    OnNodeConnectedCallbackType
);

pub type OnNodeInputDisconnectedCallbackType = extern "C" fn(
    refany: RefAny,
    info: CallbackInfo,
    input: NodeGraphNodeId,
    input_index: usize,
) -> Update;
impl_widget_callback!(
    OnNodeInputDisconnected,
    OptionOnNodeInputDisconnected,
    OnNodeInputDisconnectedCallback,
    OnNodeInputDisconnectedCallbackType
);

pub type OnNodeOutputDisconnectedCallbackType = extern "C" fn(
    refany: RefAny,
    info: CallbackInfo,
    output: NodeGraphNodeId,
    output_index: usize,
) -> Update;
impl_widget_callback!(
    OnNodeOutputDisconnected,
    OptionOnNodeOutputDisconnected,
    OnNodeOutputDisconnectedCallback,
    OnNodeOutputDisconnectedCallbackType
);

pub type OnNodeFieldEditedCallbackType = extern "C" fn(
    refany: RefAny,
    info: CallbackInfo,
    node_id: NodeGraphNodeId,
    field_id: usize,
    node_type: NodeTypeId,
    new_value: NodeTypeFieldValue,
) -> Update;
impl_widget_callback!(
    OnNodeFieldEdited,
    OptionOnNodeFieldEdited,
    OnNodeFieldEditedCallback,
    OnNodeFieldEditedCallbackType
);

/// Unique identifier for an input/output port type.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct InputOutputTypeId {
    pub inner: u64,
}

impl_option!(
    InputOutputTypeId,
    OptionInputOutputTypeId,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(
    InputOutputTypeId,
    InputOutputTypeIdVec,
    InputOutputTypeIdVecDestructor,
    InputOutputTypeIdVecDestructorType,
    InputOutputTypeIdVecSlice,
    OptionInputOutputTypeId
);
impl_vec_clone!(
    InputOutputTypeId,
    InputOutputTypeIdVec,
    InputOutputTypeIdVecDestructor
);
impl_vec_mut!(InputOutputTypeId, InputOutputTypeIdVec);
impl_vec_debug!(InputOutputTypeId, InputOutputTypeIdVec);

/// Unique identifier for a node type (e.g. "Add", "Multiply").
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NodeTypeId {
    pub inner: u64,
}

/// Unique identifier for a node instance within the graph.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NodeGraphNodeId {
    pub inner: u64,
}

/// A single node with typed input/output connections and editable fields.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Node {
    pub node_type: NodeTypeId,
    pub position: NodeGraphNodePosition,
    pub fields: NodeTypeFieldVec,
    pub connect_in: InputConnectionVec,
    pub connect_out: OutputConnectionVec,
}

/// A key-value field on a node (e.g. a text input labelled "Name").
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeTypeField {
    pub key: AzString,
    pub value: NodeTypeFieldValue,
}

impl_option!(
    NodeTypeField,
    OptionNodeTypeField,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    NodeTypeField,
    NodeTypeFieldVec,
    NodeTypeFieldVecDestructor,
    NodeTypeFieldVecDestructorType,
    NodeTypeFieldVecSlice,
    OptionNodeTypeField
);
impl_vec_clone!(NodeTypeField, NodeTypeFieldVec, NodeTypeFieldVecDestructor);
impl_vec_debug!(NodeTypeField, NodeTypeFieldVec);
impl_vec_mut!(NodeTypeField, NodeTypeFieldVec);

/// The value of a node field, determining which widget is rendered.
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum NodeTypeFieldValue {
    TextInput(AzString),
    NumberInput(f32),
    CheckBox(bool),
    ColorInput(ColorU),
    FileInput(OptionString),
}

/// An input port's connections to one or more output ports on other nodes.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputConnection {
    pub input_index: usize,
    pub connects_to: OutputNodeAndIndexVec,
}

impl_option!(
    InputConnection,
    OptionInputConnection,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    InputConnection,
    InputConnectionVec,
    InputConnectionVecDestructor,
    InputConnectionVecDestructorType,
    InputConnectionVecSlice,
    OptionInputConnection
);
impl_vec_clone!(
    InputConnection,
    InputConnectionVec,
    InputConnectionVecDestructor
);
impl_vec_debug!(InputConnection, InputConnectionVec);
impl_vec_mut!(InputConnection, InputConnectionVec);

/// Reference to a specific output port on a node.
#[derive(Copy, Debug, Clone)]
#[repr(C)]
pub struct OutputNodeAndIndex {
    pub node_id: NodeGraphNodeId,
    pub output_index: usize,
}

impl_option!(
    OutputNodeAndIndex,
    OptionOutputNodeAndIndex,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    OutputNodeAndIndex,
    OutputNodeAndIndexVec,
    OutputNodeAndIndexVecDestructor,
    OutputNodeAndIndexVecDestructorType,
    OutputNodeAndIndexVecSlice,
    OptionOutputNodeAndIndex
);
impl_vec_clone!(
    OutputNodeAndIndex,
    OutputNodeAndIndexVec,
    OutputNodeAndIndexVecDestructor
);
impl_vec_debug!(OutputNodeAndIndex, OutputNodeAndIndexVec);
impl_vec_mut!(OutputNodeAndIndex, OutputNodeAndIndexVec);

/// An output port's connections to one or more input ports on other nodes.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct OutputConnection {
    pub output_index: usize,
    pub connects_to: InputNodeAndIndexVec,
}

impl_option!(
    OutputConnection,
    OptionOutputConnection,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    OutputConnection,
    OutputConnectionVec,
    OutputConnectionVecDestructor,
    OutputConnectionVecDestructorType,
    OutputConnectionVecSlice,
    OptionOutputConnection
);
impl_vec_clone!(
    OutputConnection,
    OutputConnectionVec,
    OutputConnectionVecDestructor
);
impl_vec_debug!(OutputConnection, OutputConnectionVec);
impl_vec_mut!(OutputConnection, OutputConnectionVec);

/// Reference to a specific input port on a node.
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct InputNodeAndIndex {
    pub node_id: NodeGraphNodeId,
    pub input_index: usize,
}

impl_option!(
    InputNodeAndIndex,
    OptionInputNodeAndIndex,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    InputNodeAndIndex,
    InputNodeAndIndexVec,
    InputNodeAndIndexVecDestructor,
    InputNodeAndIndexVecDestructorType,
    InputNodeAndIndexVecSlice,
    OptionInputNodeAndIndex
);
impl_vec_clone!(
    InputNodeAndIndex,
    InputNodeAndIndexVec,
    InputNodeAndIndexVecDestructor
);
impl_vec_debug!(InputNodeAndIndex, InputNodeAndIndexVec);
impl_vec_mut!(InputNodeAndIndex, InputNodeAndIndexVec);

/// Metadata describing a node type and its I/O port configuration.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeTypeInfo {
    /// Whether this node type is a "root" type
    pub is_root: bool,
    /// Name of the node type
    pub node_type_name: AzString,
    /// List of inputs for this node
    pub inputs: InputOutputTypeIdVec,
    /// List of outputs for this node
    pub outputs: InputOutputTypeIdVec,
}

/// Display metadata for an input/output port type (name and color).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputOutputInfo {
    /// Data type of this input / output
    pub data_type: AzString,
    /// Which color to use for the input / output
    pub color: ColorU,
}

/// Things only relevant to the display of the node in an interactive editor
/// - such as x and y position in the node graph, name, etc.
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct NodeGraphNodePosition {
    /// X Position of the node
    pub x: f32,
    /// Y Position of the node
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum NodeGraphError {
    /// MIME type is not the same (for example: connection "spatialdata/point"
    /// with a node that expects "spatialdata/line")
    NodeMimeTypeMismatch,
    /// Invalid index when accessing a node in / output
    NodeInvalidIndex,
    /// The in-/ output matching encountered a non-existing hash to a node that doesn't exist
    NodeInvalidNode,
    /// Root node is missing from the graph tree
    NoRootNode,
}

impl fmt::Display for NodeGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::NodeGraphError::{
            NoRootNode, NodeInvalidIndex, NodeInvalidNode, NodeMimeTypeMismatch,
        };
        match self {
            NodeMimeTypeMismatch => write!(f, "MIME type mismatch"),
            NodeInvalidIndex => write!(f, "Invalid node index"),
            NodeInvalidNode => write!(f, "Invalid node"),
            NoRootNode => write!(f, "No root node found"),
        }
    }
}

/// Amount (in logical pixels) the entire graph was dragged.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct GraphDragAmount {
    pub x: f32,
    pub y: f32,
}

/// Amount (in logical pixels) a single node was dragged.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct NodeDragAmount {
    pub x: f32,
    pub y: f32,
}

impl NodeGraph {
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        ::core::mem::swap(&mut default, self);
        default
    }

    /// Connects the current nodes input with another nodes output
    ///
    /// ## Inputs
    ///
    /// - `output_node_id`: The ID of the output node (index in the `NodeGraphs` internal `BTree`)
    /// - `output_index`: The index of the output *on the output node*
    /// - `input_node_id`: Same as `output_node_id`, but for the input node
    /// - `input_index`: Same as `output_index`, but for the input node
    ///
    /// ## Returns
    ///
    /// One of:
    ///
    /// - `NodeGraphError::NodeInvalidNode`: One of the input nodes does not exist
    /// - `NodeGraphError::NodeInvalidIndex`: One node has an invalid `output` or `input` index
    /// - `NodeGraphError::NodeMimeTypeMismatch`: The types of two connected `outputs` and `inputs`
    ///   aren't the same
    /// - `Ok(())`: The connection was established successfully.
    fn connect_input_output(
        &mut self,
        input_node_id: NodeGraphNodeId,
        input_index: usize,
        output_node_id: NodeGraphNodeId,
        output_index: usize,
    ) -> Result<(), NodeGraphError> {
        // Verify that the node type of the connection matches
        self.verify_nodetype_match(output_node_id, output_index, input_node_id, input_index)?;

        // connect input -> output
        if let Some(input_node) = self
            .nodes
            .as_mut()
            .iter_mut()
            .find(|i| i.node_id == input_node_id)
        {
            if let Some(position) = input_node
                .node
                .connect_in
                .as_ref()
                .iter()
                .position(|i| i.input_index == input_index)
            {
                input_node.node.connect_in.as_mut()[position]
                    .connects_to
                    .push(OutputNodeAndIndex {
                        node_id: output_node_id,
                        output_index,
                    });
            } else {
                input_node.node.connect_in.push(InputConnection {
                    input_index,
                    connects_to: vec![OutputNodeAndIndex {
                        node_id: output_node_id,
                        output_index,
                    }]
                    .into(),
                });
            }
        } else {
            return Err(NodeGraphError::NodeInvalidNode);
        }

        // connect output -> input
        if let Some(output_node) = self
            .nodes
            .as_mut()
            .iter_mut()
            .find(|i| i.node_id == output_node_id)
        {
            if let Some(position) = output_node
                .node
                .connect_out
                .as_ref()
                .iter()
                .position(|i| i.output_index == output_index)
            {
                output_node.node.connect_out.as_mut()[position]
                    .connects_to
                    .push(InputNodeAndIndex {
                        node_id: input_node_id,
                        input_index,
                    });
            } else {
                output_node.node.connect_out.push(OutputConnection {
                    output_index,
                    connects_to: vec![InputNodeAndIndex {
                        node_id: input_node_id,
                        input_index,
                    }]
                    .into(),
                });
            }
        } else {
            return Err(NodeGraphError::NodeInvalidNode);
        }

        Ok(())
    }

    /// Disconnect an input if it is connected to an output
    ///
    /// # Inputs
    ///
    /// - `input_node_id`: The ID of the input node (index in the `NodeGraphs` internal `BTree`)
    /// - `input_index`: The index of the input *on the input node*
    ///
    /// # Returns
    ///
    /// - `Err(NodeGraphError::NodeInvalidNode)`: The node at index `input_node_id` does not
    ///   exist
    /// - `Err(NodeGraphError::NodeInvalidIndex)`: One node has an invalid `input` or `output`
    ///   index
    /// - `Err(NodeGraphError::NodeMimeTypeMismatch)`: The types of two connected `input` and
    ///   `output` do not match
    /// - `Ok(())`: The disconnection completed successfully.
    fn disconnect_input(
        &mut self,
        input_node_id: NodeGraphNodeId,
        input_index: usize,
    ) -> Result<(), NodeGraphError> {
        let output_connections = {
            let input_node = self
                .nodes
                .as_ref()
                .iter()
                .find(|i| i.node_id == input_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;

            match input_node
                .node
                .connect_in
                .iter()
                .find(|i| i.input_index == input_index)
            {
                None => return Ok(()),
                Some(s) => s.connects_to.clone(),
            }
        };

        // for every output that this input was connected to...
        for OutputNodeAndIndex {
            node_id,
            output_index,
        } in output_connections.as_ref()
        {
            let output_node_id = *node_id;
            let output_index = *output_index;

            // verify that the node type of the connection matches
            self.verify_nodetype_match(output_node_id, output_index, input_node_id, input_index)?;

            // disconnect input -> output

            if let Some(input_node) = self
                .nodes
                .as_mut()
                .iter_mut()
                .find(|i| i.node_id == input_node_id)
            {
                if let Some(position) = input_node
                    .node
                    .connect_in
                    .iter()
                    .position(|i| i.input_index == input_index)
                {
                    input_node.node.connect_in.remove(position);
                }
            } else {
                return Err(NodeGraphError::NodeInvalidNode);
            }

            if let Some(output_node) = self
                .nodes
                .as_mut()
                .iter_mut()
                .find(|i| i.node_id == output_node_id)
            {
                if let Some(position) = output_node
                    .node
                    .connect_out
                    .iter()
                    .position(|i| i.output_index == output_index)
                {
                    output_node.node.connect_out.remove(position);
                }
            } else {
                return Err(NodeGraphError::NodeInvalidNode);
            }
        }

        Ok(())
    }

    /// Disconnect an output if it is connected to an input
    ///
    /// # Inputs
    ///
    /// - `output_node_id`: The ID of the output node (index in the `NodeGraphs` internal `BTree`)
    /// - `output_index`: The index of the output *on the output node*
    ///
    /// # Returns
    ///
    /// - `Err(NodeGraphError::NodeInvalidNode)`: The node at index `output_node_id` does not exist
    /// - `Err(NodeGraphError::NodeInvalidIndex)`: One node has an invalid `input` or `output` index
    /// - `Err(NodeGraphError::NodeMimeTypeMismatch)`: The types of two connected `input` and
    ///   `output` do not match
    /// - `Ok(())`: The disconnection completed successfully.
    fn disconnect_output(
        &mut self,
        output_node_id: NodeGraphNodeId,
        output_index: usize,
    ) -> Result<(), NodeGraphError> {
        let input_connections = {
            let output_node = self
                .nodes
                .as_ref()
                .iter()
                .find(|i| i.node_id == output_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;

            match output_node
                .node
                .connect_out
                .iter()
                .find(|i| i.output_index == output_index)
            {
                None => return Ok(()),
                Some(s) => s.connects_to.clone(),
            }
        };

        for InputNodeAndIndex {
            node_id,
            input_index,
        } in &input_connections
        {
            let input_node_id = *node_id;
            let input_index = *input_index;

            // verify that the node type of the connection matches
            self.verify_nodetype_match(output_node_id, output_index, input_node_id, input_index)?;

            if let Some(output_node) = self
                .nodes
                .as_mut()
                .iter_mut()
                .find(|i| i.node_id == output_node_id)
            {
                if let Some(position) = output_node
                    .node
                    .connect_out
                    .iter()
                    .position(|i| i.output_index == output_index)
                {
                    output_node.node.connect_out.remove(position);
                }
            } else {
                return Err(NodeGraphError::NodeInvalidNode);
            }

            if let Some(input_node) = self
                .nodes
                .as_mut()
                .iter_mut()
                .find(|i| i.node_id == input_node_id)
            {
                if let Some(position) = input_node
                    .node
                    .connect_in
                    .iter()
                    .position(|i| i.input_index == input_index)
                {
                    input_node.node.connect_in.remove(position);
                }
            } else {
                return Err(NodeGraphError::NodeInvalidNode);
            }
        }

        Ok(())
    }

    /// Verifies that the node types of two connections match
    fn verify_nodetype_match(
        &self,
        output_node_id: NodeGraphNodeId,
        output_index: usize,
        input_node_id: NodeGraphNodeId,
        input_index: usize,
    ) -> Result<(), NodeGraphError> {
        let output_node = self
            .nodes
            .iter()
            .find(|i| i.node_id == output_node_id)
            .ok_or(NodeGraphError::NodeInvalidNode)?;

        let output_node_type = self
            .node_types
            .iter()
            .find(|i| i.node_type_id == output_node.node.node_type)
            .ok_or(NodeGraphError::NodeInvalidNode)?;

        let output_type = output_node_type
            .node_type_info
            .outputs
            .as_ref()
            .get(output_index)
            .copied()
            .ok_or(NodeGraphError::NodeInvalidIndex)?;

        let input_node = self
            .nodes
            .iter()
            .find(|i| i.node_id == input_node_id)
            .ok_or(NodeGraphError::NodeInvalidNode)?;

        let input_node_type = self
            .node_types
            .iter()
            .find(|i| i.node_type_id == input_node.node.node_type)
            .ok_or(NodeGraphError::NodeInvalidNode)?;

        let input_type = input_node_type
            .node_type_info
            .inputs
            .as_ref()
            .get(input_index)
            .copied()
            .ok_or(NodeGraphError::NodeInvalidIndex)?;

        // Input / Output do not have the same TypeId
        if input_type != output_type {
            return Err(NodeGraphError::NodeMimeTypeMismatch);
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use]
    pub fn dom(self) -> Dom {
        static NODEGRAPH_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("nodegraph"))];

        static NODEGRAPH_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Image(
            AzString::from_const_str("nodegraph-background"),
        )];

        static NODEGRAPH_NODES_CONTAINER_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("nodegraph-nodes-container"))];

        static NODEGRAPH_NODES_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
            CssPropertyWithConditions::simple(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
            CssPropertyWithConditions::simple(CssProperty::position(LayoutPosition::Absolute)),
        ];

        let nodegraph_wrapper_props = vec![
            CssPropertyWithConditions::simple(CssProperty::overflow_x(LayoutOverflow::Hidden)),
            CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Hidden)),
            CssPropertyWithConditions::simple(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
            CssPropertyWithConditions::simple(CssProperty::background_content(
                StyleBackgroundContentVec::from_const_slice(NODEGRAPH_BACKGROUND),
            )),
            CssPropertyWithConditions::simple(CssProperty::background_repeat(
                vec![StyleBackgroundRepeat::PatternRepeat].into(),
            )),
            CssPropertyWithConditions::simple(CssProperty::background_position(
                vec![StyleBackgroundPosition {
                    horizontal: BackgroundPositionHorizontal::Exact(PixelValue::const_px(0)),
                    vertical: BackgroundPositionVertical::Exact(PixelValue::const_px(0)),
                }]
                .into(),
            )),
        ];

        let nodegraph_props = vec![
            CssPropertyWithConditions::simple(CssProperty::overflow_x(LayoutOverflow::Hidden)),
            CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Hidden)),
            CssPropertyWithConditions::simple(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
            CssPropertyWithConditions::simple(CssProperty::position(LayoutPosition::Relative)),
        ];

        let marker_prefix = next_marker_prefix();
        let connections_marker: AzString =
            format!("{}-connections", marker_prefix.as_str()).into();

        let node_graph_local_dataset = RefAny::new(NodeGraphLocalDataset {
            node_graph: self.clone(), // TODO: expensive
            last_input_or_output_clicked: None,
            active_node_being_dragged: None,
            marker_prefix: marker_prefix.clone(),
            callbacks: self.callbacks.clone(),
        });

        let context_menu = Menu::create(
            vec![MenuItem::String(
                StringMenuItem::create(self.add_node_str.clone()).with_children(
                    self.node_types
                        .iter()
                        .map(
                            |NodeTypeIdInfoMap {
                                 node_type_id,
                                 node_type_info,
                             }| {
                                let context_menu_local_dataset =
                                    RefAny::new(ContextMenuEntryLocalDataset {
                                        node_type: *node_type_id,
                                        // RefAny<NodeGraphLocalDataset>
                                        backref: node_graph_local_dataset.clone(),
                                    });

                                MenuItem::String(
                                    StringMenuItem::create(node_type_info.node_type_name.clone())
                                        .with_callback(
                                            context_menu_local_dataset,
                                            nodegraph_context_menu_click as usize,
                                        ),
                                )
                            },
                        )
                        .collect::<Vec<_>>()
                        .into(),
                ),
            )]
            .into(),
        );

        Dom::create_div()
            .with_css_props(nodegraph_wrapper_props.into())
            .with_context_menu(context_menu)
            .with_children(
                vec![Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(NODEGRAPH_CLASS))
                    .with_css_props(nodegraph_props.into())
                    .with_callbacks(
                        vec![
                            CoreCallbackData {
                                event: EventFilter::Hover(HoverEventFilter::MouseOver),
                                refany: node_graph_local_dataset.clone(),
                                callback: CoreCallback {
                                    cb: nodegraph_drag_graph_or_nodes as usize,
                                    ctx: OptionRefAny::None,
                                },
                            },
                            CoreCallbackData {
                                event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                                refany: node_graph_local_dataset.clone(),
                                callback: CoreCallback {
                                    cb: nodegraph_unset_active_node as usize,
                                    ctx: OptionRefAny::None,
                                },
                            },
                        ]
                        .into(),
                    )
                    .with_children({
                        vec![
                            // connections
                            render_connections(&self, connections_marker),
                            // nodes
                            self.nodes
                                .iter()
                                .filter_map(|NodeIdNodeMap { node_id, node }| {
                                    let node_type_info = self
                                        .node_types
                                        .iter()
                                        .find(|i| i.node_type_id == node.node_type)?;
                                    let node_local_dataset = NodeLocalDataset {
                                        node_id: *node_id,
                                        backref: node_graph_local_dataset.clone(),
                                    };

                                    Some(render_node(
                                        node,
                                        (self.offset.x, self.offset.y),
                                        &node_type_info.node_type_info,
                                        node_local_dataset,
                                        self.scale_factor,
                                        node_marker(&marker_prefix, *node_id),
                                    ))
                                })
                                .collect::<Dom>()
                                .with_ids_and_classes(IdOrClassVec::from_const_slice(
                                    NODEGRAPH_NODES_CONTAINER_CLASS,
                                ))
                                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                                    NODEGRAPH_NODES_CONTAINER_PROPS,
                                )),
                        ]
                        .into()
                    })]
                .into(),
            )
            .with_dataset(Some(node_graph_local_dataset).into())
            // The wrapper's marker: what the context-menu callback resolves
            // to place a new node relative to this graph's on-screen box.
            .with_marker(Some(marker_prefix).into())
    }
}

/// Process-unique prefix for one `NodeGraph::dom()` render's marker strings.
/// The prefix (and every marker derived from it) is rebuilt together with the
/// dataset that stores it on each `layout()` pass, so lookups made through
/// the live dataset always match the live DOM.
fn next_marker_prefix() -> AzString {
    use core::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("azul-nodegraph-{:x}", NEXT.fetch_add(1, Ordering::Relaxed)).into()
}

/// Marker string of one visual node: `{prefix}-node-{id}`.
fn node_marker(prefix: &AzString, node_id: NodeGraphNodeId) -> AzString {
    format!("{}-node-{:x}", prefix.as_str(), node_id.inner).into()
}

// dataset set on the top-level nodegraph node,
// containing all the state of the node graph
struct NodeGraphLocalDataset {
    node_graph: NodeGraph,
    last_input_or_output_clicked: Option<(NodeGraphNodeId, InputOrOutput)>,
    /// The graph node currently being dragged. Its VISUAL node is resolved
    /// via `node_marker(&marker_prefix, id)` — no dataset handle needed.
    active_node_being_dragged: Option<NodeGraphNodeId>,
    /// Prefix of this render's marker strings (see `next_marker_prefix`):
    /// the wrapper carries `{prefix}`, the connections container
    /// `{prefix}-connections`, each visual node `{prefix}-node-{id}`.
    /// Replaces the old empty-marker-dataset type searches
    /// (`get_node_id_of_root_dataset`), which were ambiguous the moment two
    /// datasets of one type existed.
    marker_prefix: AzString,
    callbacks: NodeGraphCallbacks,
}

struct ContextMenuEntryLocalDataset {
    node_type: NodeTypeId,
    backref: RefAny, // RefAny<NodeGraphLocalDataset>
}

struct NodeLocalDataset {
    node_id: NodeGraphNodeId,
    backref: RefAny, // RefAny<NodeGraphLocalDataset>
}

#[derive(Debug, Copy, Clone)]
enum InputOrOutput {
    Input(usize),
    Output(usize),
}

struct NodeInputOutputLocalDataset {
    io_id: InputOrOutput,
    backref: RefAny, // RefAny<NodeLocalDataset>
}

struct NodeFieldLocalDataset {
    field_idx: usize,
    backref: RefAny, // RefAny<NodeLocalDataset>
}

#[derive(Copy, Clone)]
struct ConnectionLocalDataset {
    out_node_id: NodeGraphNodeId,
    out_idx: usize,
    in_node_id: NodeGraphNodeId,
    in_idx: usize,
    swap_vert: bool,
    swap_horz: bool,
    color: ColorU,
}

#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn render_node(
    node: &Node,
    graph_offset: (f32, f32),
    node_info: &NodeTypeInfo,
    mut node_local_dataset: NodeLocalDataset,
    scale_factor: f32,
    visual_marker: AzString,
) -> Dom {
    use azul_core::dom::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, Dom, DomVec, IdOrClass,
        IdOrClass::Class, IdOrClassVec,
    };
    #[allow(clippy::wildcard_imports)]
    // widget/render module pulls in the css property/value types it builds with
    use azul_css::*;

    const STRING_9416190750059025162: AzString = AzString::from_const_str("Material Icons");
    const STRING_16146701490593874959: AzString = AzString::from_const_str("system:ui");
    const STYLE_BACKGROUND_CONTENT_524016094839686509_ITEMS: &[StyleBackgroundContent] =
        &[StyleBackgroundContent::Color(ColorU {
            r: 34,
            g: 34,
            b: 34,
            a: 255,
        })];
    const STYLE_BACKGROUND_CONTENT_10430246856047584562_ITEMS: &[StyleBackgroundContent] =
        &[StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                dir_from: DirectionCorner::Left,
                dir_to: DirectionCorner::Right,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: NormalizedLinearColorStopVec::from_const_slice(
                LINEAR_COLOR_STOP_4373556077110009258_ITEMS,
            ),
        })];
    const STYLE_BACKGROUND_CONTENT_11535310356736632656_ITEMS: &[StyleBackgroundContent] =
        &[StyleBackgroundContent::RadialGradient(RadialGradient {
            shape: Shape::Ellipse,
            extend_mode: ExtendMode::Clamp,
            position: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Left,
                vertical: BackgroundPositionVertical::Top,
            },
            size: RadialGradientSize::FarthestCorner,
            stops: NormalizedLinearColorStopVec::from_const_slice(
                LINEAR_COLOR_STOP_15596411095679453272_ITEMS,
            ),
        })];
    const STYLE_BACKGROUND_CONTENT_11936041127084538304_ITEMS: &[StyleBackgroundContent] =
        &[StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                dir_from: DirectionCorner::Right,
                dir_to: DirectionCorner::Left,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: NormalizedLinearColorStopVec::from_const_slice(
                LINEAR_COLOR_STOP_4373556077110009258_ITEMS,
            ),
        })];
    const STYLE_BACKGROUND_CONTENT_15813232491335471489_ITEMS: &[StyleBackgroundContent] =
        &[StyleBackgroundContent::Color(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 85,
        })];
    const STYLE_BACKGROUND_CONTENT_17648039690071193942_ITEMS: &[StyleBackgroundContent] =
        &[StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                dir_from: DirectionCorner::Top,
                dir_to: DirectionCorner::Bottom,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: NormalizedLinearColorStopVec::from_const_slice(
                LINEAR_COLOR_STOP_7397113864565941600_ITEMS,
            ),
        })];
    const STYLE_TRANSFORM_347117342922946953_ITEMS: &[StyleTransform] =
        &[StyleTransform::Translate(StyleTransformTranslate2D {
            x: PixelValue::const_px(200),
            y: PixelValue::const_px(100),
        })];
    const STYLE_TRANSFORM_14683950870521466298_ITEMS: &[StyleTransform] =
        &[StyleTransform::Translate(StyleTransformTranslate2D {
            x: PixelValue::const_px(240),
            y: PixelValue::const_px(-10),
        })];
    const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
        &[StyleFontFamily::System(STRING_16146701490593874959)];
    const STYLE_FONT_FAMILY_11383897783350685780_ITEMS: &[StyleFontFamily] =
        &[StyleFontFamily::System(STRING_9416190750059025162)];
    const LINEAR_COLOR_STOP_4373556077110009258_ITEMS: &[NormalizedLinearColorStop] = &[
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(20),
            color: ColorOrSystem::color(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 204,
            }),
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(100),
            color: ColorOrSystem::color(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }),
        },
    ];
    const LINEAR_COLOR_STOP_7397113864565941600_ITEMS: &[NormalizedLinearColorStop] = &[
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(0),
            color: ColorOrSystem::color(ColorU {
                r: 229,
                g: 57,
                b: 53,
                a: 255,
            }),
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(100),
            color: ColorOrSystem::color(ColorU {
                r: 227,
                g: 93,
                b: 91,
                a: 255,
            }),
        },
    ];
    const LINEAR_COLOR_STOP_15596411095679453272_ITEMS: &[NormalizedLinearColorStop] = &[
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(0),
            color: ColorOrSystem::color(ColorU {
                r: 47,
                g: 49,
                b: 54,
                a: 255,
            }),
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(50),
            color: ColorOrSystem::color(ColorU {
                r: 47,
                g: 49,
                b: 54,
                a: 255,
            }),
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(100),
            color: ColorOrSystem::color(ColorU {
                r: 32,
                g: 34,
                b: 37,
                a: 255,
            }),
        },
    ];

    const CSS_MATCH_10339190304804100510_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_output_wrapper
        CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        CssPropertyWithConditions::simple(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column),
        )),
        CssPropertyWithConditions::simple(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft {
            inner: PixelValue::const_px(0),
        }))),
        CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
    ];
    const CSS_MATCH_10339190304804100510: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_10339190304804100510_PROPERTIES);

    const CSS_MATCH_11452431279102104133_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_input_connection_label
        CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
            StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
        ))),
        CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
            StyleFontSize {
                inner: PixelValue::const_px(12),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight::Px(PixelValue::const_px(15)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Right,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::const_px(100)),
        ))),
    ];
    const CSS_MATCH_11452431279102104133: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_11452431279102104133_PROPERTIES);

    const CSS_MATCH_1173826950760010563_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_configuration_field_value:focus
        CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        // .node_configuration_field_value
        CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
            LayoutAlignItems::Center,
        ))),
        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_524016094839686509_ITEMS,
            )),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(1),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Left,
        ))),
    ];
    const CSS_MATCH_1173826950760010563: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_1173826950760010563_PROPERTIES);

    const CSS_MATCH_1198521124955124418_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_configuration_field_label
        CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
            LayoutAlignItems::Center,
        ))),
        CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(1),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::MaxWidth(LayoutMaxWidthValue::Exact(
            LayoutMaxWidth {
                inner: PixelValue::const_px(120),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(10),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Left,
        ))),
    ];
    const CSS_MATCH_1198521124955124418: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_1198521124955124418_PROPERTIES);

    const CSS_MATCH_12038890904436132038_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_output_connection_label_wrapper
        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_10430246856047584562_ITEMS,
            )),
        )),
        CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(5),
            },
        ))),
    ];
    const CSS_MATCH_12038890904436132038: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_12038890904436132038_PROPERTIES);

    const CSS_MATCH_12400244273289328300_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_output_container
        CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        CssPropertyWithConditions::simple(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
        )),
        CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
            LayoutMarginTop {
                inner: PixelValue::const_px(10),
            },
        ))),
    ];
    const CSS_MATCH_12400244273289328300: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_12400244273289328300_PROPERTIES);

    const CSS_MATCH_14906563417280941890_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .outputs
        CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(0),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Relative,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::const_px(0)),
        ))),
    ];
    const CSS_MATCH_14906563417280941890: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_14906563417280941890_PROPERTIES);

    const CSS_MATCH_16946967739775705757_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .inputs
        CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(0),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Relative,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::const_px(0)),
        ))),
    ];
    const CSS_MATCH_16946967739775705757: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_16946967739775705757_PROPERTIES);

    const CSS_MATCH_1739273067404038547_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_label
        CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
            StyleFontSize {
                inner: PixelValue::const_px(18),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight::Px(PixelValue::const_px(50)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(5),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
            LayoutPaddingTop {
                inner: PixelValue::const_px(10),
            },
        ))),
    ];
    const CSS_MATCH_1739273067404038547: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_1739273067404038547_PROPERTIES);

    const CSS_MATCH_2008162367868363199_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_output_connection_label
        CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
            StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
        ))),
        CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
            StyleFontSize {
                inner: PixelValue::const_px(12),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight::Px(PixelValue::const_px(15)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Left,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::const_px(100)),
        ))),
    ];
    const CSS_MATCH_2008162367868363199: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_2008162367868363199_PROPERTIES);

    const CSS_MATCH_2639191696846875011_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_configuration_field_container
        CssPropertyWithConditions::simple(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column),
        )),
        CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
            LayoutPaddingTop {
                inner: PixelValue::const_px(3),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::PaddingBottom(
            LayoutPaddingBottomValue::Exact(LayoutPaddingBottom {
                inner: PixelValue::const_px(3),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(5),
            },
        ))),
        CssPropertyWithConditions::simple(CssProperty::PaddingRight(
            LayoutPaddingRightValue::Exact(LayoutPaddingRight {
                inner: PixelValue::const_px(5),
            }),
        )),
    ];
    const CSS_MATCH_2639191696846875011: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_2639191696846875011_PROPERTIES);

    const CSS_MATCH_3354247437065914166_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_body
        CssPropertyWithConditions::simple(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
        )),
        CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Relative,
        ))),
    ];
    const CSS_MATCH_3354247437065914166: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_3354247437065914166_PROPERTIES);

    const CSS_MATCH_4700400755767504372_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_input_connection_label_wrapper
        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_11936041127084538304_ITEMS,
            )),
        )),
        CssPropertyWithConditions::simple(CssProperty::PaddingRight(
            LayoutPaddingRightValue::Exact(LayoutPaddingRight {
                inner: PixelValue::const_px(5),
            }),
        )),
    ];
    const CSS_MATCH_4700400755767504372: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4700400755767504372_PROPERTIES);

    const CSS_MATCH_705881630351954657_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_input_wrapper
        CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        CssPropertyWithConditions::simple(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column),
        )),
        CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Right(LayoutRightValue::Exact(
            LayoutRight {
                inner: PixelValue::const_px(0),
            },
        ))),
    ];
    const CSS_MATCH_705881630351954657: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_705881630351954657_PROPERTIES);

    const CSS_MATCH_7395766480280098891_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_close_button
        CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
            LayoutAlignItems::Center,
        ))),
        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_17648039690071193942_ITEMS,
            )),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
            BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 229,
                    g: 57,
                    b: 53,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(2),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Outset,
            }),
        ))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
            BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 229,
                    g: 57,
                    b: 53,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(2),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Outset,
            }),
        ))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
            BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 229,
                    g: 57,
                    b: 53,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(2),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Outset,
            }),
        ))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
            StyleBoxShadowValue::Exact(BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 229,
                    g: 57,
                    b: 53,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(2),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Outset,
            })),
        )),
        CssPropertyWithConditions::simple(CssProperty::Cursor(StyleCursorValue::Exact(
            StyleCursor::Pointer,
        ))),
        CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
            StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_11383897783350685780_ITEMS),
        ))),
        CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight::Px(PixelValue::const_px(20)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
        CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Center,
        ))),
        CssPropertyWithConditions::simple(CssProperty::Transform(StyleTransformVecValue::Exact(
            StyleTransformVec::from_const_slice(STYLE_TRANSFORM_14683950870521466298_ITEMS),
        ))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::const_px(20)),
        ))),
    ];
    const CSS_MATCH_7395766480280098891: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_7395766480280098891_PROPERTIES);

    const CSS_MATCH_7432473243011547380_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_content_wrapper
        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_15813232491335471489_ITEMS,
            )),
        )),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
            BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(4),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Inset,
            }),
        ))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
            BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(4),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Inset,
            }),
        ))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
            BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(4),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Inset,
            }),
        ))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
            StyleBoxShadowValue::Exact(BoxOrStatic::Static(&StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                color: ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(4),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::const_px(0),
                },
                clip_mode: BoxShadowClipMode::Inset,
            })),
        )),
        CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(1),
            },
        ))),
    ];
    const CSS_MATCH_7432473243011547380: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_7432473243011547380_PROPERTIES);

    const CSS_MATCH_9863994880298313101_PROPERTIES: &[CssPropertyWithConditions] = &[
        // .node_input_container
        CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        CssPropertyWithConditions::simple(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
        )),
        CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
            LayoutMarginTop {
                inner: PixelValue::const_px(10),
            },
        ))),
    ];
    const CSS_MATCH_9863994880298313101: CssPropertyWithConditionsVec =
        CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_9863994880298313101_PROPERTIES);

    // NODE RENDER FUNCTION BEGIN

    let node_transform = StyleTransformTranslate2D {
        x: PixelValue::px(graph_offset.0 + node.position.x),
        y: PixelValue::px(graph_offset.1 + node.position.y),
    };

    // get names and colors for inputs / outputs
    let inputs = node_info
        .inputs
        .iter()
        .filter_map(|io_id| {
            let node_graph_ref = node_local_dataset
                .backref
                .downcast_ref::<NodeGraphLocalDataset>()?;
            let io_info = node_graph_ref
                .node_graph
                .input_output_types
                .iter()
                .find(|i| i.io_type_id == *io_id)?;
            Some((io_info.io_info.data_type.clone(), io_info.io_info.color))
        })
        .collect::<Vec<_>>();

    let outputs = node_info
        .outputs
        .iter()
        .filter_map(|io_id| {
            let node_graph_ref = node_local_dataset
                .backref
                .downcast_ref::<NodeGraphLocalDataset>()?;
            let io_info = node_graph_ref
                .node_graph
                .input_output_types
                .iter()
                .find(|i| i.io_type_id == *io_id)?;
            Some((io_info.io_info.data_type.clone(), io_info.io_info.color))
        })
        .collect::<Vec<_>>();

    let node_local_dataset = RefAny::new(node_local_dataset);

    Dom::create_div()
    .with_css_props(vec![
        CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
    ].into())
    .with_children(vec![
        Dom::create_div()
        .with_callbacks(vec![
           CoreCallbackData {
               event: EventFilter::Hover(HoverEventFilter::LeftMouseDown),
               refany: node_local_dataset.clone(),
               callback: CoreCallback { cb: nodegraph_set_active_node as usize, ctx: OptionRefAny::None },
           },
        ].into())
        .with_css_props(vec![
           // .node_graph_node
           CssPropertyWithConditions::simple(CssProperty::OverflowX(
               LayoutOverflowValue::Exact(LayoutOverflow::Visible)
           )),
           CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
               LayoutPosition::Relative,
           ))),
           CssPropertyWithConditions::simple(CssProperty::OverflowY(
               LayoutOverflowValue::Exact(LayoutOverflow::Visible)
           )),
           CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
               StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                   STYLE_BACKGROUND_CONTENT_11535310356736632656_ITEMS,
               )),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
               StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                   inner: ColorU {
                       r: 0,
                       g: 180,
                       b: 219,
                       a: 255,
                   },
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
               StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                   inner: ColorU {
                       r: 0,
                       g: 180,
                       b: 219,
                       a: 255,
                   },
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
               StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                   inner: ColorU {
                       r: 0,
                       g: 180,
                       b: 219,
                       a: 255,
                   },
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
               StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                   inner: ColorU {
                       r: 0,
                       g: 180,
                       b: 219,
                       a: 255,
                   },
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
               StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                   inner: BorderStyle::Solid,
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
               StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                   inner: BorderStyle::Solid,
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
               StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                   inner: BorderStyle::Solid,
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
               StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                   inner: BorderStyle::Solid,
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
               LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                   inner: PixelValue::const_px(1),
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
               LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                   inner: PixelValue::const_px(1),
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
               LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                   inner: PixelValue::const_px(1),
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
               LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                   inner: PixelValue::const_px(1),
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(BoxOrStatic::heap(
               StyleBoxShadow {
                   offset_x: PixelValueNoPercent { inner: PixelValue::const_px(0) }, offset_y: PixelValueNoPercent { inner: PixelValue::const_px(0) },
                   color: ColorU {
                       r: 0,
                       g: 131,
                       b: 176,
                       a: 119,
                   },
                   blur_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(3),
                   },
                   spread_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(0),
                   },
                   clip_mode: BoxShadowClipMode::Outset,
               },
           )))),
           CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(BoxOrStatic::heap(
               StyleBoxShadow {
                   offset_x: PixelValueNoPercent { inner: PixelValue::const_px(0) }, offset_y: PixelValueNoPercent { inner: PixelValue::const_px(0) },
                   color: ColorU {
                       r: 0,
                       g: 131,
                       b: 176,
                       a: 119,
                   },
                   blur_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(3),
                   },
                   spread_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(0),
                   },
                   clip_mode: BoxShadowClipMode::Outset,
               },
           )))),
           CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(BoxOrStatic::heap(
               StyleBoxShadow {
                   offset_x: PixelValueNoPercent { inner: PixelValue::const_px(0) }, offset_y: PixelValueNoPercent { inner: PixelValue::const_px(0) },
                   color: ColorU {
                       r: 0,
                       g: 131,
                       b: 176,
                       a: 119,
                   },
                   blur_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(3),
                   },
                   spread_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(0),
                   },
                   clip_mode: BoxShadowClipMode::Outset,
               },
           )))),
           CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
               StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                   offset_x: PixelValueNoPercent { inner: PixelValue::const_px(0) }, offset_y: PixelValueNoPercent { inner: PixelValue::const_px(0) },
                   color: ColorU {
                       r: 0,
                       g: 131,
                       b: 176,
                       a: 119,
                   },
                   blur_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(3),
                   },
                   spread_radius: PixelValueNoPercent {
                       inner: PixelValue::const_px(0),
                   },
                   clip_mode: BoxShadowClipMode::Outset,
               })),
           )),
           CssPropertyWithConditions::simple(CssProperty::TextColor(StyleTextColorValue::Exact(
               StyleTextColor {
                   inner: ColorU {
                       r: 255,
                       g: 255,
                       b: 255,
                       a: 255,
                   },
               },
           ))),

           CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
               LayoutDisplay::Block
           ))),
           CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
               StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
           ))),
           CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
               LayoutPaddingTop {
                   inner: PixelValue::const_px(10),
               },
           ))),
           CssPropertyWithConditions::simple(CssProperty::PaddingBottom(
               LayoutPaddingBottomValue::Exact(LayoutPaddingBottom {
                   inner: PixelValue::const_px(10),
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
               LayoutPaddingLeft {
                   inner: PixelValue::const_px(10),
               },
           ))),
           CssPropertyWithConditions::simple(CssProperty::PaddingRight(
               LayoutPaddingRightValue::Exact(LayoutPaddingRight {
                   inner: PixelValue::const_px(10),
               }),
           )),
           CssPropertyWithConditions::simple(CssProperty::Transform(StyleTransformVecValue::Exact(
               if scale_factor == 1.0 {
                    vec![
                         StyleTransform::Translate(node_transform)
                    ]
               } else {
                    vec![
                         StyleTransform::Translate(node_transform),
                         StyleTransform::ScaleX(PercentageValue::new(scale_factor * 100.0)),
                         StyleTransform::ScaleY(PercentageValue::new(scale_factor * 100.0)),
                    ]
               }.into()
           ))),
           CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
               LayoutWidth::Px(PixelValue::const_px(250),),
           ))),
        ].into())
        .with_ids_and_classes({
           const IDS_AND_CLASSES_4480169002427296613: &[IdOrClass] =
               &[Class(AzString::from_const_str("node_graph_node"))];
           IdOrClassVec::from_const_slice(IDS_AND_CLASSES_4480169002427296613)
        })
        .with_children(DomVec::from_vec(vec![
           crate::widgets::widget_p_with_text(AzString::from_const_str("X"))
               .with_css_props(CSS_MATCH_7395766480280098891)
               .with_callbacks(vec![
                   CoreCallbackData {
                       event: EventFilter::Hover(HoverEventFilter::Click),
                       refany: node_local_dataset.clone(),
                       callback: CoreCallback { cb: nodegraph_delete_node as usize, ctx: OptionRefAny::None },
                   },
               ].into())
               .with_ids_and_classes({
                   const IDS_AND_CLASSES_7122017923389407516: &[IdOrClass] =
                       &[Class(AzString::from_const_str("node_close_button"))];
                   IdOrClassVec::from_const_slice(IDS_AND_CLASSES_7122017923389407516)
               }),
           crate::widgets::widget_p_with_text(node_info.node_type_name.clone())
               .with_css_props(CSS_MATCH_1739273067404038547)
               .with_ids_and_classes({
                   const IDS_AND_CLASSES_15777790571346582635: &[IdOrClass] =
                       &[Class(AzString::from_const_str("node_label"))];
                   IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15777790571346582635)
               }),
           Dom::create_div()
               .with_css_props(CSS_MATCH_3354247437065914166)
               .with_ids_and_classes({
                   const IDS_AND_CLASSES_5590500152394859708: &[IdOrClass] =
                       &[Class(AzString::from_const_str("node_body"))];
                   IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5590500152394859708)
               })
               .with_children(DomVec::from_vec(vec![
                   Dom::create_div()
                       .with_css_props(CSS_MATCH_16946967739775705757)
                       .with_ids_and_classes({
                           const IDS_AND_CLASSES_3626404106673061698: &[IdOrClass] =
                               &[Class(AzString::from_const_str("inputs"))];
                           IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3626404106673061698)
                       })
                       .with_children(DomVec::from_vec(vec![Dom::create_div()
                           .with_css_props(CSS_MATCH_705881630351954657)
                           .with_ids_and_classes({
                               const IDS_AND_CLASSES_12825690349660780627: &[IdOrClass] =
                                   &[Class(AzString::from_const_str("node_input_wrapper"))];
                               IdOrClassVec::from_const_slice(
                                   IDS_AND_CLASSES_12825690349660780627,
                               )
                           })
                           .with_children(DomVec::from_vec(
                               inputs
                               .into_iter()
                               .enumerate()
                               .map(|(io_id, (input_label, input_color))| {
                                   use self::InputOrOutput::Input;

                                   Dom::create_div()
                                       .with_css_props(CSS_MATCH_9863994880298313101)
                                       .with_ids_and_classes({
                                           const IDS_AND_CLASSES_5020681879750641508:
                                               &[IdOrClass] = &[Class(AzString::from_const_str(
                                               "node_input_container",
                                           ))];
                                           IdOrClassVec::from_const_slice(
                                               IDS_AND_CLASSES_5020681879750641508,
                                           )
                                       })
                                       .with_children(DomVec::from_vec(vec![
                                           Dom::create_div()
                                               .with_css_props(
                                                   CSS_MATCH_4700400755767504372,
                                               )
                                               .with_ids_and_classes({
                                                   const IDS_AND_CLASSES_9154857442066749879:
                                                       &[IdOrClass] =
                                                       &[Class(AzString::from_const_str(
                                                           "node_input_connection_label_wrapper",
                                                       ))];
                                                   IdOrClassVec::from_const_slice(
                                                       IDS_AND_CLASSES_9154857442066749879,
                                                   )
                                               })
                                               .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                                                   input_label,
                                               )
                                               .with_css_props(
                                                   CSS_MATCH_11452431279102104133,
                                               )
                                               .with_ids_and_classes({
                                                   const IDS_AND_CLASSES_16291496011772407931:
                                                       &[IdOrClass] =
                                                       &[Class(AzString::from_const_str(
                                                           "node_input_connection_label",
                                                       ))];
                                                   IdOrClassVec::from_const_slice(
                                                       IDS_AND_CLASSES_16291496011772407931,
                                                   )
                                               })])),
                                           Dom::create_div()
                                               .with_callbacks(vec![
                                                   CoreCallbackData {
                                                       event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                                                       refany: RefAny::new(NodeInputOutputLocalDataset {
                                                           io_id: Input(io_id),
                                                           backref: node_local_dataset.clone(),
                                                       }),
                                                       callback: CoreCallback { cb: nodegraph_input_output_connect as usize, ctx: OptionRefAny::None },
                                                   },
                                                   CoreCallbackData {
                                                       event: EventFilter::Hover(HoverEventFilter::MiddleMouseUp),
                                                       refany: RefAny::new(NodeInputOutputLocalDataset {
                                                           io_id: Input(io_id),
                                                           backref: node_local_dataset.clone(),
                                                       }),
                                                       callback: CoreCallback { cb: nodegraph_input_output_disconnect as usize, ctx: OptionRefAny::None },
                                                   },
                                               ].into())
                                               .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                                                       // .node_input
                                                       CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                                                           StyleBackgroundContentVecValue::Exact(vec![StyleBackgroundContent::Color(input_color)].into()),
                                                       )),
                                                       CssPropertyWithConditions::simple(CssProperty::Cursor(StyleCursorValue::Exact(
                                                           StyleCursor::Pointer,
                                                       ))),
                                                       CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                                                           LayoutHeight::Px(PixelValue::const_px(15),),
                                                       ))),
                                                       CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
                                                           LayoutWidth::Px(PixelValue::const_px(15),),
                                                       ))),
                                                   ])
                                               )
                                               .with_ids_and_classes({
                                                   const IDS_AND_CLASSES_2128818677168244823:
                                                       &[IdOrClass] = &[Class(
                                                       AzString::from_const_str("node_input"),
                                                   )];
                                                   IdOrClassVec::from_const_slice(
                                                       IDS_AND_CLASSES_2128818677168244823,
                                                   )
                                               }),
                                       ]))
                               }).collect()
                           ))
                       ])),
                   Dom::create_div()
                       .with_css_props(CSS_MATCH_7432473243011547380)
                       .with_ids_and_classes({
                           const IDS_AND_CLASSES_746059979773622802: &[IdOrClass] =
                               &[Class(AzString::from_const_str("node_content_wrapper"))];
                           IdOrClassVec::from_const_slice(IDS_AND_CLASSES_746059979773622802)
                       })
                       .with_children({

                           let mut fields = Vec::new();

                           for (field_idx, field) in node.fields.iter().enumerate() {

                               let field_local_dataset = RefAny::new(NodeFieldLocalDataset {
                                   field_idx,
                                   backref: node_local_dataset.clone(),
                               });

                               let div = Dom::create_div()
                               .with_css_props(CSS_MATCH_2639191696846875011)
                               .with_ids_and_classes({
                                   const IDS_AND_CLASSES_4413230059125905311: &[IdOrClass] =
                                       &[Class(AzString::from_const_str(
                                           "node_configuration_field_container",
                                       ))];
                                   IdOrClassVec::from_const_slice(
                                       IDS_AND_CLASSES_4413230059125905311,
                                   )
                               })
                               .with_children(DomVec::from_vec(vec![
                                   crate::widgets::widget_p_with_text(field.key.clone())
                                   .with_css_props(CSS_MATCH_1198521124955124418)
                                   .with_ids_and_classes({
                                       const IDS_AND_CLASSES_12334207996395559585:
                                           &[IdOrClass] =
                                           &[Class(AzString::from_const_str(
                                               "node_configuration_field_label",
                                           ))];
                                       IdOrClassVec::from_const_slice(
                                           IDS_AND_CLASSES_12334207996395559585,
                                       )
                                   }),

                                   match &field.value {
                                       NodeTypeFieldValue::TextInput(initial_text) => {
                                           let cb: TextInputOnFocusLostCallbackType = nodegraph_on_textinput_focus_lost;
                                           TextInput::create()
                                           .with_text(initial_text.clone())
                                           .with_on_focus_lost(field_local_dataset, cb)
                                           .dom()
                                       },
                                       NodeTypeFieldValue::NumberInput(initial_value) => {
                                           let cb: NumberInputOnFocusLostCallbackType = nodegraph_on_numberinput_focus_lost;
                                           NumberInput::create(*initial_value)
                                           .with_on_focus_lost(field_local_dataset, cb)
                                           .dom()
                                       },
                                       NodeTypeFieldValue::CheckBox(initial_checked) => {
                                           let cb: CheckBoxOnToggleCallbackType = nodegraph_on_checkbox_value_changed;
                                           CheckBox::create(*initial_checked)
                                           .with_on_toggle(field_local_dataset, cb)
                                           .dom()
                                       },
                                       NodeTypeFieldValue::ColorInput(initial_color) => {
                                           let cb: ColorInputOnValueChangeCallbackType = nodegraph_on_colorinput_value_changed;
                                           ColorInput::create(*initial_color)
                                           .with_on_value_change(field_local_dataset, cb)
                                           .dom()
                                       },
                                       NodeTypeFieldValue::FileInput(file_path) => {
                                           let cb: FileInputOnPathChangeCallbackType = nodegraph_on_fileinput_button_clicked;
                                           FileInput::create(file_path.clone())
                                           .with_on_path_change(field_local_dataset, cb)
                                           .dom()
                                       },
                                   }
                               ]));

                               fields.push(div);
                           }

                           DomVec::from_vec(fields)
                       }),
                   Dom::create_div()
                       .with_css_props(CSS_MATCH_14906563417280941890)
                       .with_ids_and_classes({
                           const IDS_AND_CLASSES_4737474624251936466: &[IdOrClass] =
                               &[Class(AzString::from_const_str("outputs"))];
                           IdOrClassVec::from_const_slice(IDS_AND_CLASSES_4737474624251936466)
                       })
                       .with_children(DomVec::from_vec(vec![Dom::create_div()
                           .with_css_props(CSS_MATCH_10339190304804100510)
                           .with_ids_and_classes({
                               const IDS_AND_CLASSES_12883576328110161157: &[IdOrClass] =
                                   &[Class(AzString::from_const_str("node_output_wrapper"))];
                               IdOrClassVec::from_const_slice(
                                   IDS_AND_CLASSES_12883576328110161157,
                               )
                           })
                           .with_children(DomVec::from_vec(
                               outputs
                               .into_iter()
                               .enumerate()
                               .map(|(io_id, (output_label, output_color))| {
                                   use self::InputOrOutput::Output;
                                   Dom::create_div()
                                       .with_css_props(CSS_MATCH_12400244273289328300)
                                       .with_ids_and_classes({
                                           const IDS_AND_CLASSES_10917819668096233812:
                                               &[IdOrClass] = &[Class(AzString::from_const_str(
                                               "node_output_container",
                                           ))];
                                           IdOrClassVec::from_const_slice(
                                               IDS_AND_CLASSES_10917819668096233812,
                                           )
                                       })
                                       .with_children(DomVec::from_vec(vec![
                                           Dom::create_div()
                                               .with_callbacks(vec![
                                                   CoreCallbackData {
                                                       event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                                                       refany: RefAny::new(NodeInputOutputLocalDataset {
                                                           io_id: Output(io_id),
                                                           backref: node_local_dataset.clone(),
                                                       }),
                                                       callback: CoreCallback { cb: nodegraph_input_output_connect as usize, ctx: OptionRefAny::None },
                                                   },
                                                   CoreCallbackData {
                                                       event: EventFilter::Hover(HoverEventFilter::MiddleMouseUp),
                                                       refany: RefAny::new(NodeInputOutputLocalDataset {
                                                           io_id: Output(io_id),
                                                           backref: node_local_dataset.clone(),
                                                       }),
                                                       callback: CoreCallback { cb: nodegraph_input_output_disconnect as usize, ctx: OptionRefAny::None },
                                                   },
                                               ].into())
                                               .with_css_props(
                                                   CssPropertyWithConditionsVec::from_vec(vec![
                                                       // .node_output
                                                       CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                                                           StyleBackgroundContentVecValue::Exact(vec![
                                                               StyleBackgroundContent::Color(output_color)
                                                           ].into()),
                                                       )),
                                                       CssPropertyWithConditions::simple(CssProperty::Cursor(StyleCursorValue::Exact(
                                                           StyleCursor::Pointer,
                                                       ))),
                                                       CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                                                           LayoutHeight::Px(PixelValue::const_px(15),),
                                                       ))),
                                                       CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
                                                           LayoutWidth::Px(PixelValue::const_px(15),),
                                                       ))),
                                                   ])
                                               )
                                               .with_ids_and_classes({
                                                   const IDS_AND_CLASSES_17632471664405317563:
                                                       &[IdOrClass] = &[Class(
                                                       AzString::from_const_str("node_output"),
                                                   )];
                                                   IdOrClassVec::from_const_slice(
                                                       IDS_AND_CLASSES_17632471664405317563,
                                                   )
                                               }),
                                           Dom::create_div()
                                               .with_css_props(
                                                   CSS_MATCH_12038890904436132038,
                                               )
                                               .with_ids_and_classes({
                                                   const IDS_AND_CLASSES_1667960214206134147:
                                                       &[IdOrClass] =
                                                       &[Class(AzString::from_const_str(
                                                           "node_output_connection_label_wrapper",
                                                       ))];
                                                   IdOrClassVec::from_const_slice(
                                                       IDS_AND_CLASSES_1667960214206134147,
                                                   )
                                               })
                                               .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                                                   output_label,
                                               )
                                               .with_css_props(
                                                   CSS_MATCH_2008162367868363199,
                                               )
                                               .with_ids_and_classes({
                                                   const IDS_AND_CLASSES_2974914452796301884:
                                                       &[IdOrClass] =
                                                       &[Class(AzString::from_const_str(
                                                           "node_output_connection_label",
                                                       ))];
                                                   IdOrClassVec::from_const_slice(
                                                       IDS_AND_CLASSES_2974914452796301884,
                                                   )
                                               })])),
                                       ]))
                               }).collect()
                           ))])),
               ])),
        ]))
        .with_dataset(Some(node_local_dataset).into())
        // Resolvable from the drag callback via `node_marker(prefix, id)` —
        // the fast-path jump that positions this node without a relayout.
        .with_marker(Some(visual_marker).into())
    ].into())
}

#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn render_connections(node_graph: &NodeGraph, connections_marker: AzString) -> Dom {
    static NODEGRAPH_CONNECTIONS_CONTAINER_CLASS: &[IdOrClass] = &[Class(
        AzString::from_const_str("nodegraph-connections-container"),
    )];

    static NODEGRAPH_CONNECTIONS_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
        CssPropertyWithConditions::simple(CssProperty::position(LayoutPosition::Absolute)),
        CssPropertyWithConditions::simple(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
    ];

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(
            NODEGRAPH_CONNECTIONS_CONTAINER_CLASS,
        ))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            NODEGRAPH_CONNECTIONS_CONTAINER_PROPS,
        ))
        .with_marker(Some(connections_marker).into())
        .with_children({
            let mut children = Vec::new();

            for NodeIdNodeMap { node_id, node } in node_graph.nodes.as_ref() {
                let out_node_id = node_id;
                let node_type_info = match node_graph
                    .node_types
                    .iter()
                    .find(|i| i.node_type_id == node.node_type)
                {
                    Some(s) => &s.node_type_info,
                    None => continue,
                };

                for OutputConnection {
                    output_index,
                    connects_to,
                } in node.connect_out.as_ref()
                {
                    let Some(output_type_id) = node_type_info.outputs.get(*output_index) else {
                        continue;
                    };

                    let output_color = match node_graph
                        .input_output_types
                        .iter()
                        .find(|o| o.io_type_id == *output_type_id)
                    {
                        Some(s) => s.io_info.color,
                        None => continue,
                    };

                    for InputNodeAndIndex {
                        node_id,
                        input_index,
                    } in connects_to.as_ref()
                    {
                        let in_node_id = node_id;

                        let mut cld = ConnectionLocalDataset {
                            out_node_id: *out_node_id,
                            out_idx: *output_index,
                            in_node_id: *in_node_id,
                            in_idx: *input_index,
                            swap_vert: false,
                            swap_horz: false,
                            color: output_color,
                        };

                        let Some((rect, swap_vert, swap_horz)) = get_rect(node_graph, cld) else {
                            continue;
                        };

                        cld.swap_vert = swap_vert;
                        cld.swap_horz = swap_horz;

                        let cld_refany = RefAny::new(cld);
                        let connection_div = Dom::create_image(ImageRef::callback(
                            draw_connection as usize,
                            cld_refany.clone(),
                        ))
                        .with_dataset(Some(cld_refany).into())
                        .with_css_props(
                            vec![
                                CssPropertyWithConditions::simple(CssProperty::Transform(
                                    StyleTransformVecValue::Exact(
                                        vec![
                                            StyleTransform::Translate(StyleTransformTranslate2D {
                                                x: PixelValue::px(
                                                    node_graph.offset.x + rect.origin.x,
                                                ),
                                                y: PixelValue::px(
                                                    node_graph.offset.y + rect.origin.y,
                                                ),
                                            }),
                                            StyleTransform::ScaleX(PercentageValue::new(
                                                node_graph.scale_factor * 100.0,
                                            )),
                                            StyleTransform::ScaleY(PercentageValue::new(
                                                node_graph.scale_factor * 100.0,
                                            )),
                                        ]
                                        .into(),
                                    ),
                                )),
                                CssPropertyWithConditions::simple(CssProperty::Width(
                                    LayoutWidthValue::Exact(LayoutWidth::Px(PixelValue::px(
                                        rect.size.width,
                                    ))),
                                )),
                                CssPropertyWithConditions::simple(CssProperty::Height(
                                    LayoutHeightValue::Exact(LayoutHeight::Px(PixelValue::px(
                                        rect.size.height,
                                    ))),
                                )),
                            ]
                            .into(),
                        );

                        children.push(
                            Dom::create_div()
                                .with_css("flex-grow: 1; position: absolute; overflow: hidden;")
                                .with_children(vec![connection_div].into()),
                        );
                    }
                }
            }

            children.into()
        })
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
extern "C" fn draw_connection(mut refany: RefAny, _info: ()) -> ImageRef {
    // RenderImageCallbackInfo not available in memtest
    // let size = info.get_bounds().get_physical_size();
    let size = LogicalSize {
        width: 100.0,
        height: 100.0,
    };

    // Cannot call draw_connection_inner without RenderImageCallbackInfo
    ImageRef::null_image(
        size.width as usize,
        size.height as usize,
        RawImageFormat::R8,
        Vec::new(),
    )
}

const NODE_WIDTH: f32 = 250.0;
const V_OFFSET: f32 = 71.0;
const DIST_BETWEEN_NODES: f32 = 10.0;
const CONNECTION_DOT_HEIGHT: f32 = 15.0;

// calculates the rect on which the connection is drawn in the UI
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded layout/render numeric cast
fn get_rect(
    node_graph: &NodeGraph,
    connection: ConnectionLocalDataset,
) -> Option<(LogicalRect, bool, bool)> {
    let ConnectionLocalDataset {
        out_node_id,
        out_idx,
        in_node_id,
        in_idx,
        ..
    } = connection;
    let out_node = node_graph.nodes.iter().find(|i| i.node_id == out_node_id)?;
    let in_node = node_graph.nodes.iter().find(|i| i.node_id == in_node_id)?;

    let x_out = out_node.node.position.x + NODE_WIDTH;
    let y_out = out_node.node.position.y
        + V_OFFSET
        + (out_idx as f32 * (DIST_BETWEEN_NODES + CONNECTION_DOT_HEIGHT));

    let x_in = in_node.node.position.x;
    let y_in = in_node.node.position.y
        + V_OFFSET
        + (in_idx as f32 * (DIST_BETWEEN_NODES + CONNECTION_DOT_HEIGHT));

    let should_swap_vertical = y_in > y_out;
    let should_swap_horizontal = x_in < x_out;

    let width = (x_in - x_out).abs();
    let height = (y_in - y_out).abs() + CONNECTION_DOT_HEIGHT;

    let x = x_in.min(x_out);
    let y = y_in.min(y_out);

    Some((
        LogicalRect {
            size: LogicalSize { width, height },
            origin: LogicalPosition { x, y },
        },
        should_swap_vertical,
        should_swap_horizontal,
    ))
}

extern "C" fn nodegraph_set_active_node(mut refany: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut refany) = refany.downcast_mut::<NodeLocalDataset>() {
        let node_id = refany.node_id;
        if let Some(mut backref) = refany.backref.downcast_mut::<NodeGraphLocalDataset>() {
            backref.active_node_being_dragged = Some(node_id);
        }
    }
    Update::DoNothing
}

extern "C" fn nodegraph_unset_active_node(mut refany: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut refany) = refany.downcast_mut::<NodeGraphLocalDataset>() {
        refany.active_node_being_dragged = None;
    }
    Update::DoNothing
}

// drag either the graph or the currently active nodes
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[allow(clippy::single_match_else)] // drag-node (Some) and drag-graph (None) are each ~135-line blocks; match labels the two modes far more clearly than if-let/else
extern "C" fn nodegraph_drag_graph_or_nodes(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeGraphLocalDataset>() else {
        return Update::DoNothing;
    };
    let refany = &mut *refany;

    let Some(prev) = info.get_previous_mouse_state() else {
        return Update::DoNothing;
    };
    let cur = info.get_current_mouse_state();
    if !(cur.left_down && prev.left_down) {
        // event is not a drag event
        return Update::DoNothing;
    }

    let (InWindow(current_mouse_pos), InWindow(previous_mouse_pos)) =
        (cur.cursor_position, prev.cursor_position)
    else {
        return Update::DoNothing;
    };

    let dx = (current_mouse_pos.x - previous_mouse_pos.x) * (1.0 / refany.node_graph.scale_factor);
    let dy = (current_mouse_pos.y - previous_mouse_pos.y) * (1.0 / refany.node_graph.scale_factor);
    let nodegraph_node = info.get_hit_node();

    let should_update = match refany.active_node_being_dragged {
        // drag node
        Some(node_graph_node_id) => {
            let dragged_node_marker = node_marker(&refany.marker_prefix, node_graph_node_id);
            let connections_marker: AzString =
                format!("{}-connections", refany.marker_prefix.as_str()).into();

            let _nodegraph_node = info.get_hit_node();
            let result = match refany.callbacks.on_node_dragged.as_ref() {
                Some(OnNodeDragged { callback, refany }) => (callback.cb)(
                    refany.clone(),
                    info,
                    node_graph_node_id,
                    NodeDragAmount { x: dx, y: dy },
                ),
                None => Update::DoNothing,
            };

            // update the visual transform of the node in the UI
            let node_position = match refany
                .node_graph
                .nodes
                .iter_mut()
                .find(|i| i.node_id == node_graph_node_id)
            {
                Some(s) => {
                    s.node.position.x += dx;
                    s.node.position.y += dy;
                    s.node.position
                }
                None => return Update::DoNothing,
            };

            let Some(visual_node_id) = info.get_node_id_by_marker(dragged_node_marker) else {
                return Update::DoNothing;
            };

            let node_transform = StyleTransformTranslate2D {
                x: PixelValue::px(node_position.x + refany.node_graph.offset.x),
                y: PixelValue::px(node_position.y + refany.node_graph.offset.y),
            };

            info.set_css_property(
                visual_node_id,
                CssProperty::transform(
                    if refany.node_graph.scale_factor == 1.0 {
                        vec![StyleTransform::Translate(node_transform)]
                    } else {
                        vec![
                            StyleTransform::Translate(node_transform),
                            StyleTransform::ScaleX(PercentageValue::new(
                                refany.node_graph.scale_factor * 100.0,
                            )),
                            StyleTransform::ScaleY(PercentageValue::new(
                                refany.node_graph.scale_factor * 100.0,
                            )),
                        ]
                    }
                    .into(),
                ),
            );

            // get the NodeId of the node containing all the connection lines
            let Some(connection_container_nodeid) =
                info.get_node_id_by_marker(connections_marker)
            else {
                return result;
            };

            // animate all the connections
            let mut first_connection_child = info.get_first_child(connection_container_nodeid);

            while let Some(connection_nodeid) = first_connection_child {
                first_connection_child = info.get_next_sibling(connection_nodeid);

                let Some(first_child) = info.get_first_child(connection_nodeid) else {
                    continue;
                };

                let Some(mut dataset) = info.get_dataset(first_child) else {
                    continue;
                };

                let Some(mut cld) = dataset.downcast_mut::<ConnectionLocalDataset>() else {
                    continue;
                };

                if !(cld.out_node_id == node_graph_node_id || cld.in_node_id == node_graph_node_id)
                {
                    continue; // connection does not need to be modified
                }

                let Some((new_rect, swap_vert, swap_horz)) = get_rect(&refany.node_graph, *cld)
                else {
                    continue;
                };

                cld.swap_vert = swap_vert;
                cld.swap_horz = swap_horz;

                let node_transform = StyleTransformTranslate2D {
                    x: PixelValue::px(refany.node_graph.offset.x + new_rect.origin.x),
                    y: PixelValue::px(refany.node_graph.offset.y + new_rect.origin.y),
                };

                info.set_css_property(
                    first_child,
                    CssProperty::transform(
                        if refany.node_graph.scale_factor == 1.0 {
                            vec![StyleTransform::Translate(node_transform)]
                        } else {
                            vec![
                                StyleTransform::Translate(node_transform),
                                StyleTransform::ScaleX(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                                StyleTransform::ScaleY(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                            ]
                        }
                        .into(),
                    ),
                );

                info.set_css_property(
                    first_child,
                    CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth::Px(PixelValue::px(
                        new_rect.size.width,
                    )))),
                );
                info.set_css_property(
                    first_child,
                    CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight::Px(
                        PixelValue::px(new_rect.size.height),
                    ))),
                );
            }

            result
        }
        // drag graph
        None => {
            let result = match refany.callbacks.on_node_graph_dragged.as_ref() {
                Some(OnNodeGraphDragged { callback, refany }) => {
                    (callback.cb)(refany.clone(), info, GraphDragAmount { x: dx, y: dy })
                }
                None => Update::DoNothing,
            };

            refany.node_graph.offset.x += dx;
            refany.node_graph.offset.y += dy;

            // Update the visual node positions
            let Some(node_container) = info.get_first_child(nodegraph_node) else {
                return Update::DoNothing;
            };

            let Some(node_container) = info.get_next_sibling(node_container) else {
                return Update::DoNothing;
            };

            let Some(mut node) = info.get_first_child(node_container) else {
                return Update::DoNothing;
            };

            loop {
                let Some(node_first_child) = info.get_first_child(node) else {
                    return Update::DoNothing;
                };

                let mut node_local_dataset = match info.get_dataset(node_first_child) {
                    None => return Update::DoNothing,
                    Some(s) => s,
                };

                let Some(node_graph_node_id) =
                    node_local_dataset.downcast_ref::<NodeLocalDataset>()
                else {
                    continue;
                };

                let node_graph_node_id = node_graph_node_id.node_id;

                let node_position = match refany
                    .node_graph
                    .nodes
                    .iter()
                    .find(|i| i.node_id == node_graph_node_id)
                {
                    Some(s) => s.node.position,
                    None => continue,
                };

                let node_transform = StyleTransformTranslate2D {
                    x: PixelValue::px(node_position.x + refany.node_graph.offset.x),
                    y: PixelValue::px(node_position.y + refany.node_graph.offset.y),
                };

                info.set_css_property(
                    node_first_child,
                    CssProperty::transform(
                        if refany.node_graph.scale_factor == 1.0 {
                            vec![StyleTransform::Translate(node_transform)]
                        } else {
                            vec![
                                StyleTransform::Translate(node_transform),
                                StyleTransform::ScaleX(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                                StyleTransform::ScaleY(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                            ]
                        }
                        .into(),
                    ),
                );

                node = match info.get_next_sibling(node) {
                    Some(s) => s,
                    None => break,
                };
            }

            let connections_marker: AzString =
                format!("{}-connections", refany.marker_prefix.as_str()).into();

            // Update the connection positions
            let Some(connection_container_nodeid) =
                info.get_node_id_by_marker(connections_marker)
            else {
                return result;
            };

            let mut first_connection_child = info.get_first_child(connection_container_nodeid);

            while let Some(connection_nodeid) = first_connection_child {
                first_connection_child = info.get_next_sibling(connection_nodeid);

                let Some(first_child) = info.get_first_child(connection_nodeid) else {
                    continue;
                };

                let Some(mut dataset) = info.get_dataset(first_child) else {
                    continue;
                };

                let Some(cld) = dataset.downcast_ref::<ConnectionLocalDataset>() else {
                    continue;
                };

                let Some((new_rect, _, _)) = get_rect(&refany.node_graph, *cld) else {
                    continue;
                };

                info.set_css_property(
                    first_child,
                    CssProperty::transform(
                        vec![
                            StyleTransform::Translate(StyleTransformTranslate2D {
                                x: PixelValue::px(refany.node_graph.offset.x + new_rect.origin.x),
                                y: PixelValue::px(refany.node_graph.offset.y + new_rect.origin.y),
                            }),
                            StyleTransform::ScaleX(PercentageValue::new(
                                refany.node_graph.scale_factor * 100.0,
                            )),
                            StyleTransform::ScaleY(PercentageValue::new(
                                refany.node_graph.scale_factor * 100.0,
                            )),
                        ]
                        .into(),
                    ),
                );
            }

            result
        }
    };

    info.stop_propagation();

    should_update
}

extern "C" fn nodegraph_duplicate_node(mut refany: RefAny, _info: CallbackInfo) -> Update {
    let Some(_data) = refany.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    Update::DoNothing // TODO
}

extern "C" fn nodegraph_delete_node(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = refany.node_id;

    let Some(mut backref) = refany.backref.downcast_mut::<NodeGraphLocalDataset>() else {
        return Update::DoNothing;
    };

    let result = match backref.callbacks.on_node_removed.as_ref() {
        Some(OnNodeRemoved { callback, refany }) => (callback.cb)(refany.clone(), info, node_id),
        None => Update::DoNothing,
    };

    result
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
extern "C" fn nodegraph_context_menu_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::CursorPosition;

    let Some(mut refany) = refany.downcast_mut::<ContextMenuEntryLocalDataset>() else {
        return Update::DoNothing;
    };

    let new_node_type = refany.node_type;

    let Some(mut backref) = refany.backref.downcast_mut::<NodeGraphLocalDataset>() else {
        return Update::DoNothing;
    };

    // The wrapper root carries the render's marker prefix (see
    // `NodeGraph::dom`); resolving it is what tells us whether this graph is
    // actually in the DOM right now.
    let Some(node_graph_wrapper_id) =
        info.get_node_id_by_marker(backref.marker_prefix.clone())
    else {
        return Update::DoNothing;
    };

    let node_wrapper_offset = info
        .get_node_position(node_graph_wrapper_id)
        .map_or((0.0, 0.0), |p| (p.x, p.y));

    let cursor_in_viewport = match info.get_current_mouse_state().cursor_position {
        InWindow(i) => i,
        CursorPosition::OutOfWindow(i) => i,
        CursorPosition::Uninitialized => LogicalPosition::zero(),
    };

    let new_node_pos = NodeGraphNodePosition {
        x: (cursor_in_viewport.x - node_wrapper_offset.0) * (1.0 / backref.node_graph.scale_factor)
            - backref.node_graph.offset.x,
        y: (cursor_in_viewport.y - node_wrapper_offset.1) * (1.0 / backref.node_graph.scale_factor)
            - backref.node_graph.offset.y,
    };

    let new_node_id = backref.node_graph.generate_unique_node_id();

    let result = match backref.callbacks.on_node_added.as_ref() {
        Some(OnNodeAdded { callback, refany }) => (callback.cb)(
            refany.clone(),
            info,
            new_node_type,
            new_node_id,
            new_node_pos,
        ),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_input_output_connect(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    use self::InputOrOutput::{Input, Output};

    let Some(mut refany) = refany.downcast_mut::<NodeInputOutputLocalDataset>() else {
        return Update::DoNothing;
    };

    let io_id = refany.io_id;

    let Some(mut backref) = refany.backref.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = backref.node_id;

    let Some(mut backref) = backref.backref.downcast_mut::<NodeGraphLocalDataset>() else {
        return Update::DoNothing;
    };

    let (input_node, input_index, output_node, output_index) =
        match backref.last_input_or_output_clicked {
            None => {
                backref.last_input_or_output_clicked = Some((node_id, io_id));
                return Update::DoNothing;
            }
            Some((prev_node_id, prev_io_id)) => {
                match (prev_io_id, io_id) {
                    (Input(i), Output(o)) => (prev_node_id, i, node_id, o),
                    (Output(o), Input(i)) => (node_id, i, prev_node_id, o),
                    _ => {
                        // error: trying to connect input to input or output to output
                        backref.last_input_or_output_clicked = None;
                        return Update::DoNothing;
                    }
                }
            }
        };

    // verify that the nodetype matches
    match backref.node_graph.connect_input_output(
        input_node,
        input_index,
        output_node,
        output_index,
    ) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{e:?}");
            backref.last_input_or_output_clicked = None;
            return Update::DoNothing;
        }
    }

    let result = match backref.callbacks.on_node_connected.as_ref() {
        Some(OnNodeConnected { callback, refany }) => {
            let r = (callback.cb)(
                refany.clone(),
                info,
                input_node,
                input_index,
                output_node,
                output_index,
            );
            backref.last_input_or_output_clicked = None;
            r
        }
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_input_output_disconnect(mut refany: RefAny, info: CallbackInfo) -> Update {
    use self::InputOrOutput::{Input, Output};

    let Some(mut refany) = refany.downcast_mut::<NodeInputOutputLocalDataset>() else {
        return Update::DoNothing;
    };

    let io_id = refany.io_id;

    let Some(mut backref) = refany.backref.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = backref.node_id;

    let Some(mut backref) = backref.backref.downcast_mut::<NodeGraphLocalDataset>() else {
        return Update::DoNothing;
    };

    let mut result = Update::DoNothing;
    match io_id {
        Input(i) => {
            result.max_self(
                match backref.callbacks.on_node_input_disconnected.as_ref() {
                    Some(OnNodeInputDisconnected { callback, refany }) => {
                        (callback.cb)(refany.clone(), info, node_id, i)
                    }
                    None => Update::DoNothing,
                },
            );
        }
        Output(o) => {
            result.max_self(
                match backref.callbacks.on_node_output_disconnected.as_ref() {
                    Some(OnNodeOutputDisconnected { callback, refany }) => {
                        (callback.cb)(refany.clone(), info, node_id, o)
                    }
                    None => Update::DoNothing,
                },
            );
        }
    }

    result
}

extern "C" fn nodegraph_on_textinput_focus_lost(
    mut refany: RefAny,
    info: CallbackInfo,
    textinputstate: TextInputState,
) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeFieldLocalDataset>() else {
        return Update::DoNothing;
    };

    let field_idx = refany.field_idx;

    let Some(mut node_local_dataset) = refany.backref.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = node_local_dataset.node_id;

    let Some(mut node_graph) = node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    else {
        return Update::DoNothing;
    };

    let node_type = match node_graph
        .node_graph
        .nodes
        .iter()
        .find(|i| i.node_id == node_id)
    {
        Some(s) => s.node.node_type,
        None => return Update::DoNothing,
    };

    let result = match node_graph.callbacks.on_node_field_edited.as_ref() {
        Some(OnNodeFieldEdited { refany, callback }) => (callback.cb)(
            refany.clone(),
            info,
            node_id,
            field_idx,
            node_type,
            NodeTypeFieldValue::TextInput(textinputstate.get_text().into()),
        ),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_on_numberinput_focus_lost(
    mut refany: RefAny,
    info: CallbackInfo,
    numberinputstate: NumberInputState,
) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeFieldLocalDataset>() else {
        return Update::DoNothing;
    };

    let field_idx = refany.field_idx;

    let Some(mut node_local_dataset) = refany.backref.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = node_local_dataset.node_id;

    let Some(mut node_graph) = node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    else {
        return Update::DoNothing;
    };

    let node_type = match node_graph
        .node_graph
        .nodes
        .iter()
        .find(|i| i.node_id == node_id)
    {
        Some(s) => s.node.node_type,
        None => return Update::DoNothing,
    };

    let result = match node_graph.callbacks.on_node_field_edited.as_ref() {
        Some(OnNodeFieldEdited { refany, callback }) => (callback.cb)(
            refany.clone(),
            info,
            node_id,
            field_idx,
            node_type,
            NodeTypeFieldValue::NumberInput(numberinputstate.number),
        ),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_on_checkbox_value_changed(
    mut refany: RefAny,
    info: CallbackInfo,
    checkboxinputstate: CheckBoxState,
) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeFieldLocalDataset>() else {
        return Update::DoNothing;
    };

    let field_idx = refany.field_idx;

    let Some(mut node_local_dataset) = refany.backref.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = node_local_dataset.node_id;

    let Some(mut node_graph) = node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    else {
        return Update::DoNothing;
    };

    let node_type = match node_graph
        .node_graph
        .nodes
        .iter()
        .find(|i| i.node_id == node_id)
    {
        Some(s) => s.node.node_type,
        None => return Update::DoNothing,
    };

    let result = match node_graph.callbacks.on_node_field_edited.as_ref() {
        Some(OnNodeFieldEdited { refany, callback }) => (callback.cb)(
            refany.clone(),
            info,
            node_id,
            field_idx,
            node_type,
            NodeTypeFieldValue::CheckBox(checkboxinputstate.checked),
        ),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_on_colorinput_value_changed(
    mut refany: RefAny,
    info: CallbackInfo,
    colorinputstate: ColorInputState,
) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeFieldLocalDataset>() else {
        return Update::DoNothing;
    };

    let field_idx = refany.field_idx;

    let Some(mut node_local_dataset) = refany.backref.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = node_local_dataset.node_id;
    let Some(mut node_graph) = node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    else {
        return Update::DoNothing;
    };

    let node_type = match node_graph
        .node_graph
        .nodes
        .iter()
        .find(|i| i.node_id == node_id)
    {
        Some(s) => s.node.node_type,
        None => return Update::DoNothing,
    };

    let result = match node_graph.callbacks.on_node_field_edited.as_ref() {
        Some(OnNodeFieldEdited { refany, callback }) => (callback.cb)(
            refany.clone(),
            info,
            node_id,
            field_idx,
            node_type,
            NodeTypeFieldValue::ColorInput(colorinputstate.color),
        ),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_on_fileinput_button_clicked(
    mut refany: RefAny,
    info: CallbackInfo,
    file: FileInputState,
) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeFieldLocalDataset>() else {
        return Update::DoNothing;
    };

    let field_idx = refany.field_idx;

    let Some(mut node_local_dataset) = refany.backref.downcast_mut::<NodeLocalDataset>() else {
        return Update::DoNothing;
    };

    let node_id = node_local_dataset.node_id;
    let Some(mut node_graph) = node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    else {
        return Update::DoNothing;
    };

    let node_type = match node_graph
        .node_graph
        .nodes
        .iter()
        .find(|i| i.node_id == node_id)
    {
        Some(s) => s.node.node_type,
        None => return Update::DoNothing,
    };

    // If a new file was selected, invoke callback
    let result = match node_graph.callbacks.on_node_field_edited.as_ref() {
        Some(OnNodeFieldEdited { refany, callback }) => (callback.cb)(
            refany.clone(),
            info,
            node_id,
            field_idx,
            node_type,
            NodeTypeFieldValue::FileInput(file.path),
        ),
        None => return Update::DoNothing,
    };

    result
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// Two node types that are deliberately *type-incompatible*: `TYPE_A` speaks
    /// `IO_INT` on both ends, `TYPE_B` speaks `IO_FLOAT`. Every "mime type mismatch"
    /// assertion below is A-to-B; every legal connection is A-to-A.
    const TYPE_A: NodeTypeId = NodeTypeId { inner: 1 };
    const TYPE_B: NodeTypeId = NodeTypeId { inner: 2 };
    /// A node type id that is *never* registered in `node_types`.
    const TYPE_UNREGISTERED: NodeTypeId = NodeTypeId { inner: 99 };

    const IO_INT: InputOutputTypeId = InputOutputTypeId { inner: 10 };
    const IO_FLOAT: InputOutputTypeId = InputOutputTypeId { inner: 20 };
    /// An I/O type id that has no entry in `input_output_types` (so no color).
    const IO_COLORLESS: InputOutputTypeId = InputOutputTypeId { inner: 77 };

    const N1: NodeGraphNodeId = NodeGraphNodeId { inner: 1 };
    const N2: NodeGraphNodeId = NodeGraphNodeId { inner: 2 };
    const N3: NodeGraphNodeId = NodeGraphNodeId { inner: 3 };
    const N4: NodeGraphNodeId = NodeGraphNodeId { inner: 4 };
    /// A node id that is never in the graph.
    const MISSING: NodeGraphNodeId = NodeGraphNodeId { inner: 999 };

    /// The four geometry constants `get_rect` is built from, restated here so that a
    /// silent change to any of them fails the geometry tests loudly instead of
    /// silently re-deriving the "expected" value from the same source.
    const EXPECT_NODE_WIDTH: f32 = 250.0;
    const EXPECT_V_OFFSET: f32 = 71.0;
    const EXPECT_PORT_PITCH: f32 = 25.0; // DIST_BETWEEN_NODES + CONNECTION_DOT_HEIGHT
    const EXPECT_DOT_HEIGHT: f32 = 15.0;

    fn io_types() -> InputOutputTypeIdInfoMapVec {
        vec![
            InputOutputTypeIdInfoMap {
                io_type_id: IO_INT,
                io_info: InputOutputInfo {
                    data_type: AzString::from_const_str("int"),
                    color: ColorU {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 255,
                    },
                },
            },
            InputOutputTypeIdInfoMap {
                io_type_id: IO_FLOAT,
                io_info: InputOutputInfo {
                    data_type: AzString::from_const_str("float"),
                    color: ColorU {
                        r: 4,
                        g: 5,
                        b: 6,
                        a: 255,
                    },
                },
            },
        ]
        .into()
    }

    fn node_types() -> NodeTypeIdInfoMapVec {
        vec![
            NodeTypeIdInfoMap {
                node_type_id: TYPE_A,
                node_type_info: NodeTypeInfo {
                    is_root: true,
                    node_type_name: AzString::from_const_str("A"),
                    inputs: vec![IO_INT].into(),
                    outputs: vec![IO_INT].into(),
                },
            },
            NodeTypeIdInfoMap {
                node_type_id: TYPE_B,
                node_type_info: NodeTypeInfo {
                    is_root: false,
                    node_type_name: AzString::from_const_str("B"),
                    inputs: vec![IO_FLOAT].into(),
                    outputs: vec![IO_FLOAT].into(),
                },
            },
        ]
        .into()
    }

    fn mk_node(node_type: NodeTypeId, x: f32, y: f32) -> Node {
        Node {
            node_type,
            position: NodeGraphNodePosition { x, y },
            fields: NodeTypeFieldVec::new(),
            connect_in: InputConnectionVec::new(),
            connect_out: OutputConnectionVec::new(),
        }
    }

    /// Four nodes: `N1`, `N3`, `N4` are `TYPE_A` (int), `N2` is `TYPE_B` (float).
    /// So `N1 -> N3`, `N1 -> N4` and `N3 -> N4` are legal connections and anything
    /// touching `N2` is a mime-type mismatch.
    fn graph() -> NodeGraph {
        NodeGraph {
            node_types: node_types(),
            input_output_types: io_types(),
            nodes: vec![
                NodeIdNodeMap {
                    node_id: N1,
                    node: mk_node(TYPE_A, 0.0, 0.0),
                },
                NodeIdNodeMap {
                    node_id: N2,
                    node: mk_node(TYPE_B, 400.0, 100.0),
                },
                NodeIdNodeMap {
                    node_id: N3,
                    node: mk_node(TYPE_A, 800.0, 50.0),
                },
                NodeIdNodeMap {
                    node_id: N4,
                    node: mk_node(TYPE_A, -100.0, 200.0),
                },
            ]
            .into(),
            ..NodeGraph::default()
        }
    }

    /// `(input_index, [(output_node_id, output_index)])` for every input port of `id`.
    fn inputs_of(g: &NodeGraph, id: NodeGraphNodeId) -> Vec<(usize, Vec<(u64, usize)>)> {
        g.nodes
            .iter()
            .find(|n| n.node_id == id)
            .map_or_else(Vec::new, |n| {
                n.node
                    .connect_in
                    .iter()
                    .map(|c| {
                        (
                            c.input_index,
                            c.connects_to
                                .iter()
                                .map(|o| (o.node_id.inner, o.output_index))
                                .collect(),
                        )
                    })
                    .collect()
            })
    }

    /// `(output_index, [(input_node_id, input_index)])` for every output port of `id`.
    fn outputs_of(g: &NodeGraph, id: NodeGraphNodeId) -> Vec<(usize, Vec<(u64, usize)>)> {
        g.nodes
            .iter()
            .find(|n| n.node_id == id)
            .map_or_else(Vec::new, |n| {
                n.node
                    .connect_out
                    .iter()
                    .map(|c| {
                        (
                            c.output_index,
                            c.connects_to
                                .iter()
                                .map(|i| (i.node_id.inner, i.input_index))
                                .collect(),
                        )
                    })
                    .collect()
            })
    }

    /// The full wiring of the graph, as a comparable value — the "encoding" that the
    /// connect/disconnect round-trip tests compare before and after.
    type Wiring = Vec<(
        u64,
        Vec<(usize, Vec<(u64, usize)>)>,
        Vec<(usize, Vec<(u64, usize)>)>,
    )>;
    fn wiring(g: &NodeGraph) -> Wiring {
        g.nodes
            .iter()
            .map(|n| {
                (
                    n.node_id.inner,
                    inputs_of(g, n.node_id),
                    outputs_of(g, n.node_id),
                )
            })
            .collect()
    }

    /// Pushes an output connection *without* going through `connect_input_output`, so
    /// that structurally-impossible graphs (dangling target, out-of-range port, port
    /// with no registered color) can be handed to the renderers.
    fn force_out_connection(
        mut g: NodeGraph,
        from: NodeGraphNodeId,
        out_idx: usize,
        to: NodeGraphNodeId,
        in_idx: usize,
    ) -> NodeGraph {
        if let Some(n) = g.nodes.as_mut().iter_mut().find(|n| n.node_id == from) {
            n.node.connect_out.push(OutputConnection {
                output_index: out_idx,
                connects_to: vec![InputNodeAndIndex {
                    node_id: to,
                    input_index: in_idx,
                }]
                .into(),
            });
        }
        g
    }

    fn count_nodes(dom: &Dom) -> usize {
        1 + dom.children.iter().map(count_nodes).sum::<usize>()
    }

    /// A `RefAny<NodeGraphLocalDataset>` wrapping a snapshot of `g` — the payload every
    /// node-graph callback expects to find at the end of its `backref` chain.
    fn graph_dataset(g: &NodeGraph) -> RefAny {
        RefAny::new(NodeGraphLocalDataset {
            node_graph: g.clone(),
            last_input_or_output_clicked: None,
            active_node_being_dragged: None,
            marker_prefix: "azul-nodegraph-test".into(),
            callbacks: g.callbacks.clone(),
        })
    }

    /// Reads the graph back out of a `NodeGraphLocalDataset` handle.
    fn dataset_graph(handle: &RefAny) -> NodeGraph {
        let mut handle = handle.clone();
        let d = handle
            .downcast_ref::<NodeGraphLocalDataset>()
            .expect("not a NodeGraphLocalDataset");
        d.node_graph.clone()
    }

    /// `InputOrOutput` is not `PartialEq`, so flatten it into something that is.
    fn io_kind(io: InputOrOutput) -> (bool, usize) {
        match io {
            InputOrOutput::Input(i) => (true, i),
            InputOrOutput::Output(o) => (false, o),
        }
    }

    fn pending_click(handle: &RefAny) -> Option<(u64, (bool, usize))> {
        let mut handle = handle.clone();
        let d = handle
            .downcast_ref::<NodeGraphLocalDataset>()
            .expect("not a NodeGraphLocalDataset");
        d.last_input_or_output_clicked
            .map(|(id, io)| (id.inner, io_kind(io)))
    }

    // ------------------------------------------------------------------
    // Callback harness (mirrors the one in check_box.rs / color_input.rs)
    // ------------------------------------------------------------------

    /// A `DomNodeId` whose node component is `None` — "no concrete node was hit".
    fn hit_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Runs `f` with a `CallbackInfo` whose window holds `styled_dom` as the root DOM.
    /// `previous_window_state` is deliberately `None`, which is what makes
    /// `get_previous_mouse_state()` return `None` in the drag tests.
    fn with_info<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled_dom));

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let r = f(&mut info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    /// Shorthand for "deliver one event to `cb` with an otherwise-empty window".
    fn fire(cb: impl FnOnce(CallbackInfo) -> Update) -> Update {
        with_info(StyledDom::default(), hit_none(), |info| cb(*info)).0
    }

    // ------------------------------------------------------------------
    // User-callback recorder
    // ------------------------------------------------------------------

    /// Everything the widget's user-facing callbacks were handed, in call order.
    #[derive(Debug, Default)]
    struct Log {
        removed: Vec<u64>,
        added: Vec<(u64, u64, f32, f32)>,
        connected: Vec<(u64, usize, u64, usize)>,
        input_disconnected: Vec<(u64, usize)>,
        output_disconnected: Vec<(u64, usize)>,
        /// `(node_id, field_idx, node_type)` of every `on_node_field_edited` call.
        edited: Vec<(u64, usize, u64)>,
        text_values: Vec<String>,
        number_values: Vec<f32>,
        bool_values: Vec<bool>,
        color_values: Vec<(u8, u8, u8, u8)>,
        file_values: Vec<Option<String>>,
    }

    fn log_of(handle: &RefAny, f: impl FnOnce(&Log)) {
        let mut handle = handle.clone();
        let l = handle.downcast_ref::<Log>().expect("not a Log");
        f(&l);
    }

    extern "C" fn rec_removed(
        mut refany: RefAny,
        _info: CallbackInfo,
        node_id: NodeGraphNodeId,
    ) -> Update {
        if let Some(mut l) = refany.downcast_mut::<Log>() {
            l.removed.push(node_id.inner);
        }
        Update::RefreshDom
    }

    extern "C" fn rec_added(
        mut refany: RefAny,
        _info: CallbackInfo,
        new_node_type: NodeTypeId,
        new_node_id: NodeGraphNodeId,
        new_node_position: NodeGraphNodePosition,
    ) -> Update {
        if let Some(mut l) = refany.downcast_mut::<Log>() {
            l.added.push((
                new_node_type.inner,
                new_node_id.inner,
                new_node_position.x,
                new_node_position.y,
            ));
        }
        Update::RefreshDomAllWindows
    }

    extern "C" fn rec_connected(
        mut refany: RefAny,
        _info: CallbackInfo,
        input: NodeGraphNodeId,
        input_index: usize,
        output: NodeGraphNodeId,
        output_index: usize,
    ) -> Update {
        if let Some(mut l) = refany.downcast_mut::<Log>() {
            l.connected
                .push((input.inner, input_index, output.inner, output_index));
        }
        Update::RefreshDom
    }

    extern "C" fn rec_input_disconnected(
        mut refany: RefAny,
        _info: CallbackInfo,
        input: NodeGraphNodeId,
        input_index: usize,
    ) -> Update {
        if let Some(mut l) = refany.downcast_mut::<Log>() {
            l.input_disconnected.push((input.inner, input_index));
        }
        Update::RefreshDom
    }

    extern "C" fn rec_output_disconnected(
        mut refany: RefAny,
        _info: CallbackInfo,
        output: NodeGraphNodeId,
        output_index: usize,
    ) -> Update {
        if let Some(mut l) = refany.downcast_mut::<Log>() {
            l.output_disconnected.push((output.inner, output_index));
        }
        Update::RefreshDomAllWindows
    }

    extern "C" fn rec_field_edited(
        mut refany: RefAny,
        _info: CallbackInfo,
        node_id: NodeGraphNodeId,
        field_id: usize,
        node_type: NodeTypeId,
        new_value: NodeTypeFieldValue,
    ) -> Update {
        if let Some(mut l) = refany.downcast_mut::<Log>() {
            l.edited.push((node_id.inner, field_id, node_type.inner));
            match new_value {
                NodeTypeFieldValue::TextInput(s) => l.text_values.push(s.as_str().to_string()),
                NodeTypeFieldValue::NumberInput(n) => l.number_values.push(n),
                NodeTypeFieldValue::CheckBox(b) => l.bool_values.push(b),
                NodeTypeFieldValue::ColorInput(c) => l.color_values.push((c.r, c.g, c.b, c.a)),
                NodeTypeFieldValue::FileInput(p) => l
                    .file_values
                    .push(p.as_ref().map(|s| s.as_str().to_string())),
            }
        }
        Update::RefreshDom
    }

    /// A graph whose callbacks all funnel into one freshly-created `Log`.
    fn graph_with_log() -> (NodeGraph, RefAny) {
        let log = RefAny::new(Log::default());
        let mut g = graph();
        g.callbacks = NodeGraphCallbacks {
            on_node_removed: OptionOnNodeRemoved::Some(OnNodeRemoved {
                refany: log.clone(),
                callback: OnNodeRemovedCallback {
                    cb: rec_removed,
                    ctx: OptionRefAny::None,
                },
            }),
            on_node_added: OptionOnNodeAdded::Some(OnNodeAdded {
                refany: log.clone(),
                callback: OnNodeAddedCallback {
                    cb: rec_added,
                    ctx: OptionRefAny::None,
                },
            }),
            on_node_connected: OptionOnNodeConnected::Some(OnNodeConnected {
                refany: log.clone(),
                callback: OnNodeConnectedCallback {
                    cb: rec_connected,
                    ctx: OptionRefAny::None,
                },
            }),
            on_node_input_disconnected: OptionOnNodeInputDisconnected::Some(
                OnNodeInputDisconnected {
                    refany: log.clone(),
                    callback: OnNodeInputDisconnectedCallback {
                        cb: rec_input_disconnected,
                        ctx: OptionRefAny::None,
                    },
                },
            ),
            on_node_output_disconnected: OptionOnNodeOutputDisconnected::Some(
                OnNodeOutputDisconnected {
                    refany: log.clone(),
                    callback: OnNodeOutputDisconnectedCallback {
                        cb: rec_output_disconnected,
                        ctx: OptionRefAny::None,
                    },
                },
            ),
            on_node_field_edited: OptionOnNodeFieldEdited::Some(OnNodeFieldEdited {
                refany: log.clone(),
                callback: OnNodeFieldEditedCallback {
                    cb: rec_field_edited,
                    ctx: OptionRefAny::None,
                },
            }),
            ..NodeGraphCallbacks::default()
        };
        (g, log)
    }

    // ==================================================================
    // 1. NodeGraph::generate_unique_node_id
    // ==================================================================

    #[test]
    fn generate_unique_node_id_on_an_empty_graph_is_one_not_zero() {
        // `0` is a perfectly valid node id, so the generator must not hand it out for
        // the first node either — `max().unwrap_or(0) + 1`.
        assert_eq!(NodeGraph::default().generate_unique_node_id().inner, 1);
    }

    #[test]
    fn generate_unique_node_id_returns_max_plus_one_and_ignores_gaps_and_order() {
        // Ids are deliberately unsorted and non-contiguous: the generator must take the
        // maximum, not the last element and not the length.
        let g = NodeGraph {
            nodes: vec![
                NodeIdNodeMap {
                    node_id: NodeGraphNodeId { inner: 7 },
                    node: mk_node(TYPE_A, 0.0, 0.0),
                },
                NodeIdNodeMap {
                    node_id: NodeGraphNodeId { inner: 0 },
                    node: mk_node(TYPE_A, 0.0, 0.0),
                },
                NodeIdNodeMap {
                    node_id: NodeGraphNodeId { inner: 3 },
                    node: mk_node(TYPE_A, 0.0, 0.0),
                },
            ]
            .into(),
            ..Default::default()
        };
        assert_eq!(g.generate_unique_node_id().inner, 8);
    }

    #[test]
    fn generate_unique_node_id_tolerates_duplicate_ids_in_the_graph() {
        let g = NodeGraph {
            nodes: vec![
                NodeIdNodeMap {
                    node_id: N2,
                    node: mk_node(TYPE_A, 0.0, 0.0),
                },
                NodeIdNodeMap {
                    node_id: N2,
                    node: mk_node(TYPE_A, 0.0, 0.0),
                },
            ]
            .into(),
            ..Default::default()
        };
        assert_eq!(g.generate_unique_node_id().inner, 3);
    }

    #[test]
    fn generate_unique_node_id_saturates_instead_of_overflowing_at_u64_max() {
        // `saturating_add(1)` means the id at the top of the range is NOT unique: it
        // collides with the existing node. That is a real (if unreachable in practice)
        // limitation — what matters here is that it saturates rather than wrapping to
        // 0 or panicking in a debug build.
        let mut g = NodeGraph {
            nodes: vec![NodeIdNodeMap {
                node_id: NodeGraphNodeId { inner: u64::MAX },
                node: mk_node(TYPE_A, 0.0, 0.0),
            }]
            .into(),
            ..Default::default()
        };
        let id = g.generate_unique_node_id();
        assert_eq!(id.inner, u64::MAX);
        assert!(
            g.nodes.iter().any(|n| n.node_id == id),
            "at u64::MAX the generated id collides — documented saturation, not wraparound",
        );

        // ...and one below the top still behaves normally.
        g.nodes = vec![NodeIdNodeMap {
            node_id: NodeGraphNodeId {
                inner: u64::MAX - 1,
            },
            node: mk_node(TYPE_A, 0.0, 0.0),
        }]
        .into();
        assert_eq!(g.generate_unique_node_id().inner, u64::MAX);
    }

    #[test]
    fn generate_unique_node_id_is_pure_and_repeats_until_the_node_is_inserted() {
        let g = graph();
        let first = g.generate_unique_node_id();
        assert_eq!(first, g.generate_unique_node_id());
        assert_eq!(first.inner, 5); // max(1,2,3,4) + 1
    }

    // ==================================================================
    // 2. NodeGraphError: Display / Debug
    // ==================================================================

    const ALL_ERRORS: [NodeGraphError; 4] = [
        NodeGraphError::NodeMimeTypeMismatch,
        NodeGraphError::NodeInvalidIndex,
        NodeGraphError::NodeInvalidNode,
        NodeGraphError::NoRootNode,
    ];

    #[test]
    fn node_graph_error_display_is_non_empty_ascii_and_single_line() {
        for e in ALL_ERRORS {
            let s = format!("{e}");
            assert!(!s.is_empty(), "{e:?} formatted to the empty string");
            assert!(!s.contains('\n'), "{e:?} formatted to a multi-line string");
            assert!(s.is_ascii(), "{e:?} formatted to non-ascii: {s}");
        }
    }

    #[test]
    fn node_graph_error_display_distinguishes_every_variant() {
        // A copy-pasted match arm that returns the same message for two variants would
        // make the error useless in a log; this is the assertion that catches it.
        let mut seen: Vec<String> = ALL_ERRORS.iter().map(|e| format!("{e}")).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), ALL_ERRORS.len());
    }

    #[test]
    fn node_graph_error_display_survives_width_precision_and_fill_specifiers() {
        // `write!` inside a Display impl ignores the outer format spec, but the spec
        // must not make the impl panic or truncate to nothing.
        for e in ALL_ERRORS {
            assert!(!format!("{e:>80}").is_empty());
            assert!(!format!("{e:*^3}").is_empty());
            assert!(!format!("{e:.1}").is_empty());
            assert!(!format!("{e:?}").is_empty());
        }
    }

    #[test]
    fn node_graph_error_debug_and_display_are_both_usable_and_differ_in_style() {
        // Debug is the derived variant name; Display is prose. They should not be the
        // same string, otherwise one of the two impls is missing.
        for e in ALL_ERRORS {
            assert_ne!(format!("{e:?}"), format!("{e}"));
        }
    }

    // ==================================================================
    // 3. NodeGraph::swap_with_default
    // ==================================================================

    /// Everything about a `NodeGraph` that is cheaply comparable.
    fn summary(g: &NodeGraph) -> (usize, usize, usize, bool, f32, f32, f32, String) {
        (
            g.node_types.len(),
            g.input_output_types.len(),
            g.nodes.len(),
            g.allow_multiple_root_nodes,
            g.offset.x,
            g.offset.y,
            g.scale_factor,
            g.add_node_str.as_str().to_string(),
        )
    }

    fn distinctive() -> NodeGraph {
        NodeGraph {
            allow_multiple_root_nodes: true,
            offset: LogicalPosition { x: -3.5, y: 12.25 },
            scale_factor: 2.5,
            add_node_str: AzString::from_const_str("Ajouter un nœud"),
            ..graph()
        }
    }

    #[test]
    fn swap_with_default_hands_back_the_old_value_and_leaves_a_default_behind() {
        let mut g = distinctive();
        let expected = summary(&g);

        let taken = g.swap_with_default();

        assert_eq!(summary(&taken), expected);
        assert_eq!(summary(&g), summary(&NodeGraph::default()));
    }

    #[test]
    fn swap_with_default_round_trips_a_graph_through_two_owners() {
        // encode == decode: moving a graph out and back must not lose a single field.
        let mut a = distinctive();
        let expected = summary(&a);

        let mut b = a.swap_with_default(); // a := default, b := original
        let c = b.swap_with_default(); // b := default, c := original again

        assert_eq!(summary(&c), expected);
        assert_eq!(summary(&b), summary(&NodeGraph::default()));
        assert_eq!(summary(&a), summary(&NodeGraph::default()));
    }

    #[test]
    fn swap_with_default_on_an_already_default_graph_is_a_no_op() {
        let mut g = NodeGraph::default();
        let taken = g.swap_with_default();
        assert_eq!(summary(&taken), summary(&NodeGraph::default()));
        assert_eq!(summary(&g), summary(&NodeGraph::default()));
    }

    #[test]
    fn swap_with_default_preserves_non_finite_offsets_and_scale_verbatim() {
        // The swap is a `mem::swap`, so NaN/inf must survive bit-for-bit rather than
        // being normalised away.
        let mut g = NodeGraph {
            offset: LogicalPosition {
                x: f32::INFINITY,
                y: f32::NEG_INFINITY,
            },
            scale_factor: f32::NAN,
            ..NodeGraph::default()
        };
        let taken = g.swap_with_default();
        assert!(taken.offset.x.is_infinite() && taken.offset.x.is_sign_positive());
        assert!(taken.offset.y.is_infinite() && taken.offset.y.is_sign_negative());
        assert!(taken.scale_factor.is_nan());
        assert_eq!(g.scale_factor, 1.0);
    }

    #[test]
    fn swap_with_default_moves_the_connections_not_just_the_node_list() {
        let mut g = graph();
        g.connect_input_output(N3, 0, N1, 0)
            .expect("legal A->A wire");
        let before = wiring(&g);

        let taken = g.swap_with_default();

        assert_eq!(wiring(&taken), before);
        assert!(g.nodes.is_empty());
    }

    // ==================================================================
    // 4. NodeGraph::verify_nodetype_match
    // ==================================================================

    #[test]
    fn verify_nodetype_match_accepts_matching_types_at_index_zero() {
        let g = graph();
        assert_eq!(g.verify_nodetype_match(N1, 0, N3, 0), Ok(()));
    }

    #[test]
    fn verify_nodetype_match_rejects_a_type_mismatch() {
        let g = graph();
        // N1 emits `int`, N2 consumes `float`.
        assert_eq!(
            g.verify_nodetype_match(N1, 0, N2, 0),
            Err(NodeGraphError::NodeMimeTypeMismatch)
        );
    }

    #[test]
    fn verify_nodetype_match_reports_a_missing_node_on_either_side() {
        let g = graph();
        assert_eq!(
            g.verify_nodetype_match(MISSING, 0, N3, 0),
            Err(NodeGraphError::NodeInvalidNode)
        );
        assert_eq!(
            g.verify_nodetype_match(N1, 0, MISSING, 0),
            Err(NodeGraphError::NodeInvalidNode)
        );
    }

    #[test]
    fn verify_nodetype_match_reports_a_node_whose_type_is_not_registered() {
        let mut g = graph();
        g.nodes.push(NodeIdNodeMap {
            node_id: NodeGraphNodeId { inner: 50 },
            node: mk_node(TYPE_UNREGISTERED, 0.0, 0.0),
        });
        assert_eq!(
            g.verify_nodetype_match(NodeGraphNodeId { inner: 50 }, 0, N3, 0),
            Err(NodeGraphError::NodeInvalidNode)
        );
        assert_eq!(
            g.verify_nodetype_match(N1, 0, NodeGraphNodeId { inner: 50 }, 0),
            Err(NodeGraphError::NodeInvalidNode)
        );
    }

    #[test]
    fn verify_nodetype_match_rejects_out_of_range_port_indices() {
        let g = graph();
        // Both node types declare exactly one input and one output, so index 1 is the
        // first out-of-range index.
        assert_eq!(
            g.verify_nodetype_match(N1, 1, N3, 0),
            Err(NodeGraphError::NodeInvalidIndex)
        );
        assert_eq!(
            g.verify_nodetype_match(N1, 0, N3, 1),
            Err(NodeGraphError::NodeInvalidIndex)
        );
    }

    #[test]
    fn verify_nodetype_match_does_not_panic_at_usize_max_indices() {
        // `Vec::get(usize::MAX)` must be the thing that fails, not an unchecked index.
        let g = graph();
        assert_eq!(
            g.verify_nodetype_match(N1, usize::MAX, N3, 0),
            Err(NodeGraphError::NodeInvalidIndex)
        );
        assert_eq!(
            g.verify_nodetype_match(N1, 0, N3, usize::MAX),
            Err(NodeGraphError::NodeInvalidIndex)
        );
        assert_eq!(
            g.verify_nodetype_match(N1, usize::MAX, N3, usize::MAX),
            Err(NodeGraphError::NodeInvalidIndex)
        );
    }

    #[test]
    fn verify_nodetype_match_checks_nodes_before_indices() {
        // Ordering matters for the error a user sees: a missing node is reported even
        // when the index is also nonsense.
        let g = graph();
        assert_eq!(
            g.verify_nodetype_match(MISSING, usize::MAX, N3, usize::MAX),
            Err(NodeGraphError::NodeInvalidNode)
        );
    }

    #[test]
    fn verify_nodetype_match_allows_a_node_to_be_wired_to_itself() {
        // Documented behaviour: there is no self-loop / cycle check at this layer.
        let g = graph();
        assert_eq!(g.verify_nodetype_match(N1, 0, N1, 0), Ok(()));
    }

    #[test]
    fn verify_nodetype_match_does_not_mutate_the_graph() {
        let g = graph();
        let before = wiring(&g);
        let _ = g.verify_nodetype_match(N1, 0, N3, 0);
        let _ = g.verify_nodetype_match(MISSING, usize::MAX, N2, 9);
        assert_eq!(wiring(&g), before);
    }

    // ==================================================================
    // 5. NodeGraph::connect_input_output
    // ==================================================================

    #[test]
    fn connect_input_output_wires_both_directions_at_index_zero() {
        let mut g = graph();
        assert_eq!(g.connect_input_output(N3, 0, N1, 0), Ok(()));

        assert_eq!(inputs_of(&g, N3), vec![(0, vec![(N1.inner, 0)])]);
        assert_eq!(outputs_of(&g, N1), vec![(0, vec![(N3.inner, 0)])]);
        // ...and nothing else moved.
        assert!(outputs_of(&g, N3).is_empty());
        assert!(inputs_of(&g, N1).is_empty());
    }

    #[test]
    fn connect_input_output_rejects_a_mime_type_mismatch_without_mutating() {
        let mut g = graph();
        let before = wiring(&g);
        assert_eq!(
            g.connect_input_output(N2, 0, N1, 0),
            Err(NodeGraphError::NodeMimeTypeMismatch)
        );
        assert_eq!(wiring(&g), before, "a rejected connect must be atomic");
    }

    #[test]
    fn connect_input_output_rejects_missing_nodes_without_mutating() {
        for (input, output) in [(MISSING, N1), (N3, MISSING), (MISSING, MISSING)] {
            let mut g = graph();
            let before = wiring(&g);
            assert_eq!(
                g.connect_input_output(input, 0, output, 0),
                Err(NodeGraphError::NodeInvalidNode)
            );
            assert_eq!(wiring(&g), before);
        }
    }

    #[test]
    fn connect_input_output_rejects_out_of_range_and_usize_max_indices() {
        for (in_idx, out_idx) in [
            (1_usize, 0_usize),
            (0, 1),
            (usize::MAX, 0),
            (0, usize::MAX),
            (usize::MAX, usize::MAX),
        ] {
            let mut g = graph();
            let before = wiring(&g);
            assert_eq!(
                g.connect_input_output(N3, in_idx, N1, out_idx),
                Err(NodeGraphError::NodeInvalidIndex),
                "in={in_idx} out={out_idx}",
            );
            assert_eq!(wiring(&g), before);
        }
    }

    #[test]
    fn connect_input_output_appends_to_an_existing_port_rather_than_replacing_it() {
        // Two different sources feeding the same input port must both be recorded.
        let mut g = graph();
        g.connect_input_output(N4, 0, N1, 0).expect("N1 -> N4");
        g.connect_input_output(N4, 0, N3, 0).expect("N3 -> N4");

        assert_eq!(
            inputs_of(&g, N4),
            vec![(0, vec![(N1.inner, 0), (N3.inner, 0)])],
            "the second wire must not overwrite the first",
        );
        assert_eq!(outputs_of(&g, N1), vec![(0, vec![(N4.inner, 0)])]);
        assert_eq!(outputs_of(&g, N3), vec![(0, vec![(N4.inner, 0)])]);
    }

    #[test]
    fn connect_input_output_records_a_duplicate_wire_twice() {
        // Documented behaviour: there is no de-duplication, so connecting the same two
        // ports twice yields two identical entries on both sides.
        let mut g = graph();
        g.connect_input_output(N3, 0, N1, 0).expect("first");
        g.connect_input_output(N3, 0, N1, 0).expect("second");

        assert_eq!(
            inputs_of(&g, N3),
            vec![(0, vec![(N1.inner, 0), (N1.inner, 0)])]
        );
        assert_eq!(
            outputs_of(&g, N1),
            vec![(0, vec![(N3.inner, 0), (N3.inner, 0)])]
        );
    }

    #[test]
    fn connect_input_output_permits_a_self_loop() {
        // No cycle detection at this layer — the node ends up wired to itself.
        let mut g = graph();
        assert_eq!(g.connect_input_output(N1, 0, N1, 0), Ok(()));
        assert_eq!(inputs_of(&g, N1), vec![(0, vec![(N1.inner, 0)])]);
        assert_eq!(outputs_of(&g, N1), vec![(0, vec![(N1.inner, 0)])]);
    }

    // ==================================================================
    // 6. NodeGraph::disconnect_input
    // ==================================================================

    #[test]
    fn disconnect_input_round_trips_a_single_connection() {
        // encode == decode: connect then disconnect restores the exact wiring.
        let mut g = graph();
        let before = wiring(&g);

        g.connect_input_output(N3, 0, N1, 0).expect("connect");
        assert_ne!(wiring(&g), before);

        assert_eq!(g.disconnect_input(N3, 0), Ok(()));
        assert_eq!(wiring(&g), before);
    }

    #[test]
    fn disconnect_input_reports_a_missing_node() {
        let mut g = graph();
        assert_eq!(
            g.disconnect_input(MISSING, 0),
            Err(NodeGraphError::NodeInvalidNode)
        );
    }

    #[test]
    fn disconnect_input_on_an_unconnected_port_is_ok_and_changes_nothing() {
        let mut g = graph();
        let before = wiring(&g);
        assert_eq!(g.disconnect_input(N3, 0), Ok(()));
        assert_eq!(wiring(&g), before);
    }

    #[test]
    fn disconnect_input_at_usize_max_is_ok_rather_than_invalid_index() {
        // Documented behaviour: an index that is not present short-circuits to `Ok(())`
        // *before* any range validation, so even `usize::MAX` is accepted silently.
        let mut g = graph();
        let before = wiring(&g);
        assert_eq!(g.disconnect_input(N3, usize::MAX), Ok(()));
        assert_eq!(wiring(&g), before);
    }

    #[test]
    fn disconnect_input_clears_every_source_feeding_that_port() {
        let mut g = graph();
        let before = wiring(&g);
        g.connect_input_output(N4, 0, N1, 0).expect("N1 -> N4");
        g.connect_input_output(N4, 0, N3, 0).expect("N3 -> N4");

        assert_eq!(g.disconnect_input(N4, 0), Ok(()));
        assert_eq!(
            wiring(&g),
            before,
            "both upstream ports must be released, not just the first",
        );
    }

    #[test]
    fn disconnect_input_orphans_a_sibling_sharing_the_same_output_port() {
        // BUG (characterised, not endorsed): `disconnect_input` removes the *whole*
        // `OutputConnection` entry of the upstream port instead of removing just the
        // one `InputNodeAndIndex` that pointed back. When two inputs are fed by the
        // same output, disconnecting one of them silently drops the other's
        // forward edge while leaving its backward edge in place — the two halves of
        // the graph disagree afterwards.
        let mut g = graph();
        g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
        g.connect_input_output(N4, 0, N1, 0).expect("N1 -> N4");
        assert_eq!(
            outputs_of(&g, N1),
            vec![(0, vec![(N3.inner, 0), (N4.inner, 0)])]
        );

        assert_eq!(g.disconnect_input(N3, 0), Ok(()));

        assert!(inputs_of(&g, N3).is_empty(), "the requested edge is gone");
        assert_eq!(
            inputs_of(&g, N4),
            vec![(0, vec![(N1.inner, 0)])],
            "N4 still believes it is connected to N1",
        );
        assert!(
            outputs_of(&g, N1).is_empty(),
            "...but N1 no longer lists N4 — the collateral damage this test pins down",
        );
    }

    // ==================================================================
    // 7. NodeGraph::disconnect_output
    // ==================================================================

    #[test]
    fn disconnect_output_round_trips_a_single_connection() {
        let mut g = graph();
        let before = wiring(&g);

        g.connect_input_output(N3, 0, N1, 0).expect("connect");
        assert_eq!(g.disconnect_output(N1, 0), Ok(()));

        assert_eq!(wiring(&g), before);
    }

    #[test]
    fn disconnect_output_reports_a_missing_node() {
        let mut g = graph();
        assert_eq!(
            g.disconnect_output(MISSING, 0),
            Err(NodeGraphError::NodeInvalidNode)
        );
    }

    #[test]
    fn disconnect_output_on_an_unconnected_port_is_ok_and_changes_nothing() {
        let mut g = graph();
        let before = wiring(&g);
        assert_eq!(g.disconnect_output(N1, 0), Ok(()));
        assert_eq!(wiring(&g), before);
    }

    #[test]
    fn disconnect_output_at_usize_max_is_ok_rather_than_invalid_index() {
        let mut g = graph();
        let before = wiring(&g);
        assert_eq!(g.disconnect_output(N1, usize::MAX), Ok(()));
        assert_eq!(wiring(&g), before);
    }

    #[test]
    fn disconnect_output_releases_every_downstream_input_it_fed() {
        // The mirror image of `disconnect_input_orphans_a_sibling...`: here the fan-out
        // case *is* handled correctly, because the loop walks the cloned target list.
        let mut g = graph();
        let before = wiring(&g);
        g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
        g.connect_input_output(N4, 0, N1, 0).expect("N1 -> N4");

        assert_eq!(g.disconnect_output(N1, 0), Ok(()));
        assert_eq!(wiring(&g), before, "no dangling back-reference may survive");
    }

    #[test]
    fn disconnect_output_of_a_self_loop_leaves_no_residue() {
        let mut g = graph();
        let before = wiring(&g);
        g.connect_input_output(N1, 0, N1, 0).expect("self loop");
        assert_eq!(g.disconnect_output(N1, 0), Ok(()));
        assert_eq!(wiring(&g), before);
    }

    // ==================================================================
    // 8. get_rect
    // ==================================================================

    fn connection(
        out: NodeGraphNodeId,
        out_idx: usize,
        inn: NodeGraphNodeId,
        in_idx: usize,
    ) -> ConnectionLocalDataset {
        ConnectionLocalDataset {
            out_node_id: out,
            out_idx,
            in_node_id: inn,
            in_idx,
            // Deliberately wrong: `get_rect` must recompute both flags from geometry.
            swap_vert: true,
            swap_horz: true,
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        }
    }

    #[test]
    fn get_rect_returns_none_for_a_dangling_endpoint() {
        let g = graph();
        assert!(get_rect(&g, connection(MISSING, 0, N3, 0)).is_none());
        assert!(get_rect(&g, connection(N1, 0, MISSING, 0)).is_none());
        assert!(get_rect(&g, connection(MISSING, 0, MISSING, 0)).is_none());
    }

    #[test]
    fn get_rect_computes_the_bounding_box_of_the_two_ports() {
        // N1 sits at (0, 0), N3 at (800, 50); both use port 0.
        let g = graph();
        let (rect, swap_vert, swap_horz) =
            get_rect(&g, connection(N1, 0, N3, 0)).expect("both nodes exist");

        let x_out = 0.0 + EXPECT_NODE_WIDTH;
        let y_out = 0.0 + EXPECT_V_OFFSET;
        let x_in = 800.0;
        let y_in = 50.0 + EXPECT_V_OFFSET;

        assert_eq!(rect.origin.x, x_out.min(x_in));
        assert_eq!(rect.origin.y, y_out.min(y_in));
        assert_eq!(rect.size.width, (x_in - x_out).abs());
        assert_eq!(rect.size.height, (y_in - y_out).abs() + EXPECT_DOT_HEIGHT);
        assert!(swap_vert, "the input port sits below the output port");
        assert!(!swap_horz, "the input node is to the right of the output");
    }

    #[test]
    fn get_rect_recomputes_the_swap_flags_and_ignores_the_ones_it_was_handed() {
        // The fixture passes `swap_vert: true, swap_horz: true` every time; here both
        // must come back `false`, proving the incoming values are not echoed.
        let g = graph();
        // N3 (800, 50) -> N1 (0, 0): input is left of, and above, the output.
        let (_, swap_vert, swap_horz) = get_rect(&g, connection(N3, 0, N1, 0)).expect("exists");
        assert!(!swap_vert);
        assert!(swap_horz);
    }

    #[test]
    fn get_rect_height_is_never_below_the_connection_dot() {
        // Two nodes at the same height give a zero-height span; the dot height is the
        // floor that keeps the rect drawable.
        let mut g = graph();
        g.nodes.as_mut()[2].node.position = NodeGraphNodePosition { x: 800.0, y: 0.0 };
        let (rect, _, _) = get_rect(&g, connection(N1, 0, N3, 0)).expect("exists");
        assert_eq!(rect.size.height, EXPECT_DOT_HEIGHT);
    }

    #[test]
    fn get_rect_port_index_shifts_the_endpoint_by_a_fixed_pitch() {
        // N1's output port (y = 71) is the topmost point of the rect; moving N3's input
        // down by three port pitches must therefore grow the height by exactly three
        // pitches and leave the origin where it was.
        let g = graph();
        let (base, _, _) = get_rect(&g, connection(N1, 0, N3, 0)).expect("exists");
        let (shifted, _, _) = get_rect(&g, connection(N1, 0, N3, 3)).expect("exists");

        assert_eq!(shifted.origin.y, base.origin.y);
        assert_eq!(
            shifted.size.height - base.size.height,
            3.0 * EXPECT_PORT_PITCH,
        );
    }

    #[test]
    fn get_rect_stays_finite_at_usize_max_port_indices() {
        // `usize::MAX as f32` is ~1.8e19 — large, but multiplying by the 25px pitch
        // still lands well inside f32 range, so nothing may become inf or NaN.
        let g = graph();
        let (rect, _, _) =
            get_rect(&g, connection(N1, usize::MAX, N3, usize::MAX)).expect("both nodes exist");
        assert!(rect.origin.y.is_finite(), "y = {}", rect.origin.y);
        assert!(rect.size.height.is_finite(), "h = {}", rect.size.height);
        assert!(rect.size.width.is_finite());
        assert!(rect.size.height >= EXPECT_DOT_HEIGHT);
    }

    #[test]
    fn get_rect_with_a_nan_position_yields_nan_extent_but_a_finite_origin() {
        // `f32::min` returns the non-NaN operand, so the origin survives even though
        // the extent does not. Neither may panic.
        let mut g = graph();
        g.nodes.as_mut()[2].node.position = NodeGraphNodePosition {
            x: f32::NAN,
            y: f32::NAN,
        };
        let (rect, swap_vert, swap_horz) = get_rect(&g, connection(N1, 0, N3, 0)).expect("exists");

        assert!(rect.size.width.is_nan());
        assert!(rect.size.height.is_nan());
        assert_eq!(rect.origin.x, EXPECT_NODE_WIDTH);
        assert_eq!(rect.origin.y, EXPECT_V_OFFSET);
        // NaN compares false against everything, so both flags fall to `false`.
        assert!(!swap_vert);
        assert!(!swap_horz);
    }

    #[test]
    fn get_rect_with_infinite_positions_does_not_panic() {
        let mut g = graph();
        g.nodes.as_mut()[2].node.position = NodeGraphNodePosition {
            x: f32::INFINITY,
            y: f32::NEG_INFINITY,
        };
        let (rect, swap_vert, swap_horz) = get_rect(&g, connection(N1, 0, N3, 0)).expect("exists");
        assert!(rect.size.width.is_infinite());
        assert!(rect.size.height.is_infinite());
        assert!(!swap_vert, "-inf is not above the output port");
        assert!(!swap_horz, "+inf is not left of the output port");

        // Both endpoints infinite in the same direction => inf - inf => NaN extent.
        g.nodes.as_mut()[0].node.position = NodeGraphNodePosition {
            x: f32::INFINITY,
            y: f32::NEG_INFINITY,
        };
        let (rect, _, _) = get_rect(&g, connection(N1, 0, N3, 0)).expect("exists");
        assert!(rect.size.width.is_nan());
    }

    #[test]
    fn get_rect_is_a_pure_query() {
        let g = graph();
        let before = wiring(&g);
        let _ = get_rect(&g, connection(N1, usize::MAX, N3, 0));
        assert_eq!(wiring(&g), before);
    }

    // ==================================================================
    // 9. render_node
    // ==================================================================

    fn node_dataset(g: &NodeGraph, id: NodeGraphNodeId) -> NodeLocalDataset {
        NodeLocalDataset {
            node_id: id,
            backref: graph_dataset(g),
        }
    }

    fn render_one(g: &NodeGraph, id: NodeGraphNodeId, offset: (f32, f32), scale: f32) -> Dom {
        let n = g
            .nodes
            .iter()
            .find(|n| n.node_id == id)
            .expect("node in fixture");
        let ty = g
            .node_types
            .iter()
            .find(|t| t.node_type_id == n.node.node_type)
            .expect("type in fixture");
        render_node(
            &n.node,
            offset,
            &ty.node_type_info,
            node_dataset(g, id),
            scale,
            node_marker(&"azul-nodegraph-test".into(), id),
        )
    }

    #[test]
    fn render_node_produces_a_single_wrapper_child_carrying_the_dataset() {
        let g = graph();
        let dom = render_one(&g, N1, (0.0, 0.0), 1.0);
        assert_eq!(dom.children.len(), 1);
        let inner = &dom.children.as_slice()[0];
        let mut ds = inner
            .root
            .get_dataset()
            .cloned()
            .expect("the node body must carry its NodeLocalDataset");
        assert!(ds.downcast_ref::<NodeLocalDataset>().is_some());
    }

    #[test]
    fn render_node_survives_every_pathological_scale_factor() {
        // `scale_factor == 1.0` picks a shorter transform list; every other value takes
        // the scale branch, where the f32 is pushed through `PercentageValue::new`.
        let g = graph();
        let baseline = count_nodes(&render_one(&g, N1, (0.0, 0.0), 1.0));
        for scale in [
            0.0,
            -0.0,
            -1.0,
            1e-30,
            f32::MAX,
            f32::MIN,
            f32::EPSILON,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            let dom = render_one(&g, N1, (0.0, 0.0), scale);
            assert_eq!(
                count_nodes(&dom),
                baseline,
                "scale {scale} changed the node structure",
            );
        }
    }

    #[test]
    fn render_node_survives_every_pathological_graph_offset() {
        let g = graph();
        let baseline = count_nodes(&render_one(&g, N1, (0.0, 0.0), 1.0));
        for offset in [
            (f32::NAN, f32::NAN),
            (f32::INFINITY, f32::NEG_INFINITY),
            (f32::MAX, f32::MIN),
            (-1e30, 1e30),
        ] {
            assert_eq!(count_nodes(&render_one(&g, N1, offset, 1.0)), baseline);
        }
    }

    #[test]
    fn render_node_survives_a_node_positioned_at_nan_and_infinity() {
        let mut g = graph();
        g.nodes.as_mut()[0].node.position = NodeGraphNodePosition {
            x: f32::NAN,
            y: f32::INFINITY,
        };
        let dom = render_one(&g, N1, (0.0, 0.0), 1.0);
        assert!(count_nodes(&dom) > 1);
    }

    #[test]
    fn render_node_drops_all_ports_when_the_backref_is_not_a_node_graph_dataset() {
        // Both port lists are built by downcasting through `backref`; a foreign payload
        // must degrade to "no ports" rather than panicking.
        let g = graph();
        let n = g.nodes.iter().find(|n| n.node_id == N1).expect("fixture");
        let ty = g
            .node_types
            .iter()
            .find(|t| t.node_type_id == TYPE_A)
            .expect("fixture");

        let broken = render_node(
            &n.node,
            (0.0, 0.0),
            &ty.node_type_info,
            NodeLocalDataset {
                node_id: N1,
                backref: RefAny::new(0xDEAD_BEEF_u32),
            },
            1.0,
            node_marker(&"azul-nodegraph-test".into(), N1),
        );
        let intact = render_one(&g, N1, (0.0, 0.0), 1.0);

        assert!(count_nodes(&broken) > 1, "the node body still renders");
        assert!(
            count_nodes(&broken) < count_nodes(&intact),
            "a broken backref must cost the ports: {} vs {}",
            count_nodes(&broken),
            count_nodes(&intact),
        );
    }

    #[test]
    fn render_node_drops_ports_whose_io_type_has_no_registered_info() {
        let mut g = graph();
        // Point TYPE_A's single input at an I/O id that has no `InputOutputInfo`.
        g.node_types.as_mut()[0].node_type_info.inputs = vec![IO_COLORLESS].into();
        let stripped = count_nodes(&render_one(&g, N1, (0.0, 0.0), 1.0));
        let intact = count_nodes(&render_one(&graph(), N1, (0.0, 0.0), 1.0));
        assert!(stripped < intact, "{stripped} vs {intact}");
    }

    #[test]
    fn render_node_renders_all_five_field_widget_kinds() {
        let mut g = graph();
        g.nodes.as_mut()[0].node.fields = vec![
            NodeTypeField {
                key: AzString::from_const_str("text"),
                value: NodeTypeFieldValue::TextInput(AzString::from_const_str("hello")),
            },
            NodeTypeField {
                key: AzString::from_const_str("number"),
                value: NodeTypeFieldValue::NumberInput(1.5),
            },
            NodeTypeField {
                key: AzString::from_const_str("check"),
                value: NodeTypeFieldValue::CheckBox(true),
            },
            NodeTypeField {
                key: AzString::from_const_str("color"),
                value: NodeTypeFieldValue::ColorInput(ColorU {
                    r: 9,
                    g: 8,
                    b: 7,
                    a: 6,
                }),
            },
            NodeTypeField {
                key: AzString::from_const_str("file"),
                value: NodeTypeFieldValue::FileInput(OptionString::None),
            },
        ]
        .into();

        let with_fields = count_nodes(&render_one(&g, N1, (0.0, 0.0), 1.0));
        let without = count_nodes(&render_one(&graph(), N1, (0.0, 0.0), 1.0));
        assert!(with_fields > without, "{with_fields} vs {without}");
    }

    #[test]
    fn render_node_field_count_is_monotonic() {
        let base = count_nodes(&render_one(&graph(), N1, (0.0, 0.0), 1.0));
        let mut previous = base;
        for count in 1..=4_usize {
            let mut g = graph();
            g.nodes.as_mut()[0].node.fields = (0..count)
                .map(|_| NodeTypeField {
                    key: AzString::from_const_str("f"),
                    value: NodeTypeFieldValue::CheckBox(false),
                })
                .collect::<Vec<_>>()
                .into();
            let now = count_nodes(&render_one(&g, N1, (0.0, 0.0), 1.0));
            assert!(now > previous, "{count} fields: {now} !> {previous}");
            previous = now;
        }
    }

    #[test]
    fn render_node_accepts_pathological_field_values() {
        // Empty / emoji / RTL / zero-width labels, NaN and infinite numbers, a fully
        // transparent color and a unicode file path.
        let mut g = graph();
        g.nodes.as_mut()[0].node.fields = vec![
            NodeTypeField {
                key: AzString::from_const_str(""),
                value: NodeTypeFieldValue::TextInput(AzString::from_const_str("")),
            },
            NodeTypeField {
                key: AzString::from_const_str("🎉\u{200b}اختبار"),
                value: NodeTypeFieldValue::TextInput(AzString::from_const_str("𝕬\u{0301}\u{feff}")),
            },
            NodeTypeField {
                key: AzString::from_const_str("nan"),
                value: NodeTypeFieldValue::NumberInput(f32::NAN),
            },
            NodeTypeField {
                key: AzString::from_const_str("inf"),
                value: NodeTypeFieldValue::NumberInput(f32::NEG_INFINITY),
            },
            NodeTypeField {
                key: AzString::from_const_str("max"),
                value: NodeTypeFieldValue::NumberInput(f32::MAX),
            },
            NodeTypeField {
                key: AzString::from_const_str("clear"),
                value: NodeTypeFieldValue::ColorInput(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                }),
            },
            NodeTypeField {
                key: AzString::from_const_str("path"),
                value: NodeTypeFieldValue::FileInput(OptionString::Some(AzString::from_const_str(
                    "/tmp/日本語/🎉.txt",
                ))),
            },
        ]
        .into();

        assert!(count_nodes(&render_one(&g, N1, (f32::NAN, f32::NAN), f32::NAN)) > 1);
    }

    // ==================================================================
    // 10. render_connections
    // ==================================================================

    fn marker() -> AzString {
        "azul-nodegraph-test-connections".into()
    }

    #[test]
    fn render_connections_of_an_unwired_graph_has_no_children() {
        let dom = render_connections(&graph(), marker());
        assert_eq!(dom.children.len(), 0);
        assert!(
            dom.root.get_marker().is_some(),
            "the container must keep the MARKER that drag-handling resolves",
        );
    }

    #[test]
    fn render_connections_emits_exactly_one_child_per_wire() {
        let mut g = graph();
        g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
        assert_eq!(render_connections(&g, marker()).children.len(), 1);

        g.connect_input_output(N4, 0, N1, 0).expect("N1 -> N4");
        assert_eq!(render_connections(&g, marker()).children.len(), 2);

        g.connect_input_output(N4, 0, N3, 0).expect("N3 -> N4");
        assert_eq!(render_connections(&g, marker()).children.len(), 3);
    }

    #[test]
    fn render_connections_skips_a_wire_to_a_node_that_no_longer_exists() {
        // `get_rect` returns `None`; the renderer must drop the wire, not unwrap it.
        let g = force_out_connection(graph(), N1, 0, MISSING, 0);
        assert_eq!(render_connections(&g, marker()).children.len(), 0);
    }

    #[test]
    fn render_connections_skips_an_out_of_range_output_port() {
        for out_idx in [1_usize, 99, usize::MAX] {
            let g = force_out_connection(graph(), N1, out_idx, N3, 0);
            assert_eq!(
                render_connections(&g, marker()).children.len(),
                0,
                "output index {out_idx} must be skipped",
            );
        }
    }

    #[test]
    fn render_connections_skips_a_node_whose_type_is_not_registered() {
        let mut g = graph();
        g.nodes.push(NodeIdNodeMap {
            node_id: NodeGraphNodeId { inner: 50 },
            node: mk_node(TYPE_UNREGISTERED, 0.0, 0.0),
        });
        let g = force_out_connection(g, NodeGraphNodeId { inner: 50 }, 0, N3, 0);
        assert_eq!(render_connections(&g, marker()).children.len(), 0);
    }

    #[test]
    fn render_connections_skips_a_port_whose_io_type_has_no_color() {
        let mut g = graph();
        g.node_types.as_mut()[0].node_type_info.outputs = vec![IO_COLORLESS].into();
        let g = force_out_connection(g, N1, 0, N3, 0);
        assert_eq!(render_connections(&g, marker()).children.len(), 0);
    }

    #[test]
    fn render_connections_survives_nan_positions_and_scale() {
        let mut g = graph();
        g.scale_factor = f32::NAN;
        g.offset = LogicalPosition {
            x: f32::INFINITY,
            y: f32::NAN,
        };
        g.nodes.as_mut()[0].node.position = NodeGraphNodePosition {
            x: f32::NAN,
            y: f32::NAN,
        };
        g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
        assert_eq!(render_connections(&g, marker()).children.len(), 1);
    }

    #[test]
    fn render_connections_renders_a_self_loop() {
        let mut g = graph();
        g.connect_input_output(N1, 0, N1, 0).expect("self loop");
        assert_eq!(render_connections(&g, marker()).children.len(), 1);
    }

    // ==================================================================
    // 11. draw_connection
    // ==================================================================

    #[test]
    fn draw_connection_returns_a_fixed_100x100_null_image() {
        // The real curve rendering is stubbed out pending `RenderImageCallbackInfo`;
        // until then the size is a constant and must not depend on the payload.
        let img = draw_connection(RefAny::new(connection(N1, 0, N3, 0)), ());
        assert_eq!(img.get_size().width, 100.0);
        assert_eq!(img.get_size().height, 100.0);
    }

    #[test]
    fn draw_connection_ignores_a_payload_of_the_wrong_type() {
        for payload in [
            RefAny::new(1_u16),
            RefAny::new(0_u8),
            RefAny::new(String::new()),
        ] {
            let img = draw_connection(payload, ());
            assert_eq!(img.get_size().width, 100.0);
        }
    }

    #[test]
    fn draw_connection_does_not_consume_or_corrupt_its_payload() {
        let cld = RefAny::new(connection(N1, 2, N3, 5));
        let _ = draw_connection(cld.clone(), ());
        let _ = draw_connection(cld.clone(), ());

        let mut probe = cld.clone();
        let read_back = probe
            .downcast_ref::<ConnectionLocalDataset>()
            .expect("payload must still be downcastable after the callback ran");
        assert_eq!(read_back.out_idx, 2);
        assert_eq!(read_back.in_idx, 5);
    }

    #[test]
    fn draw_connection_returns_distinct_image_handles_per_call() {
        let cld = RefAny::new(connection(N1, 0, N3, 0));
        let a = draw_connection(cld.clone(), ());
        let b = draw_connection(cld.clone(), ());
        assert_ne!(a, b, "each call must mint a fresh ImageRef id");
    }

    // ==================================================================
    // 12. NodeGraph::dom
    // ==================================================================

    #[test]
    fn dom_of_a_default_graph_has_the_expected_skeleton() {
        let dom = NodeGraph::default().dom();

        assert!(
            dom.root.get_context_menu().is_some(),
            "the 'add node' context menu is the only way to create nodes",
        );
        assert!(dom.root.get_dataset().is_some());
        assert_eq!(
            dom.children.len(),
            1,
            "wrapper holds exactly the .nodegraph"
        );

        let nodegraph = &dom.children.as_slice()[0];
        assert_eq!(
            nodegraph.children.len(),
            2,
            "connections container + nodes container",
        );
    }

    #[test]
    fn dom_root_dataset_round_trips_back_to_a_node_graph_local_dataset() {
        let dom = graph().dom();
        let mut ds = dom.root.get_dataset().cloned().expect("root dataset");
        let inner = ds
            .downcast_ref::<NodeGraphLocalDataset>()
            .expect("root dataset must be the NodeGraphLocalDataset");
        assert_eq!(inner.node_graph.nodes.len(), 4);
        assert!(inner.last_input_or_output_clicked.is_none());
        assert!(inner.active_node_being_dragged.is_none());
    }

    #[test]
    fn dom_renders_one_child_per_node_and_silently_drops_unregistered_types() {
        let mut g = graph();
        g.nodes.push(NodeIdNodeMap {
            node_id: NodeGraphNodeId { inner: 50 },
            node: mk_node(TYPE_UNREGISTERED, 0.0, 0.0),
        });

        let dom = g.dom();
        let nodes_container = &dom.children.as_slice()[0].children.as_slice()[1];
        assert_eq!(
            nodes_container.children.len(),
            4,
            "the 5th node has no registered type and must be filtered out",
        );
    }

    #[test]
    fn dom_context_menu_lists_one_submenu_entry_per_node_type() {
        let g = graph();
        let dom = g.dom();
        let menu = dom.root.get_context_menu().expect("context menu").clone();
        assert_eq!(menu.items.len(), 1, "one top-level 'add node' entry");
        match &menu.items.as_slice()[0] {
            MenuItem::String(s) => assert_eq!(s.children.len(), 2, "TYPE_A and TYPE_B"),
            other => panic!("expected a string menu item, got {other:?}"),
        }
    }

    #[test]
    fn dom_context_menu_is_present_even_with_no_node_types_at_all() {
        let mut g = graph();
        g.node_types = NodeTypeIdInfoMapVec::new();
        let dom = g.dom();
        let menu = dom.root.get_context_menu().expect("context menu").clone();
        assert_eq!(menu.items.len(), 1);
        match &menu.items.as_slice()[0] {
            MenuItem::String(s) => assert_eq!(s.children.len(), 0),
            other => panic!("expected a string menu item, got {other:?}"),
        }
    }

    #[test]
    fn dom_survives_pathological_scale_offset_and_positions() {
        for (scale, ox, oy) in [
            (f32::NAN, f32::NAN, f32::NAN),
            (0.0, 0.0, 0.0),
            (-1.0, -1e30, 1e30),
            (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY),
            (f32::MAX, f32::MAX, f32::MIN),
        ] {
            let mut g = graph();
            g.scale_factor = scale;
            g.offset = LogicalPosition { x: ox, y: oy };
            g.nodes.as_mut()[0].node.position = NodeGraphNodePosition {
                x: f32::NAN,
                y: f32::INFINITY,
            };
            g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
            let dom = g.dom();
            assert_eq!(dom.children.len(), 1, "scale {scale}");
        }
    }

    #[test]
    fn dom_of_a_wired_graph_renders_the_connection_container_children() {
        let mut g = graph();
        g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
        let dom = g.dom();
        let connections = &dom.children.as_slice()[0].children.as_slice()[0];
        assert_eq!(connections.children.len(), 1);
    }

    #[test]
    fn dom_can_be_converted_into_a_styled_dom() {
        // `StyledDom::create_from_dom` re-derives the child counters; a mismatch there
        // would panic while building the compact arena.
        let mut g = graph();
        g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
        g.nodes.as_mut()[0].node.fields = vec![NodeTypeField {
            key: AzString::from_const_str("k"),
            value: NodeTypeFieldValue::NumberInput(f32::NAN),
        }]
        .into();
        let styled = StyledDom::create_from_dom(g.dom());
        assert!(styled.node_data.len() > 1);
    }

    // ==================================================================
    // 13. nodegraph_set_active_node / nodegraph_unset_active_node
    // ==================================================================

    #[test]
    fn set_active_node_records_the_node_and_unset_clears_it() {
        let g = graph();
        let gd = graph_dataset(&g);
        let nd = RefAny::new(NodeLocalDataset {
            node_id: N2,
            backref: gd.clone(),
        });

        assert_eq!(
            fire(|info| nodegraph_set_active_node(nd.clone(), info)),
            Update::DoNothing,
        );
        {
            let mut probe = gd.clone();
            let d = probe.downcast_ref::<NodeGraphLocalDataset>().expect("gd");
            assert_eq!(
                d.active_node_being_dragged,
                Some(N2),
            );
        }

        assert_eq!(
            fire(|info| nodegraph_unset_active_node(gd.clone(), info)),
            Update::DoNothing,
        );
        {
            let mut probe = gd.clone();
            let d = probe.downcast_ref::<NodeGraphLocalDataset>().expect("gd");
            assert!(d.active_node_being_dragged.is_none());
        }
    }

    #[test]
    fn set_active_node_ignores_a_payload_of_the_wrong_type() {
        assert_eq!(
            fire(|info| nodegraph_set_active_node(RefAny::new(1_u64), info)),
            Update::DoNothing,
        );
    }

    #[test]
    fn set_active_node_ignores_a_node_dataset_with_a_broken_backref() {
        // The outer downcast succeeds, the inner one does not — no state may change and
        // nothing may panic.
        let nd = RefAny::new(NodeLocalDataset {
            node_id: N2,
            backref: RefAny::new(0_u8),
        });
        assert_eq!(
            fire(|info| nodegraph_set_active_node(nd.clone(), info)),
            Update::DoNothing,
        );
    }

    #[test]
    fn unset_active_node_is_idempotent_and_ignores_wrong_payloads() {
        let g = graph();
        let gd = graph_dataset(&g);
        for _ in 0..3 {
            assert_eq!(
                fire(|info| nodegraph_unset_active_node(gd.clone(), info)),
                Update::DoNothing,
            );
        }
        assert_eq!(
            fire(|info| nodegraph_unset_active_node(RefAny::new(0_i8), info)),
            Update::DoNothing,
        );
    }

    // ==================================================================
    // 14. nodegraph_duplicate_node / nodegraph_delete_node
    // ==================================================================

    #[test]
    fn duplicate_node_is_a_documented_no_op_for_valid_and_invalid_payloads() {
        let g = graph();
        let nd = RefAny::new(NodeLocalDataset {
            node_id: N1,
            backref: graph_dataset(&g),
        });
        assert_eq!(
            fire(|info| nodegraph_duplicate_node(nd.clone(), info)),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_duplicate_node(RefAny::new(0_u16), info)),
            Update::DoNothing,
        );
    }

    #[test]
    fn delete_node_forwards_the_node_id_to_on_node_removed() {
        let (g, log) = graph_with_log();
        let nd = RefAny::new(NodeLocalDataset {
            node_id: N3,
            backref: graph_dataset(&g),
        });

        assert_eq!(
            fire(|info| nodegraph_delete_node(nd.clone(), info)),
            Update::RefreshDom,
            "the user callback's Update must be propagated verbatim",
        );
        log_of(&log, |l| assert_eq!(l.removed, vec![N3.inner]));
    }

    #[test]
    fn delete_node_without_a_user_callback_does_nothing() {
        let g = graph();
        let nd = RefAny::new(NodeLocalDataset {
            node_id: N3,
            backref: graph_dataset(&g),
        });
        assert_eq!(
            fire(|info| nodegraph_delete_node(nd.clone(), info)),
            Update::DoNothing,
        );
    }

    #[test]
    fn delete_node_reports_a_node_id_that_is_not_even_in_the_graph() {
        // Documented behaviour: the handler does not validate the id, it just forwards
        // it — removal is entirely the user callback's job.
        let (g, log) = graph_with_log();
        let nd = RefAny::new(NodeLocalDataset {
            node_id: MISSING,
            backref: graph_dataset(&g),
        });
        let _ = fire(|info| nodegraph_delete_node(nd.clone(), info));
        log_of(&log, |l| assert_eq!(l.removed, vec![MISSING.inner]));
    }

    #[test]
    fn delete_node_ignores_broken_payloads() {
        assert_eq!(
            fire(|info| nodegraph_delete_node(RefAny::new(0_u32), info)),
            Update::DoNothing,
        );
        let nd = RefAny::new(NodeLocalDataset {
            node_id: N1,
            backref: RefAny::new(0_u8),
        });
        assert_eq!(
            fire(|info| nodegraph_delete_node(nd.clone(), info)),
            Update::DoNothing,
        );
    }

    // ==================================================================
    // 15. nodegraph_drag_graph_or_nodes
    // ==================================================================

    #[test]
    fn drag_without_a_previous_window_state_does_nothing() {
        // The harness leaves `previous_window_state` as `None`, which is exactly the
        // very-first-event case: no delta can be computed, so nothing may move.
        let (g, _log) = graph_with_log();
        let gd = graph_dataset(&g);
        let before = wiring(&dataset_graph(&gd));

        assert_eq!(
            fire(|info| nodegraph_drag_graph_or_nodes(gd.clone(), info)),
            Update::DoNothing,
        );
        assert_eq!(wiring(&dataset_graph(&gd)), before);
        let after = dataset_graph(&gd);
        assert_eq!(after.offset.x, 0.0);
        assert_eq!(after.offset.y, 0.0);
    }

    #[test]
    fn drag_ignores_a_payload_of_the_wrong_type() {
        assert_eq!(
            fire(|info| nodegraph_drag_graph_or_nodes(RefAny::new(0_u64), info)),
            Update::DoNothing,
        );
    }

    #[test]
    fn drag_does_not_dereference_the_active_node_before_checking_the_mouse() {
        // An "active node" that is not in the graph would be an unwrap hazard if the
        // mouse-state guard were ever reordered after the lookup.
        let g = graph();
        let gd = graph_dataset(&g);
        {
            let mut probe = gd.clone();
            let mut d = probe.downcast_mut::<NodeGraphLocalDataset>().expect("gd");
            d.active_node_being_dragged = Some(MISSING);
        }
        assert_eq!(
            fire(|info| nodegraph_drag_graph_or_nodes(gd.clone(), info)),
            Update::DoNothing,
        );
    }

    // ==================================================================
    // 16. nodegraph_input_output_connect / _disconnect
    // ==================================================================

    fn io_dataset(gd: &RefAny, node_id: NodeGraphNodeId, io: InputOrOutput) -> RefAny {
        RefAny::new(NodeInputOutputLocalDataset {
            io_id: io,
            backref: RefAny::new(NodeLocalDataset {
                node_id,
                backref: gd.clone(),
            }),
        })
    }

    #[test]
    fn connect_click_one_only_arms_the_pending_port() {
        let g = graph();
        let gd = graph_dataset(&g);
        let first = io_dataset(&gd, N1, InputOrOutput::Output(0));

        assert_eq!(
            fire(|info| nodegraph_input_output_connect(first.clone(), info)),
            Update::DoNothing,
        );
        assert_eq!(pending_click(&gd), Some((N1.inner, (false, 0))));
        assert_eq!(
            wiring(&dataset_graph(&gd)),
            wiring(&graph()),
            "arming a port must not wire anything yet",
        );
    }

    #[test]
    fn connect_output_then_input_wires_the_graph_inside_the_dataset() {
        let (g, log) = graph_with_log();
        let gd = graph_dataset(&g);
        let out = io_dataset(&gd, N1, InputOrOutput::Output(0));
        let inn = io_dataset(&gd, N3, InputOrOutput::Input(0));

        let _ = fire(|info| nodegraph_input_output_connect(out.clone(), info));
        assert_eq!(
            fire(|info| nodegraph_input_output_connect(inn.clone(), info)),
            Update::RefreshDom,
        );

        let wired = dataset_graph(&gd);
        assert_eq!(inputs_of(&wired, N3), vec![(0, vec![(N1.inner, 0)])]);
        assert_eq!(outputs_of(&wired, N1), vec![(0, vec![(N3.inner, 0)])]);
        log_of(&log, |l| {
            assert_eq!(l.connected, vec![(N3.inner, 0, N1.inner, 0)]);
        });
        assert_eq!(
            pending_click(&gd),
            None,
            "a completed connection must disarm the pending port",
        );
    }

    #[test]
    fn connect_input_then_output_wires_the_same_edge_in_the_same_direction() {
        // Clicking input-first and output-first must produce identical graphs — the
        // handler swaps the roles itself.
        let (g, _log) = graph_with_log();

        let gd_a = graph_dataset(&g);
        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd_a, N1, InputOrOutput::Output(0)), info)
        });
        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd_a, N3, InputOrOutput::Input(0)), info)
        });

        let gd_b = graph_dataset(&g);
        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd_b, N3, InputOrOutput::Input(0)), info)
        });
        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd_b, N1, InputOrOutput::Output(0)), info)
        });

        assert_eq!(wiring(&dataset_graph(&gd_a)), wiring(&dataset_graph(&gd_b)));
    }

    #[test]
    fn connect_output_to_output_disarms_instead_of_wiring() {
        let (g, log) = graph_with_log();
        let gd = graph_dataset(&g);

        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd, N1, InputOrOutput::Output(0)), info)
        });
        assert_eq!(
            fire(|info| {
                nodegraph_input_output_connect(io_dataset(&gd, N3, InputOrOutput::Output(0)), info)
            }),
            Update::DoNothing,
        );

        assert_eq!(pending_click(&gd), None);
        assert_eq!(wiring(&dataset_graph(&gd)), wiring(&graph()));
        log_of(&log, |l| assert!(l.connected.is_empty()));
    }

    #[test]
    fn connect_input_to_input_disarms_instead_of_wiring() {
        let (g, _log) = graph_with_log();
        let gd = graph_dataset(&g);

        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd, N1, InputOrOutput::Input(0)), info)
        });
        assert_eq!(
            fire(|info| {
                nodegraph_input_output_connect(io_dataset(&gd, N3, InputOrOutput::Input(0)), info)
            }),
            Update::DoNothing,
        );
        assert_eq!(pending_click(&gd), None);
        assert_eq!(wiring(&dataset_graph(&gd)), wiring(&graph()));
    }

    #[test]
    fn connect_across_incompatible_types_disarms_and_leaves_the_graph_alone() {
        let (g, log) = graph_with_log();
        let gd = graph_dataset(&g);

        // N1 emits `int`, N2 consumes `float`.
        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd, N1, InputOrOutput::Output(0)), info)
        });
        assert_eq!(
            fire(|info| {
                nodegraph_input_output_connect(io_dataset(&gd, N2, InputOrOutput::Input(0)), info)
            }),
            Update::DoNothing,
        );

        assert_eq!(pending_click(&gd), None);
        assert_eq!(wiring(&dataset_graph(&gd)), wiring(&graph()));
        log_of(&log, |l| assert!(l.connected.is_empty()));
    }

    #[test]
    fn connect_with_an_out_of_range_port_index_is_rejected() {
        let (g, _log) = graph_with_log();
        let gd = graph_dataset(&g);

        let _ = fire(|info| {
            nodegraph_input_output_connect(
                io_dataset(&gd, N1, InputOrOutput::Output(usize::MAX)),
                info,
            )
        });
        assert_eq!(
            fire(|info| {
                nodegraph_input_output_connect(io_dataset(&gd, N3, InputOrOutput::Input(0)), info)
            }),
            Update::DoNothing,
        );
        assert_eq!(wiring(&dataset_graph(&gd)), wiring(&graph()));
    }

    #[test]
    fn connect_leaves_the_pending_port_armed_when_no_user_callback_is_installed() {
        // BUG (characterised): `last_input_or_output_clicked` is only cleared inside the
        // `Some(OnNodeConnected)` arm. A graph with no `on_node_connected` callback
        // therefore keeps the *first* click armed after a successful wire, so the next
        // port click re-uses the stale port and wires the wrong edge.
        let g = graph(); // no callbacks
        let gd = graph_dataset(&g);

        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd, N1, InputOrOutput::Output(0)), info)
        });
        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd, N3, InputOrOutput::Input(0)), info)
        });

        let wired = dataset_graph(&gd);
        assert_eq!(inputs_of(&wired, N3), vec![(0, vec![(N1.inner, 0)])]);
        assert_eq!(
            pending_click(&gd),
            Some((N1.inner, (false, 0))),
            "N1's output stays armed after the wire was already made",
        );

        // ...and the very next input click silently wires a second edge from it.
        let _ = fire(|info| {
            nodegraph_input_output_connect(io_dataset(&gd, N4, InputOrOutput::Input(0)), info)
        });
        assert_eq!(
            inputs_of(&dataset_graph(&gd), N4),
            vec![(0, vec![(N1.inner, 0)])],
            "the stale port produced an edge the user never armed",
        );
    }

    #[test]
    fn connect_ignores_broken_payloads_at_every_level_of_the_backref_chain() {
        assert_eq!(
            fire(|info| nodegraph_input_output_connect(RefAny::new(0_u32), info)),
            Update::DoNothing,
        );
        let bad_mid = RefAny::new(NodeInputOutputLocalDataset {
            io_id: InputOrOutput::Input(0),
            backref: RefAny::new(0_u8),
        });
        assert_eq!(
            fire(|info| nodegraph_input_output_connect(bad_mid.clone(), info)),
            Update::DoNothing,
        );
        let bad_tail = RefAny::new(NodeInputOutputLocalDataset {
            io_id: InputOrOutput::Input(0),
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: RefAny::new(0_u16),
            }),
        });
        assert_eq!(
            fire(|info| nodegraph_input_output_connect(bad_tail.clone(), info)),
            Update::DoNothing,
        );
    }

    #[test]
    fn disconnect_notifies_the_input_or_the_output_callback_but_never_both() {
        let (g, log) = graph_with_log();
        let gd = graph_dataset(&g);

        assert_eq!(
            fire(|info| {
                nodegraph_input_output_disconnect(
                    io_dataset(&gd, N3, InputOrOutput::Input(4)),
                    info,
                )
            }),
            Update::RefreshDom,
        );
        log_of(&log, |l| {
            assert_eq!(l.input_disconnected, vec![(N3.inner, 4)]);
            assert!(l.output_disconnected.is_empty());
        });

        assert_eq!(
            fire(|info| {
                nodegraph_input_output_disconnect(
                    io_dataset(&gd, N1, InputOrOutput::Output(7)),
                    info,
                )
            }),
            Update::RefreshDomAllWindows,
        );
        log_of(&log, |l| {
            assert_eq!(l.input_disconnected, vec![(N3.inner, 4)]);
            assert_eq!(l.output_disconnected, vec![(N1.inner, 7)]);
        });
    }

    #[test]
    fn disconnect_notifies_but_does_not_actually_unwire_the_graph() {
        // BUG (characterised): the handler calls neither `disconnect_input` nor
        // `disconnect_output`, so the middle-click gesture fires the user callback while
        // the widget's own copy of the graph keeps the edge. Unless the user callback
        // rebuilds the graph, the connection stays on screen.
        let (mut g, _log) = graph_with_log();
        g.connect_input_output(N3, 0, N1, 0).expect("N1 -> N3");
        let gd = graph_dataset(&g);
        let before = wiring(&dataset_graph(&gd));

        let _ = fire(|info| {
            nodegraph_input_output_disconnect(io_dataset(&gd, N3, InputOrOutput::Input(0)), info)
        });

        assert_eq!(
            wiring(&dataset_graph(&gd)),
            before,
            "the model is untouched by the disconnect gesture",
        );
    }

    #[test]
    fn disconnect_without_user_callbacks_does_nothing() {
        let g = graph();
        let gd = graph_dataset(&g);
        assert_eq!(
            fire(|info| {
                nodegraph_input_output_disconnect(
                    io_dataset(&gd, N1, InputOrOutput::Input(0)),
                    info,
                )
            }),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_input_output_disconnect(RefAny::new(0_u32), info)),
            Update::DoNothing,
        );
    }

    #[test]
    fn disconnect_forwards_usize_max_port_indices_unclamped() {
        let (g, log) = graph_with_log();
        let gd = graph_dataset(&g);
        let _ = fire(|info| {
            nodegraph_input_output_disconnect(
                io_dataset(&gd, N1, InputOrOutput::Output(usize::MAX)),
                info,
            )
        });
        log_of(&log, |l| {
            assert_eq!(l.output_disconnected, vec![(N1.inner, usize::MAX)]);
        });
    }

    // ==================================================================
    // 17. nodegraph_context_menu_click
    // ==================================================================

    #[test]
    fn context_menu_click_does_nothing_when_the_graph_is_not_in_the_dom() {
        // `get_node_id_by_marker` finds nothing in an empty window, so the handler
        // must bail out before touching the (still valid) backref.
        let (g, log) = graph_with_log();
        let cm = RefAny::new(ContextMenuEntryLocalDataset {
            node_type: TYPE_A,
            backref: graph_dataset(&g),
        });
        assert_eq!(
            fire(|info| nodegraph_context_menu_click(cm.clone(), info)),
            Update::DoNothing,
        );
        log_of(&log, |l| assert!(l.added.is_empty()));
    }

    #[test]
    fn context_menu_click_reports_a_fresh_node_id_at_the_cursor() {
        let (g, log) = graph_with_log();
        let dom = g.dom();
        let gd = dom.root.get_dataset().cloned().expect("root dataset");
        let styled = StyledDom::create_from_dom(dom);

        let cm = RefAny::new(ContextMenuEntryLocalDataset {
            node_type: TYPE_B,
            backref: gd,
        });

        let (update, _) = with_info(styled, hit_none(), |info| {
            nodegraph_context_menu_click(cm.clone(), *info)
        });

        assert_eq!(update, Update::RefreshDomAllWindows);
        log_of(&log, |l| {
            // Cursor is `Uninitialized` and there is no layout, so both the cursor and
            // the wrapper offset are (0, 0) — the position must be exactly zero, not NaN.
            assert_eq!(l.added, vec![(TYPE_B.inner, 5, 0.0, 0.0)]);
        });
    }

    #[test]
    fn context_menu_click_position_degrades_to_nan_at_a_zero_scale_factor() {
        // `1.0 / scale_factor` is `inf` at zero; `0 * inf` is NaN. This pins down what
        // the widget actually hands the user callback in that case.
        let (mut g, log) = graph_with_log();
        g.scale_factor = 0.0;
        let dom = g.dom();
        let gd = dom.root.get_dataset().cloned().expect("root dataset");
        let styled = StyledDom::create_from_dom(dom);

        let cm = RefAny::new(ContextMenuEntryLocalDataset {
            node_type: TYPE_A,
            backref: gd,
        });
        let (_, _) = with_info(styled, hit_none(), |info| {
            nodegraph_context_menu_click(cm.clone(), *info)
        });

        log_of(&log, |l| {
            assert_eq!(l.added.len(), 1);
            let (_, id, x, y) = l.added[0];
            assert_eq!(id, 5, "the id must still be generated normally");
            assert!(x.is_nan() && y.is_nan(), "0 * inf == NaN, got ({x}, {y})");
        });
    }

    #[test]
    fn context_menu_click_ignores_a_payload_of_the_wrong_type() {
        assert_eq!(
            fire(|info| nodegraph_context_menu_click(RefAny::new(0_u32), info)),
            Update::DoNothing,
        );
    }

    // ==================================================================
    // 18. field-edit callbacks
    // ==================================================================

    #[test]
    fn textinput_focus_lost_forwards_the_decoded_text() {
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 2,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N2,
                backref: graph_dataset(&g),
            }),
        });

        // 'H', a lone surrogate (never a valid char), then U+1F389 — `get_text` drops
        // the surrogate, so the callback must see exactly "H🎉".
        let state = TextInputState {
            text: vec![0x48_u32, 0xD800, 0x1F389].into(),
            ..TextInputState::default()
        };

        assert_eq!(
            fire(|info| nodegraph_on_textinput_focus_lost(fd.clone(), info, state.clone())),
            Update::RefreshDom,
        );
        log_of(&log, |l| {
            assert_eq!(l.edited, vec![(N2.inner, 2, TYPE_B.inner)]);
            assert_eq!(l.text_values, vec!["H\u{1F389}".to_string()]);
        });
    }

    #[test]
    fn textinput_focus_lost_handles_an_empty_and_an_out_of_range_scalar() {
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 0,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: graph_dataset(&g),
            }),
        });

        for (raw, expected) in [
            (vec![], ""),
            (vec![0x11_0000_u32, 0xFFFF_FFFF], ""), // both above the Unicode range
            (vec![0x0041, 0x0301], "A\u{0301}"),    // combining mark survives
        ] {
            let state = TextInputState {
                text: raw.into(),
                ..TextInputState::default()
            };
            let _ = fire(|info| nodegraph_on_textinput_focus_lost(fd.clone(), info, state.clone()));
            log_of(&log, |l| {
                assert_eq!(l.text_values.last().map(String::as_str), Some(expected));
            });
        }
    }

    #[test]
    fn numberinput_focus_lost_forwards_nan_and_infinities_verbatim() {
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 1,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: graph_dataset(&g),
            }),
        });

        for n in [
            0.0_f32,
            -0.0,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            let state = NumberInputState {
                number: n,
                ..NumberInputState::default()
            };
            assert_eq!(
                fire(|info| nodegraph_on_numberinput_focus_lost(fd.clone(), info, state)),
                Update::RefreshDom,
            );
        }

        log_of(&log, |l| {
            assert_eq!(l.number_values.len(), 9);
            assert_eq!(l.number_values[0], 0.0);
            assert_eq!(l.number_values[2], f32::MAX);
            assert!(l.number_values[6].is_infinite());
            assert!(l.number_values[8].is_nan(), "NaN must not be normalised");
            assert!(l.edited.iter().all(|e| *e == (N1.inner, 1, TYPE_A.inner)));
        });
    }

    #[test]
    fn checkbox_change_forwards_both_states() {
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 0,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: graph_dataset(&g),
            }),
        });
        for checked in [true, false, true] {
            let _ = fire(|info| {
                nodegraph_on_checkbox_value_changed(fd.clone(), info, CheckBoxState { checked })
            });
        }
        log_of(&log, |l| assert_eq!(l.bool_values, vec![true, false, true]));
    }

    #[test]
    fn colorinput_change_forwards_every_channel_including_alpha_extremes() {
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 3,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: graph_dataset(&g),
            }),
        });
        // {1,2,3,4} catches a channel swap that greys would hide; 0 and 255 alpha are
        // the two extremes.
        for c in [
            ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        ] {
            let _ = fire(|info| {
                nodegraph_on_colorinput_value_changed(
                    fd.clone(),
                    info,
                    ColorInputState { color: c },
                )
            });
        }
        log_of(&log, |l| {
            assert_eq!(
                l.color_values,
                vec![(1, 2, 3, 4), (0, 0, 0, 0), (255, 255, 255, 255)],
            );
        });
    }

    #[test]
    fn fileinput_click_forwards_both_a_missing_and_a_unicode_path() {
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 4,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: graph_dataset(&g),
            }),
        });

        let _ = fire(|info| {
            nodegraph_on_fileinput_button_clicked(
                fd.clone(),
                info,
                FileInputState {
                    path: OptionString::None,
                },
            )
        });
        let _ = fire(|info| {
            nodegraph_on_fileinput_button_clicked(
                fd.clone(),
                info,
                FileInputState {
                    path: OptionString::Some(AzString::from_const_str("/tmp/日本語/🎉.txt")),
                },
            )
        });

        log_of(&log, |l| {
            assert_eq!(
                l.file_values,
                vec![None, Some("/tmp/日本語/🎉.txt".to_string())],
            );
        });
    }

    #[test]
    fn field_callbacks_bail_out_when_the_node_is_not_in_the_graph() {
        // Every one of the five handlers looks the node type up first; a stale
        // `node_id` must return `DoNothing` instead of unwrapping.
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 0,
            backref: RefAny::new(NodeLocalDataset {
                node_id: MISSING,
                backref: graph_dataset(&g),
            }),
        });

        assert_eq!(
            fire(|info| nodegraph_on_textinput_focus_lost(
                fd.clone(),
                info,
                TextInputState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_numberinput_focus_lost(
                fd.clone(),
                info,
                NumberInputState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_checkbox_value_changed(
                fd.clone(),
                info,
                CheckBoxState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_colorinput_value_changed(
                fd.clone(),
                info,
                ColorInputState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_fileinput_button_clicked(
                fd.clone(),
                info,
                FileInputState::default()
            )),
            Update::DoNothing,
        );

        log_of(&log, |l| assert!(l.edited.is_empty()));
    }

    #[test]
    fn field_callbacks_bail_out_on_a_payload_of_the_wrong_type() {
        assert_eq!(
            fire(|info| nodegraph_on_textinput_focus_lost(
                RefAny::new(0_u32),
                info,
                TextInputState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_numberinput_focus_lost(
                RefAny::new(0_u32),
                info,
                NumberInputState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_checkbox_value_changed(
                RefAny::new(0_u32),
                info,
                CheckBoxState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_colorinput_value_changed(
                RefAny::new(0_u32),
                info,
                ColorInputState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_fileinput_button_clicked(
                RefAny::new(0_u32),
                info,
                FileInputState::default()
            )),
            Update::DoNothing,
        );
    }

    #[test]
    fn field_callbacks_forward_a_usize_max_field_index_unclamped() {
        // The field index is an opaque token as far as the widget is concerned — it is
        // never used to index anything here, so even `usize::MAX` must pass through.
        let (g, log) = graph_with_log();
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: usize::MAX,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: graph_dataset(&g),
            }),
        });
        let _ = fire(|info| {
            nodegraph_on_checkbox_value_changed(fd.clone(), info, CheckBoxState { checked: true })
        });
        log_of(&log, |l| {
            assert_eq!(l.edited, vec![(N1.inner, usize::MAX, TYPE_A.inner)]);
        });
    }

    #[test]
    fn field_callbacks_without_a_user_callback_do_nothing() {
        let g = graph(); // no callbacks installed
        let fd = RefAny::new(NodeFieldLocalDataset {
            field_idx: 0,
            backref: RefAny::new(NodeLocalDataset {
                node_id: N1,
                backref: graph_dataset(&g),
            }),
        });
        assert_eq!(
            fire(|info| nodegraph_on_checkbox_value_changed(
                fd.clone(),
                info,
                CheckBoxState::default()
            )),
            Update::DoNothing,
        );
        assert_eq!(
            fire(|info| nodegraph_on_fileinput_button_clicked(
                fd.clone(),
                info,
                FileInputState::default()
            )),
            Update::DoNothing,
        );
    }
}
