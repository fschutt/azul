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
use azul_css::css::BoxOrStatic;

use crate::{
    callbacks::{Callback, CallbackInfo},
    extra::coloru_from_str,
    widgets::{
        check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState},
        color_input::{ColorInput, ColorInputOnValueChangeCallbackType, ColorInputState},
        file_input::{FileInput, FileInputOnPathChangeCallbackType, FileInputState},
        number_input::{NumberInput, NumberInputOnFocusLostCallbackType, NumberInputState},
        text_input::{TextInput, TextInputOnFocusLostCallbackType, TextInputState},
    },
};

/// Same as the NodeGraph but without generics and without the actual data
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
    /// Generates a new NodeId that is unique in the graph
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

#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeTypeIdInfoMap {
    pub node_type_id: NodeTypeId,
    pub node_type_info: NodeTypeInfo,
}

impl_option!(NodeTypeIdInfoMap, OptionNodeTypeIdInfoMap, copy = false, [Debug, Clone]);
impl_vec!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec, NodeTypeIdInfoMapVecDestructor, NodeTypeIdInfoMapVecDestructorType, NodeTypeIdInfoMapVecSlice, OptionNodeTypeIdInfoMap);
impl_vec_clone!(
    NodeTypeIdInfoMap,
    NodeTypeIdInfoMapVec,
    NodeTypeIdInfoMapVecDestructor
);
impl_vec_mut!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec);
impl_vec_debug!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputOutputTypeIdInfoMap {
    pub io_type_id: InputOutputTypeId,
    pub io_info: InputOutputInfo,
}

impl_option!(InputOutputTypeIdInfoMap, OptionInputOutputTypeIdInfoMap, copy = false, [Debug, Clone]);
impl_vec!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec, InputOutputTypeIdInfoMapVecDestructor, InputOutputTypeIdInfoMapVecDestructorType, InputOutputTypeIdInfoMapVecSlice, OptionInputOutputTypeIdInfoMap);
impl_vec_clone!(
    InputOutputTypeIdInfoMap,
    InputOutputTypeIdInfoMapVec,
    InputOutputTypeIdInfoMapVecDestructor
);
impl_vec_mut!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec);
impl_vec_debug!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeIdNodeMap {
    pub node_id: NodeGraphNodeId,
    pub node: Node,
}

impl_option!(NodeIdNodeMap, OptionNodeIdNodeMap, copy = false, [Debug, Clone]);
impl_vec!(NodeIdNodeMap, NodeIdNodeMapVec, NodeIdNodeMapVecDestructor, NodeIdNodeMapVecDestructorType, NodeIdNodeMapVecSlice, OptionNodeIdNodeMap);
impl_vec_clone!(NodeIdNodeMap, NodeIdNodeMapVec, NodeIdNodeMapVecDestructor);
impl_vec_mut!(NodeIdNodeMap, NodeIdNodeMapVec);
impl_vec_debug!(NodeIdNodeMap, NodeIdNodeMapVec);

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub enum NodeGraphStyle {
    Default,
    // to be extended
}

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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct InputOutputTypeId {
    pub inner: u64,
}

impl_option!(InputOutputTypeId, OptionInputOutputTypeId, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_vec!(InputOutputTypeId, InputOutputTypeIdVec, InputOutputTypeIdVecDestructor, InputOutputTypeIdVecDestructorType, InputOutputTypeIdVecSlice, OptionInputOutputTypeId);
impl_vec_clone!(
    InputOutputTypeId,
    InputOutputTypeIdVec,
    InputOutputTypeIdVecDestructor
);
impl_vec_mut!(InputOutputTypeId, InputOutputTypeIdVec);
impl_vec_debug!(InputOutputTypeId, InputOutputTypeIdVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NodeTypeId {
    pub inner: u64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NodeGraphNodeId {
    pub inner: u64,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Node {
    pub node_type: NodeTypeId,
    pub position: NodeGraphNodePosition,
    pub fields: NodeTypeFieldVec,
    pub connect_in: InputConnectionVec,
    pub connect_out: OutputConnectionVec,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeTypeField {
    pub key: AzString,
    pub value: NodeTypeFieldValue,
}

impl_option!(NodeTypeField, OptionNodeTypeField, copy = false, [Debug, Clone]);
impl_vec!(NodeTypeField, NodeTypeFieldVec, NodeTypeFieldVecDestructor, NodeTypeFieldVecDestructorType, NodeTypeFieldVecSlice, OptionNodeTypeField);
impl_vec_clone!(NodeTypeField, NodeTypeFieldVec, NodeTypeFieldVecDestructor);
impl_vec_debug!(NodeTypeField, NodeTypeFieldVec);
impl_vec_mut!(NodeTypeField, NodeTypeFieldVec);

#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum NodeTypeFieldValue {
    TextInput(AzString),
    NumberInput(f32),
    CheckBox(bool),
    ColorInput(ColorU),
    FileInput(OptionString),
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputConnection {
    pub input_index: usize,
    pub connects_to: OutputNodeAndIndexVec,
}

impl_option!(InputConnection, OptionInputConnection, copy = false, [Debug, Clone]);
impl_vec!(InputConnection, InputConnectionVec, InputConnectionVecDestructor, InputConnectionVecDestructorType, InputConnectionVecSlice, OptionInputConnection);
impl_vec_clone!(
    InputConnection,
    InputConnectionVec,
    InputConnectionVecDestructor
);
impl_vec_debug!(InputConnection, InputConnectionVec);
impl_vec_mut!(InputConnection, InputConnectionVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct OutputNodeAndIndex {
    pub node_id: NodeGraphNodeId,
    pub output_index: usize,
}

impl_option!(OutputNodeAndIndex, OptionOutputNodeAndIndex, copy = false, [Debug, Clone]);
impl_vec!(OutputNodeAndIndex, OutputNodeAndIndexVec, OutputNodeAndIndexVecDestructor, OutputNodeAndIndexVecDestructorType, OutputNodeAndIndexVecSlice, OptionOutputNodeAndIndex);
impl_vec_clone!(
    OutputNodeAndIndex,
    OutputNodeAndIndexVec,
    OutputNodeAndIndexVecDestructor
);
impl_vec_debug!(OutputNodeAndIndex, OutputNodeAndIndexVec);
impl_vec_mut!(OutputNodeAndIndex, OutputNodeAndIndexVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct OutputConnection {
    pub output_index: usize,
    pub connects_to: InputNodeAndIndexVec,
}

impl_option!(OutputConnection, OptionOutputConnection, copy = false, [Debug, Clone]);
impl_vec!(OutputConnection, OutputConnectionVec, OutputConnectionVecDestructor, OutputConnectionVecDestructorType, OutputConnectionVecSlice, OptionOutputConnection);
impl_vec_clone!(
    OutputConnection,
    OutputConnectionVec,
    OutputConnectionVecDestructor
);
impl_vec_debug!(OutputConnection, OutputConnectionVec);
impl_vec_mut!(OutputConnection, OutputConnectionVec);

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct InputNodeAndIndex {
    pub node_id: NodeGraphNodeId,
    pub input_index: usize,
}

impl_option!(InputNodeAndIndex, OptionInputNodeAndIndex, copy = false, [Debug, Clone]);
impl_vec!(InputNodeAndIndex, InputNodeAndIndexVec, InputNodeAndIndexVecDestructor, InputNodeAndIndexVecDestructorType, InputNodeAndIndexVecSlice, OptionInputNodeAndIndex);
impl_vec_clone!(
    InputNodeAndIndex,
    InputNodeAndIndexVec,
    InputNodeAndIndexVecDestructor
);
impl_vec_debug!(InputNodeAndIndex, InputNodeAndIndexVec);
impl_vec_mut!(InputNodeAndIndex, InputNodeAndIndexVec);

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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::NodeGraphError::*;
        match self {
            NodeMimeTypeMismatch => write!(f, "MIME type mismatch"),
            NodeInvalidIndex => write!(f, "Invalid node index"),
            NodeInvalidNode => write!(f, "Invalid node"),
            NoRootNode => write!(f, "No root node found"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct GraphDragAmount {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct NodeDragAmount {
    pub x: f32,
    pub y: f32,
}

impl NodeGraph {
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        ::core::mem::swap(&mut default, self);
        default
    }

    /// Connects the current nodes input with another nodes output
    ///
    /// ## Inputs
    ///
    /// - `output_node_id`: The ID of the output node (index in the NodeGraphs internal BTree)
    /// - `output_index`: The index of the output *on the output node*
    /// - `input_node_id`: Same as output_node_id, but for the input node
    /// - `input_index`: Same as output_index, but for the input node
    ///
    /// ## Returns
    ///
    /// One of:
    ///
    /// - `NodeGraphError::NodeInvalidNode(n)`: The node at `` of the inputs does not exist
    /// - `NodeGraphError::NodeInvalidIndex(n, u)`: One node has an invalid `output` or `input`
    ///   index
    /// - `NodeGraphError::NodeMimeTypeMismatch`: The types of two connected `outputs` and `inputs`
    ///   isn't the same
    /// - `Ok`: The insertion of the new node went well.
    fn connect_input_output(
        &mut self,
        input_node_id: NodeGraphNodeId,
        input_index: usize,
        output_node_id: NodeGraphNodeId,
        output_index: usize,
    ) -> Result<(), NodeGraphError> {
        // Verify that the node type of the connection matches
        let _ =
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
                })
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
                })
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
    /// - `input_node_id`: The ID of the input node (index in the NodeGraphs internal BTree)
    /// - `input_index`: The index of the input *on the input node*
    ///
    /// # Returns
    ///
    /// - `Err(NodeGraphError::NodeInvalidNode(n))`: The node at index `input_node_id` does not
    ///   exist
    /// - `Err(NodeGraphError::NodeInvalidIndex(n, u))`: One node has an invalid `input` or `output`
    ///   index
    /// - `Err(NodeGraphError::NodeMimeTypeMismatch)`: The types of two connected `input` and
    ///   `output` does not match
    /// - `Ok(())`: The insertion of the new node went well.
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
        } in output_connections.as_ref().iter()
        {
            let output_node_id = *node_id;
            let output_index = *output_index;

            // verify that the node type of the connection matches
            let _ = self.verify_nodetype_match(
                output_node_id,
                output_index,
                input_node_id,
                input_index,
            )?;

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

    /// Disconnect an output if it is connected to an output
    ///
    /// # Inputs
    ///
    /// - `output_node_id`: The ID of the input node (index in the NodeGraphs internal BTree)
    /// - `output_index`: The index of the input *on the input node*
    ///
    /// # Returns
    ///
    /// - `Err(NodeGraphError::NodeInvalidNode(n))`: The node at index `output_node_id` does not
    ///   exist
    /// - `Err(NodeGraphError::NodeInvalidIndex(n, u))`: One node has an invalid `input` or `output`
    ///   index
    /// - `Err(NodeGraphError::NodeMimeTypeMismatch)`: The types of two connected `input` and
    ///   `output` does not match
    /// - `Ok(())`: The insertion of the new node went well.
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
        } in input_connections.iter()
        {
            let input_node_id = *node_id;
            let input_index = *input_index;

            // verify that the node type of the connection matches
            let _ = self.verify_nodetype_match(
                output_node_id,
                output_index,
                input_node_id,
                input_index,
            )?;

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

        let node_connection_marker = RefAny::new(NodeConnectionMarkerDataset {});

        let node_graph_local_dataset = RefAny::new(NodeGraphLocalDataset {
            node_graph: self.clone(), // TODO: expensive
            last_input_or_output_clicked: None,
            active_node_being_dragged: None,
            node_connection_marker: node_connection_marker.clone(),
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
                                    StringMenuItem::create(
                                        node_type_info.node_type_name.clone().into(),
                                    )
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
                                    cb: nodegraph_drag_graph_or_nodes as usize as usize,
                                    ctx: OptionRefAny::None,
                                },
                            },
                            CoreCallbackData {
                                event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                                refany: node_graph_local_dataset.clone(),
                                callback: CoreCallback {
                                    cb: nodegraph_unset_active_node as usize as usize,
                                    ctx: OptionRefAny::None,
                                },
                            },
                        ]
                        .into(),
                    )
                    .with_children({
                        vec![
                            // connections
                            render_connections(&self, node_connection_marker),
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
    }
}

// dataset set on the top-level nodegraph node,
// containing all the state of the node graph
struct NodeGraphLocalDataset {
    node_graph: NodeGraph,
    last_input_or_output_clicked: Option<(NodeGraphNodeId, InputOrOutput)>,
    // Ref<NodeLocalDataSet> - used as a marker for getting the visual node ID
    active_node_being_dragged: Option<(NodeGraphNodeId, RefAny)>,
    node_connection_marker: RefAny, // Ref<NodeConnectionMarkerDataset>
    callbacks: NodeGraphCallbacks,
}

struct ContextMenuEntryLocalDataset {
    node_type: NodeTypeId,
    backref: RefAny, // RefAny<NodeGraphLocalDataset>
}

struct NodeConnectionMarkerDataset {}

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

fn render_node(
    node: &Node,
    graph_offset: (f32, f32),
    node_info: &NodeTypeInfo,
    mut node_local_dataset: NodeLocalDataset,
    scale_factor: f32,
) -> Dom {
    use azul_core::dom::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, Dom, DomVec, IdOrClass,
        IdOrClass::Class, IdOrClassVec,
    };
    use azul_css::*;

    const STRING_9416190750059025162: AzString = AzString::from_const_str("Material Icons");
    const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
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
        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(BoxOrStatic::Static(&
            StyleBoxShadow {
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
            },
        )))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(BoxOrStatic::Static(&
            StyleBoxShadow {
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
            },
        )))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(BoxOrStatic::Static(&
            StyleBoxShadow {
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
            },
        )))),
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
        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(BoxOrStatic::Static(&
            StyleBoxShadow {
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
            },
        )))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(BoxOrStatic::Static(&
            StyleBoxShadow {
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
            },
        )))),
        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(BoxOrStatic::Static(&
            StyleBoxShadow {
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
            },
        )))),
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
            Some((
                io_info.io_info.data_type.clone(),
                io_info.io_info.color.clone(),
            ))
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
            Some((
                io_info.io_info.data_type.clone(),
                io_info.io_info.color.clone(),
            ))
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
               if scale_factor != 1.0 {
                    vec![
                         StyleTransform::Translate(node_transform),
                         StyleTransform::ScaleX(PercentageValue::new(scale_factor * 100.0)),
                         StyleTransform::ScaleY(PercentageValue::new(scale_factor * 100.0)),
                    ]
               } else {
                    vec![
                         StyleTransform::Translate(node_transform)
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
           Dom::create_text(AzString::from_const_str("X"))
               .with_css_props(CSS_MATCH_7395766480280098891)
               .with_callbacks(vec![
                   CoreCallbackData {
                       event: EventFilter::Hover(HoverEventFilter::MouseUp),
                       refany: node_local_dataset.clone(),
                       callback: CoreCallback { cb: nodegraph_delete_node as usize, ctx: OptionRefAny::None },
                   },
               ].into())
               .with_ids_and_classes({
                   const IDS_AND_CLASSES_7122017923389407516: &[IdOrClass] =
                       &[Class(AzString::from_const_str("node_close_button"))];
                   IdOrClassVec::from_const_slice(IDS_AND_CLASSES_7122017923389407516)
               }),
           Dom::create_text(node_info.node_type_name.clone())
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
                                   use self::InputOrOutput::*;

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
                                               .with_children(DomVec::from_vec(vec![Dom::create_text(
                                                   input_label.clone(),
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
                                   Dom::create_text(field.key.clone())
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
                                           TextInput::create()
                                           .with_text(initial_text.clone())
                                           .with_on_focus_lost(field_local_dataset, nodegraph_on_textinput_focus_lost as TextInputOnFocusLostCallbackType)
                                           .dom()
                                       },
                                       NodeTypeFieldValue::NumberInput(initial_value) => {
                                           NumberInput::create(*initial_value)
                                           .with_on_focus_lost(field_local_dataset, nodegraph_on_numberinput_focus_lost as NumberInputOnFocusLostCallbackType)
                                           .dom()
                                       },
                                       NodeTypeFieldValue::CheckBox(initial_checked) => {
                                           CheckBox::create(*initial_checked)
                                           .with_on_toggle(field_local_dataset, nodegraph_on_checkbox_value_changed as CheckBoxOnToggleCallbackType)
                                           .dom()
                                       },
                                       NodeTypeFieldValue::ColorInput(initial_color) => {
                                           ColorInput::create(*initial_color)
                                           .with_on_value_change(field_local_dataset, nodegraph_on_colorinput_value_changed as ColorInputOnValueChangeCallbackType)
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
                                   use self::InputOrOutput::*;
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
                                               .with_children(DomVec::from_vec(vec![Dom::create_text(
                                                   output_label.clone(),
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
    ].into())
}

fn render_connections(node_graph: &NodeGraph, root_marker_nodedata: RefAny) -> Dom {
    const THEME_RED: ColorU = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    }; // #484c52
    const BACKGROUND_THEME_RED: &[StyleBackgroundContent] =
        &[StyleBackgroundContent::Color(THEME_RED)];
    const BACKGROUND_COLOR_RED: StyleBackgroundContentVec =
        StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_RED);

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
        .with_dataset(Some(root_marker_nodedata).into())
        .with_children({
            let mut children = Vec::new();

            for NodeIdNodeMap { node_id, node } in node_graph.nodes.as_ref().iter() {
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
                } in node.connect_out.as_ref().iter()
                {
                    let output_type_id = match node_type_info.outputs.get(*output_index) {
                        Some(s) => s,
                        None => continue,
                    };

                    let output_color = match node_graph
                        .input_output_types
                        .iter()
                        .find(|o| o.io_type_id == *output_type_id)
                    {
                        Some(s) => s.io_info.color.clone(),
                        None => continue,
                    };

                    for InputNodeAndIndex {
                        node_id,
                        input_index,
                    } in connects_to.as_ref().iter()
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

                        let (rect, swap_vert, swap_horz) = match get_rect(&node_graph, cld) {
                            Some(s) => s,
                            None => continue,
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
                                .with_inline_style(
                                    "flex-grow: 1; position: absolute; overflow: hidden;",
                                )
                                .with_children(vec![connection_div].into()),
                        );
                    }
                }
            }

            children.into()
        })
}

extern "C" fn draw_connection(mut refany: RefAny, _info: ()) -> ImageRef {
    // RenderImageCallbackInfo not available in memtest
    // let size = info.get_bounds().get_physical_size();
    let size = azul_core::geom::LogicalSize {
        width: 100.0,
        height: 100.0,
    };
    let invalid = ImageRef::null_image(
        size.width as usize,
        size.height as usize,
        RawImageFormat::R8,
        Vec::new(),
    );

    // Cannot call draw_connection_inner without RenderImageCallbackInfo
    invalid
}

fn draw_connection_inner(
    mut refany: RefAny,
    _info: &mut (),
    _texture_size: PhysicalSizeU32,
) -> Option<ImageRef> {
    use crate::xml::svg::tessellate_path_stroke;

    let refany = refany.downcast_ref::<ConnectionLocalDataset>()?;
    let refany = &*refany;

    // Cannot proceed without RenderImageCallbackInfo - all code below requires it
    return None;

    /* Commented out - requires RenderImageCallbackInfo
    let gl_context = info.get_gl_context().into_option()?;

    let mut texture = Texture::allocate_rgba8(
        gl_context.clone(),
        texture_size,
        coloru_from_str("#00000000"),
    );

    texture.clear();

    let mut stroke_style = SvgStrokeStyle::default();
    stroke_style.line_width = 4.0;

    let tex_half = (texture_size.width as f32) / 2.0;

    let tessellated_stroke = tessellate_path_stroke(
        &SvgPath {
            items: vec![
                // depending on in which quadrant the curve is drawn relative to the input node,
                // we need a different curve
                if refany.swap_vert {
                    if refany.swap_horz {
                        //          /- input
                        //  output-/
                        SvgPathElement::CubicCurve(SvgCubicCurve {
                            start: SvgPoint {
                                x: 0.0,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                            ctrl_1: SvgPoint {
                                x: tex_half,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                            ctrl_2: SvgPoint {
                                x: tex_half,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                            end: SvgPoint {
                                x: texture_size.width as f32,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                        })
                    } else {
                        //  input -\
                        //          \- output
                        SvgPathElement::CubicCurve(SvgCubicCurve {
                            start: SvgPoint {
                                x: 0.0,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                            ctrl_1: SvgPoint {
                                x: tex_half,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                            ctrl_2: SvgPoint {
                                x: tex_half,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                            end: SvgPoint {
                                x: texture_size.width as f32,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                        })
                    }
                } else {
                    if refany.swap_horz {
                        //  output-\
                        //          \- input
                        SvgPathElement::CubicCurve(SvgCubicCurve {
                            start: SvgPoint {
                                x: 0.0,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                            ctrl_1: SvgPoint {
                                x: tex_half,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                            ctrl_2: SvgPoint {
                                x: tex_half,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                            end: SvgPoint {
                                x: texture_size.width as f32,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                        })
                    } else {
                        //         /- output
                        // input -/
                        SvgPathElement::CubicCurve(SvgCubicCurve {
                            start: SvgPoint {
                                x: 0.0,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                            ctrl_1: SvgPoint {
                                x: tex_half,
                                y: texture_size.height as f32 - (CONNECTION_DOT_HEIGHT / 2.0),
                            },
                            ctrl_2: SvgPoint {
                                x: tex_half,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                            end: SvgPoint {
                                x: texture_size.width as f32,
                                y: CONNECTION_DOT_HEIGHT / 2.0,
                            },
                        })
                    }
                },
            ]
            .into(),
        },
        stroke_style,
    );

    let tesselated_gpu_buffer = TessellatedGPUSvgNode::new(&tessellated_stroke, gl_context.clone());

    tesselated_gpu_buffer.draw(&mut texture, texture_size, refany.color, Vec::new().into());

    Some(ImageRef::new_gltexture(texture))
    */
}

const NODE_WIDTH: f32 = 250.0;
const V_OFFSET: f32 = 71.0;
const DIST_BETWEEN_NODES: f32 = 10.0;
const CONNECTION_DOT_HEIGHT: f32 = 15.0;

// calculates the rect on which the connection is drawn in the UI
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
    let data_clone = refany.clone();
    if let Some(mut refany) = refany.downcast_mut::<NodeLocalDataset>() {
        let node_id = refany.node_id.clone();
        if let Some(mut backref) = refany.backref.downcast_mut::<NodeGraphLocalDataset>() {
            backref.active_node_being_dragged = Some((node_id, data_clone));
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
extern "C" fn nodegraph_drag_graph_or_nodes(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let mut refany = match refany.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    let refany = &mut *refany;

    let prev = match info.get_previous_mouse_state() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    let cur = info.get_current_mouse_state();
    if !(cur.left_down && prev.left_down) {
        // event is not a drag event
        return Update::DoNothing;
    }

    let (current_mouse_pos, previous_mouse_pos) = match (cur.cursor_position, prev.cursor_position)
    {
        (InWindow(c), InWindow(p)) => (c, p),
        _ => return Update::DoNothing,
    };

    let dx = (current_mouse_pos.x - previous_mouse_pos.x) * (1.0 / refany.node_graph.scale_factor);
    let dy = (current_mouse_pos.y - previous_mouse_pos.y) * (1.0 / refany.node_graph.scale_factor);
    let nodegraph_node = info.get_hit_node();

    let should_update = match refany.active_node_being_dragged.clone() {
        // drag node
        Some((node_graph_node_id, data_marker)) => {
            let node_connection_marker = &mut refany.node_connection_marker;

            let _nodegraph_node = info.get_hit_node();
            let result = match refany.callbacks.on_node_dragged.as_ref() {
                Some(OnNodeDragged { callback, refany }) => (callback.cb)(
                    refany.clone(),
                    info.clone(),
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

            let visual_node_id = match info.get_node_id_of_root_dataset(data_marker) {
                Some(s) => s,
                None => return Update::DoNothing,
            };

            let node_transform = StyleTransformTranslate2D {
                x: PixelValue::px(node_position.x + refany.node_graph.offset.x),
                y: PixelValue::px(node_position.y + refany.node_graph.offset.y),
            };

            info.set_css_property(
                visual_node_id,
                CssProperty::transform(
                    if refany.node_graph.scale_factor != 1.0 {
                        vec![
                            StyleTransform::Translate(node_transform),
                            StyleTransform::ScaleX(PercentageValue::new(
                                refany.node_graph.scale_factor * 100.0,
                            )),
                            StyleTransform::ScaleY(PercentageValue::new(
                                refany.node_graph.scale_factor * 100.0,
                            )),
                        ]
                    } else {
                        vec![StyleTransform::Translate(node_transform)]
                    }
                    .into(),
                ),
            );

            // get the NodeId of the node containing all the connection lines
            let connection_container_nodeid =
                match info.get_node_id_of_root_dataset(node_connection_marker.clone()) {
                    Some(s) => s,
                    None => return result,
                };

            // animate all the connections
            let mut first_connection_child = info.get_first_child(connection_container_nodeid);

            while let Some(connection_nodeid) = first_connection_child {
                first_connection_child = info.get_next_sibling(connection_nodeid);

                let first_child = match info.get_first_child(connection_nodeid) {
                    Some(s) => s,
                    None => continue,
                };

                let mut dataset = match info.get_dataset(first_child) {
                    Some(s) => s,
                    None => continue,
                };

                let mut cld = match dataset.downcast_mut::<ConnectionLocalDataset>() {
                    Some(s) => s,
                    None => continue,
                };

                if !(cld.out_node_id == node_graph_node_id || cld.in_node_id == node_graph_node_id)
                {
                    continue; // connection does not need to be modified
                }

                let (new_rect, swap_vert, swap_horz) = match get_rect(&refany.node_graph, *cld) {
                    Some(s) => s,
                    None => continue,
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
                        if refany.node_graph.scale_factor != 1.0 {
                            vec![
                                StyleTransform::Translate(node_transform),
                                StyleTransform::ScaleX(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                                StyleTransform::ScaleY(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                            ]
                        } else {
                            vec![StyleTransform::Translate(node_transform)]
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
                Some(OnNodeGraphDragged { callback, refany }) => (callback.cb)(
                    refany.clone(),
                    info.clone(),
                    GraphDragAmount { x: dx, y: dy },
                ),
                None => Update::DoNothing,
            };

            refany.node_graph.offset.x += dx;
            refany.node_graph.offset.y += dy;

            // Update the visual node positions
            let node_container = match info.get_first_child(nodegraph_node) {
                Some(s) => s,
                None => return Update::DoNothing,
            };

            let node_container = match info.get_next_sibling(node_container) {
                Some(s) => s,
                None => return Update::DoNothing,
            };

            let mut node = match info.get_first_child(node_container) {
                Some(s) => s,
                None => return Update::DoNothing,
            };

            loop {
                let node_first_child = match info.get_first_child(node) {
                    Some(s) => s,
                    None => return Update::DoNothing,
                };

                let mut node_local_dataset = match info.get_dataset(node_first_child) {
                    None => return Update::DoNothing,
                    Some(s) => s,
                };

                let node_graph_node_id = match node_local_dataset.downcast_ref::<NodeLocalDataset>()
                {
                    Some(s) => s,
                    None => continue,
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
                        if refany.node_graph.scale_factor != 1.0 {
                            vec![
                                StyleTransform::Translate(node_transform),
                                StyleTransform::ScaleX(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                                StyleTransform::ScaleY(PercentageValue::new(
                                    refany.node_graph.scale_factor * 100.0,
                                )),
                            ]
                        } else {
                            vec![StyleTransform::Translate(node_transform)]
                        }
                        .into(),
                    ),
                );

                node = match info.get_next_sibling(node) {
                    Some(s) => s,
                    None => break,
                };
            }

            let node_connection_marker = &mut refany.node_connection_marker;

            // Update the connection positions
            let connection_container_nodeid =
                match info.get_node_id_of_root_dataset(node_connection_marker.clone()) {
                    Some(s) => s,
                    None => return result,
                };

            let mut first_connection_child = info.get_first_child(connection_container_nodeid);

            while let Some(connection_nodeid) = first_connection_child {
                first_connection_child = info.get_next_sibling(connection_nodeid);

                let first_child = match info.get_first_child(connection_nodeid) {
                    Some(s) => s,
                    None => continue,
                };

                let mut dataset = match info.get_dataset(first_child) {
                    Some(s) => s,
                    None => continue,
                };

                let cld = match dataset.downcast_ref::<ConnectionLocalDataset>() {
                    Some(s) => s,
                    None => continue,
                };

                let (new_rect, _, _) = match get_rect(&refany.node_graph, *cld) {
                    Some(s) => s,
                    None => continue,
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
    let _data = match refany.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    Update::DoNothing // TODO
}

extern "C" fn nodegraph_delete_node(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let mut refany = match refany.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = refany.node_id.clone();

    let mut backref = match refany.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = match backref.callbacks.on_node_removed.as_ref() {
        Some(OnNodeRemoved { callback, refany }) => (callback.cb)(refany.clone(), info, node_id),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_context_menu_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::CursorPosition;

    let mut refany = match refany.downcast_mut::<ContextMenuEntryLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let new_node_type = refany.node_type.clone();

    let node_graph_wrapper_id = match info.get_node_id_of_root_dataset(refany.backref.clone()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let mut backref = match refany.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_wrapper_offset = info
        .get_node_position(node_graph_wrapper_id)
        .map(|p| p)
        .map(|p| (p.x, p.y))
        .unwrap_or((0.0, 0.0));

    let cursor_in_viewport = match info.get_current_mouse_state().cursor_position {
        CursorPosition::InWindow(i) => i,
        CursorPosition::OutOfWindow(i) => i,
        _ => LogicalPosition::zero(),
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
    use self::InputOrOutput::*;

    let mut refany = match refany.downcast_mut::<NodeInputOutputLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let io_id = refany.io_id.clone();

    let mut backref = match refany.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = backref.node_id.clone();

    let mut backref = match backref.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let (input_node, input_index, output_node, output_index) =
        match backref.last_input_or_output_clicked.clone() {
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
        Ok(_) => {}
        Err(e) => {
            eprintln!("{:?}", e);
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
    use self::InputOrOutput::*;

    let mut refany = match refany.downcast_mut::<NodeInputOutputLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let io_id = refany.io_id.clone();

    let mut backref = match refany.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = backref.node_id.clone();

    let mut backref = match backref.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
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
    };

    result
}

extern "C" fn nodegraph_on_textinput_focus_lost(
    mut refany: RefAny,
    info: CallbackInfo,
    textinputstate: TextInputState,
) -> Update {
    let mut refany = match refany.downcast_mut::<NodeFieldLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let field_idx = refany.field_idx;

    let mut node_local_dataset = match refany.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = node_local_dataset.node_id;

    let mut node_graph = match node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    {
        Some(s) => s,
        None => return Update::DoNothing,
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
    let mut refany = match refany.downcast_mut::<NodeFieldLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let field_idx = refany.field_idx;

    let mut node_local_dataset = match refany.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = node_local_dataset.node_id;

    let mut node_graph = match node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    {
        Some(s) => s,
        None => return Update::DoNothing,
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
    let mut refany = match refany.downcast_mut::<NodeFieldLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let field_idx = refany.field_idx;

    let mut node_local_dataset = match refany.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = node_local_dataset.node_id;

    let mut node_graph = match node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    {
        Some(s) => s,
        None => return Update::DoNothing,
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
    let mut refany = match refany.downcast_mut::<NodeFieldLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let field_idx = refany.field_idx;

    let mut node_local_dataset = match refany.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = node_local_dataset.node_id;
    let mut node_graph = match node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    {
        Some(s) => s,
        None => return Update::DoNothing,
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
    let mut refany = match refany.downcast_mut::<NodeFieldLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let field_idx = refany.field_idx;

    let mut node_local_dataset = match refany.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = node_local_dataset.node_id;
    let mut node_graph = match node_local_dataset
        .backref
        .downcast_mut::<NodeGraphLocalDataset>()
    {
        Some(s) => s,
        None => return Update::DoNothing,
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
            NodeTypeFieldValue::FileInput(file.path.clone()),
        ),
        None => return Update::DoNothing,
    };

    result
}
