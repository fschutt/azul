use core::fmt;
use alloc::vec::Vec;
use azul_desktop::css::*;
use azul_desktop::css::AzString;
use azul_desktop::dom::{
    Dom, IdOrClass, TabIndex,
    HoverEventFilter, EventFilter,
    CallbackData,
    IdOrClass::{Id, Class},
    NodeDataInlineCssProperty,
    NodeDataInlineCssProperty::{Normal, Hover},
    DomVec, IdOrClassVec, NodeDataInlineCssPropertyVec,
};
use azul_desktop::callbacks::{
    Update, RefAny, CallbackInfo, Callback,
};
use azul_core::window::{
    LogicalSize, LogicalPosition, LogicalRect,
    CursorPosition::InWindow, Menu, MenuItem, StringMenuItem,
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
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeTypeIdInfoMap {
    pub key: NodeTypeId,
    pub value: NodeTypeInfo,
}

impl_vec!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec, NodeTypeIdInfoMapVecDestructor);
impl_vec_clone!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec, NodeTypeIdInfoMapVecDestructor);
impl_vec_debug!(NodeTypeIdInfoMap, NodeTypeIdInfoMapVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputOutputTypeIdInfoMap {
    pub key: InputOutputTypeId,
    pub value: InputOutputInfo,
}

impl_vec!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec, InputOutputTypeIdInfoMapVecDestructor);
impl_vec_clone!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec, InputOutputTypeIdInfoMapVecDestructor);
impl_vec_debug!(InputOutputTypeIdInfoMap, InputOutputTypeIdInfoMapVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeIdNodeMap {
    pub key: NodeGraphNodeId,
    pub value: Node,
}

impl_vec!(NodeIdNodeMap, NodeIdNodeMapVec, NodeIdNodeMapVecDestructor);
impl_vec_clone!(NodeIdNodeMap, NodeIdNodeMapVec, NodeIdNodeMapVecDestructor);
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

pub type OnNodeAddedCallbackType = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo, new_node_type: NodeTypeId, new_node_position: NodePosition) -> Update;
impl_callback!(OnNodeAdded, OptionOnNodeAdded, OnNodeAddedCallback, OnNodeAddedCallbackType);

pub type OnNodeRemovedCallbackType = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo, node_id_to_remove: NodeGraphNodeId) -> Update;
impl_callback!(OnNodeRemoved, OptionOnNodeRemoved, OnNodeRemovedCallback, OnNodeRemovedCallbackType);

pub type OnNodeGraphDraggedCallbackType = extern "C" fn(data: &mut RefAny,info: &mut CallbackInfo, drag_amount: GraphDragAmount) -> Update;
impl_callback!(OnNodeGraphDragged, OptionOnNodeGraphDragged, OnNodeGraphDraggedCallback, OnNodeGraphDraggedCallbackType);

pub type OnNodeDraggedCallbackType = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo, node_dragged: NodeGraphNodeId, drag_amount: NodeDragAmount) -> Update;
impl_callback!(OnNodeDragged, OptionOnNodeDragged, OnNodeDraggedCallback, OnNodeDraggedCallbackType);

pub type OnNodeConnectedCallbackType = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo, input: NodeGraphNodeId, input_index: usize, output: NodeGraphNodeId, output_index: usize) -> Update;
impl_callback!(OnNodeConnected, OptionOnNodeConnected, OnNodeConnectedCallback, OnNodeConnectedCallbackType);

pub type OnNodeInputDisconnectedCallbackType = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo, input: NodeGraphNodeId, input_index: usize) -> Update;
impl_callback!(OnNodeInputDisconnected, OptionOnNodeInputDisconnected, OnNodeInputDisconnectedCallback, OnNodeInputDisconnectedCallbackType);

pub type OnNodeOutputDisconnectedCallbackType = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo, output: NodeGraphNodeId, output_index: usize) -> Update;
impl_callback!(OnNodeOutputDisconnected, OptionOnNodeOutputDisconnected, OnNodeOutputDisconnectedCallback, OnNodeOutputDisconnectedCallbackType);

pub type OnNodeFieldEditedCallbackType = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo, node_id: NodeGraphNodeId, field_id: usize, node_type: NodeTypeId, new_value: NodeTypeFieldValue) -> Update;
impl_callback!(OnNodeFieldEdited, OptionOnNodeFieldEdited, OnNodeFieldEditedCallback, OnNodeFieldEditedCallbackType);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct InputOutputTypeId {
    pub inner: u64
}

impl_vec!(InputOutputTypeId, InputOutputTypeIdVec, InputOutputTypeIdVecDestructor);
impl_vec_clone!(InputOutputTypeId, InputOutputTypeIdVec, InputOutputTypeIdVecDestructor);
impl_vec_debug!(InputOutputTypeId, InputOutputTypeIdVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NodeTypeId {
    pub inner: u64
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NodeGraphNodeId {
    pub inner: u64
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Node {
    pub node_type: NodeTypeId,
    pub position: NodePosition,
    pub fields: NodeTypeFieldVec,
    pub connect_in: InputConnectionVec,
    pub connect_out: OutputConnectionVec,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct NodeTypeField {
    key: AzString,
    value: NodeTypeFieldValue,
}

impl_vec!(NodeTypeField, NodeTypeFieldVec, NodeTypeFieldVecDestructor);
impl_vec_clone!(NodeTypeField, NodeTypeFieldVec, NodeTypeFieldVecDestructor);
impl_vec_debug!(NodeTypeField, NodeTypeFieldVec);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum NodeTypeFieldValue {
    TextInput(AzString),
    Number(f32),
    Checkbox(bool),
    ColorInput(ColorU),
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputConnection {
    pub input_index: usize,
    pub connects_to: OutputNodeAndIndexVec,
}

impl_vec!(InputConnection, InputConnectionVec, InputConnectionVecDestructor);
impl_vec_clone!(InputConnection, InputConnectionVec, InputConnectionVecDestructor);
impl_vec_debug!(InputConnection, InputConnectionVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct OutputNodeAndIndex {
    pub node_id: NodeGraphNodeId,
    pub output_index: usize,
}

impl_vec!(OutputNodeAndIndex, OutputNodeAndIndexVec, OutputNodeAndIndexVecDestructor);
impl_vec_clone!(OutputNodeAndIndex, OutputNodeAndIndexVec, OutputNodeAndIndexVecDestructor);
impl_vec_debug!(OutputNodeAndIndex, OutputNodeAndIndexVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct OutputConnection {
    pub input_index: usize,
    pub connects_to: InputNodeAndIndexVec,
}

impl_vec!(OutputConnection, OutputConnectionVec, OutputConnectionVecDestructor);
impl_vec_clone!(OutputConnection, OutputConnectionVec, OutputConnectionVecDestructor);
impl_vec_debug!(OutputConnection, OutputConnectionVec);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct InputNodeAndIndex {
    pub node_id: NodeGraphNodeId,
    pub input_index: usize,
}

impl_vec!(InputNodeAndIndex, InputNodeAndIndexVec, InputNodeAndIndexVecDestructor);
impl_vec_clone!(InputNodeAndIndex, InputNodeAndIndexVec, InputNodeAndIndexVecDestructor);
impl_vec_debug!(InputNodeAndIndex, InputNodeAndIndexVec);

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct NodeTypeInfo {
    /// Whether this node type is a "root" type
    pub is_root: bool,
    /// Name of the node type
    pub name: AzString,
    /// List of inputs for this node
    pub inputs: InputOutputTypeIdVec,
    /// List of outputs for this node
    pub outputs: InputOutputTypeIdVec,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct InputOutputInfo {
    /// Data type of this input / output
    pub data_type: AzString,
    /// Which color to use for the input / output
    pub color: ColorU,
}

/// Things only relevant to the display of the node in an interactive editor
/// - such as x and y position in the node graph, name, etc.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct NodePosition {
    /// X Position of the node
    pub x: f32,
    /// Y Position of the node
    pub y: f32,
}

// specifies what field has changed
pub enum NodeFieldChange {
    String { old: String, new: String },
    Number { old: f32, new: f32 },
    Color { old: ColorU, new: ColorU },
    Checkbox { old: bool, new: bool },
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
    /// - `NodeGraphError::NodeInvalidIndex(n, u)`: One node has an invalid `output` or `input` index
    /// - `NodeGraphError::NodeMimeTypeMismatch`: The types of two connected
    ///    `outputs` and `inputs` isn't the same
    /// - `Ok`: The insertion of the new node went well.
    pub fn connect_input_output(
        &mut self,
        input_node_id: NodeGraphNodeId,
        input_index: usize,
        output_node_id: NodeGraphNodeId,
        output_index: usize,
    ) -> Result<(), NodeGraphError> {

        // Verify that the node type of the connection matches
        let _ = self.verify_nodetype_match(
            output_node_id,
            output_index,
            input_node_id,
            input_index
        )?;

        // connect input -> output
        {
            let mut input_node = self.nodes
                .get_mut(&input_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;
            input_node.connect_in.insert(input_index, (output_node_id, output_index));
        }

        // connect output -> input
        {
            let mut output_node = self.nodes
                .get_mut(&output_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;
            output_node.connect_out.insert(output_index, (input_node_id, input_index));
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
    /// - `Err(NodeGraphError::NodeInvalidNode(n))`: The node at index `input_node_id` does not exist
    /// - `Err(NodeGraphError::NodeInvalidIndex(n, u))`: One node has an invalid `input` or `output` index
    /// - `Err(NodeGraphError::NodeMimeTypeMismatch)`: The types of two connected `input` and `output` does not match
    /// - `Ok(())`: The insertion of the new node went well.
    ///
    pub fn disconnect_input(
        &mut self,
        input_node_id: NodeGraphNodeId,
        input_index: usize,
    ) -> Result<(), NodeGraphError> {

        let (output_node_id, output_index) = {
            let input_node = self.nodes
                .get_mut(&input_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;

            match input_node.connect_in.get(&input_index) {
                None => return Ok(()),
                Some(s) => *s,
            }
        };

        // verify that the node type of the connection matches
        let _ = self.verify_nodetype_match(
            output_node_id,
            output_index,
            input_node_id,
            input_index
        )?;

        // disconnect input -> output

        {
            let mut input_node = self.nodes
                .get_mut(&input_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;
            input_node.connect_in.remove(&input_index);
        }

        {
            let mut output_node = self.nodes
                .get_mut(&output_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;
            output_node.connect_out.remove(&output_index);
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
    /// - `Err(NodeGraphError::NodeInvalidNode(n))`: The node at index `output_node_id` does not exist
    /// - `Err(NodeGraphError::NodeInvalidIndex(n, u))`: One node has an invalid `input` or `output` index
    /// - `Err(NodeGraphError::NodeMimeTypeMismatch)`: The types of two connected `input` and `output` does not match
    /// - `Ok(())`: The insertion of the new node went well.
    ///
    pub fn disconnect_output(
        &mut self,
        output_node_id: NodeGraphNodeId,
        output_index: usize,
    ) -> Result<(), NodeGraphError> {

        let (input_node_id, input_index) = {
            let output_node = self.nodes
                .get(&output_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;

            match output_node.connect_out.get(&output_index) {
                None => return Ok(()),
                Some(s) => *s,
            }
        };

        // verify that the node type of the connection matches
        let _ = self.verify_nodetype_match(
            output_node_id,
            output_index,
            input_node_id,
            input_index
        )?;

        {
            let mut output_node = self.nodes
                .get_mut(&output_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;
            output_node.connect_out.remove(&output_index);
        }

        {
            let mut input_node = self.nodes
                .get_mut(&input_node_id)
                .ok_or(NodeGraphError::NodeInvalidNode)?;

            // disconnect
            input_node.connect_in.remove(&input_index);
        }

        Ok(())
    }

    /// Verifies that the node types of two connections match
    pub fn verify_nodetype_match(
        &self,
        output_node_id: NodeGraphNodeId,
        output_index: usize,
        input_node_id: NodeGraphNodeId,
        input_index: usize,
    ) -> Result<(), NodeGraphError> {

        let output_node = self.nodes
            .get(&output_node_id)
            .ok_or(NodeGraphError::NodeInvalidNode)?;
        let output_node_type = self.node_types
            .get(&output_node.node_type)
            .ok_or(NodeGraphError::NodeInvalidNode)?;
        let output_type = output_node_type
            .outputs
            .get(output_index)
            .ok_or(NodeGraphError::NodeInvalidIndex)?;

        let input_node = self.nodes
            .get(&input_node_id)
            .ok_or(NodeGraphError::NodeInvalidNode)?;
        let input_node_type = self.node_types
            .get(&input_node.node_type)
            .ok_or(NodeGraphError::NodeInvalidNode)?;
        let input_type = input_node_type
            .inputs
            .get(input_index)
            .ok_or(NodeGraphError::NodeInvalidIndex)?;

        // Input / Output do not have the same TypeId
        if input_type != output_type {
            return Err(NodeGraphError::NodeMimeTypeMismatch);
        }

        Ok(())
    }

    pub fn dom(self) -> Dom {

        static NODEGRAPH_CLASS: &[IdOrClass] = &[
            Class(AzString::from_const_str("nodegraph"))
        ];

        static NODEGRAPH_BACKGROUND: &[StyleBackgroundContent] = &[
            StyleBackgroundContent::Image(AzString::from_const_str("nodegraph-background"))
        ];

        static NODEGRAPH_NODES_CONTAINER_CLASS: &[IdOrClass] = &[
            Class(AzString::from_const_str("nodegraph-nodes-container"))
        ];

        static NODEGRAPH_NODES_CONTAINER_PROPS: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
            Normal(CssProperty::position(LayoutPosition::Absolute)),
        ];

        let nodegraph_wrapper_props = vec![
            Normal(CssProperty::overflow_x(LayoutOverflow::Hidden)),
            Normal(CssProperty::overflow_y(LayoutOverflow::Hidden)),
            Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
            Normal(CssProperty::background_content(StyleBackgroundContentVec::from_const_slice(NODEGRAPH_BACKGROUND))),
            Normal(CssProperty::background_repeat(vec![StyleBackgroundRepeat::Repeat].into())),
            Normal(CssProperty::background_position(vec![StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Exact(PixelValue::const_px(0)),
                vertical: BackgroundPositionVertical::Exact(PixelValue::const_px(0)),
            }].into())),
        ];

        let nodegraph_props = vec![
            Normal(CssProperty::overflow_x(LayoutOverflow::Hidden)),
            Normal(CssProperty::overflow_y(LayoutOverflow::Hidden)),
            Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
            Normal(CssProperty::position(LayoutPosition::Relative)),
        ];


        let callbacks = self.callbacks.clone();

        let node_graph_local_dataset = RefAny::new(NodeGraphLocalDataset {
            node_graph: self,
            last_input_or_output_clicked: None,
            callbacks: callbacks,
        });

        Dom::div()
        .with_inline_css_props(nodegraph_wrapper_props.into())
        .with_context_menu(Menu::new(vec![
            MenuItem::String(StringMenuItem::new("Add node".into()).with_children(
                self.node_types
                .iter()
                .map(|(node_type_id, node_type_info)| {
                     let context_menu_local_dataset = RefAny::new(ContextMenuEntryLocalDataset {
                         node_type: *node_type_id,
                         backref: node_graph_local_dataset.clone(), // RefAny<NodeGraphLocalDataset>
                     });

                     MenuItem::String(
                         StringMenuItem::new(node_type_info.name.clone().into())
                         .with_callback(context_menu_local_dataset, nodegraph_context_menu_click)
                     )
                 })
                 .collect::<Vec<_>>().into()
             )),
        ].into()))
        .with_children(vec![
           Dom::div()
           .with_ids_and_classes(IdOrClassVec::from_const_slice(NODEGRAPH_CLASS))
           .with_inline_css_props(nodegraph_props.into())
           .with_callbacks(vec![
               CallbackData {
                   event: EventFilter::Hover(HoverEventFilter::MouseOver),
                   data: node_graph_local_dataset.clone(),
                   callback: Callback { cb: nodegraph_drag_graph },
               }
           ].into())
           .with_children({
                let node_connection_marker = RefAny::new(NodeConnectionMarkerDataset { });
                vec![
                      // nodes
                      self.nodes.iter()
                      .filter_map(|(id, node)| {
                           let node_type_info = self.node_types.get(&node.node_type)?;
                           let node_local_dataset = NodeLocalDataset {
                               node_id: *id,
                               node_connection_marker: node_connection_marker.clone(),
                               backref: node_graph_local_dataset.clone(),
                           };
                           Some(render_node(node, (self.offset.x, self.offset.y), node_type_info, node_local_dataset))
                       })
                      .collect::<Dom>()
                      .with_ids_and_classes(IdOrClassVec::from_const_slice(NODEGRAPH_NODES_CONTAINER_CLASS))
                      .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(NODEGRAPH_NODES_CONTAINER_PROPS)),

                      // connections
                      render_connections(&self, node_connection_marker),

                ].into()
            })
        ].into())
    }
}

// dataset set on the top-level nodegraph node,
// containing all the state of the node graph
struct NodeGraphLocalDataset {
    node_graph: NodeGraph,
    last_input_or_output_clicked: Option<(NodeGraphNodeId, InputOrOutput)>,
    callbacks: NodeGraphCallbacks,
}

struct ContextMenuEntryLocalDataset {
    node_type: NodeTypeId,
    backref: RefAny, // RefAny<NodeGraphLocalDataset>
}

struct NodeConnectionMarkerDataset { }

struct NodeLocalDataset {
    node_id: NodeGraphNodeId,
    node_connection_marker: RefAny, // Ref<NodeConnectionMarkerDataset>
    backref: RefAny, // RefAny<NodeGraphLocalDataset>
}

#[derive(Copy, Clone)]
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
}

fn render_node(node: &Node, graph_offset: (f32, f32), node_info: &NodeTypeInfo, mut node_local_dataset: NodeLocalDataset) -> Dom {

    use azul_desktop::css::*;
    use azul_desktop::dom::{
        Dom, DomVec, IdOrClass, IdOrClassVec,
        NodeDataInlineCssPropertyVec,
        IdOrClass::{Class, Id},
        NodeDataInlineCssProperty, TabIndex,
    };
    use azul_desktop::css::AzString;

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
                from: DirectionCorner::Left,
                to: DirectionCorner::Right,
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
                from: DirectionCorner::Right,
                to: DirectionCorner::Left,
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
                from: DirectionCorner::Top,
                to: DirectionCorner::Bottom,
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
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 204,
            },
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(100),
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        },
    ];
    const LINEAR_COLOR_STOP_7397113864565941600_ITEMS: &[NormalizedLinearColorStop] = &[
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(0),
            color: ColorU {
                r: 229,
                g: 57,
                b: 53,
                a: 255,
            },
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(100),
            color: ColorU {
                r: 227,
                g: 93,
                b: 91,
                a: 255,
            },
        },
    ];
    const LINEAR_COLOR_STOP_15596411095679453272_ITEMS: &[NormalizedLinearColorStop] = &[
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(0),
            color: ColorU {
                r: 47,
                g: 49,
                b: 54,
                a: 255,
            },
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(50),
            color: ColorU {
                r: 47,
                g: 49,
                b: 54,
                a: 255,
            },
        },
        NormalizedLinearColorStop {
            offset: PercentageValue::const_new(100),
            color: ColorU {
                r: 32,
                g: 34,
                b: 37,
                a: 255,
            },
        },
    ];

    const CSS_MATCH_10339190304804100510_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_output_wrapper
        NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft {
            inner: PixelValue::const_px(0),
        }))),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
    ];
    const CSS_MATCH_10339190304804100510: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_10339190304804100510_PROPERTIES);

    const CSS_MATCH_11452431279102104133_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_input_connection_label
        NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
            StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
            StyleFontSize {
                inner: PixelValue::const_px(12),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight {
                inner: PixelValue::const_px(15),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Right,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth {
                inner: PixelValue::const_px(100),
            },
        ))),
    ];
    const CSS_MATCH_11452431279102104133: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11452431279102104133_PROPERTIES);

    const CSS_MATCH_1173826950760010563_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_configuration_field_value:focus
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                inner: ColorU {
                    r: 0,
                    g: 131,
                    b: 176,
                    a: 119,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        // .node_configuration_field_value
        NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
            LayoutAlignItems::Center,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_524016094839686509_ITEMS,
            )),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                inner: ColorU {
                    r: 54,
                    g: 57,
                    b: 63,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(1),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Left,
        ))),
    ];
    const CSS_MATCH_1173826950760010563: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1173826950760010563_PROPERTIES);

    const CSS_MATCH_1198521124955124418_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_configuration_field_label
        NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
            LayoutAlignItems::Center,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(1),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::MaxWidth(LayoutMaxWidthValue::Exact(
            LayoutMaxWidth {
                inner: PixelValue::const_px(120),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(10),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Left,
        ))),
    ];
    const CSS_MATCH_1198521124955124418: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1198521124955124418_PROPERTIES);

    const CSS_MATCH_12038890904436132038_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_output_connection_label_wrapper
        NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_10430246856047584562_ITEMS,
            )),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(5),
            },
        ))),
    ];
    const CSS_MATCH_12038890904436132038: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12038890904436132038_PROPERTIES);

    const CSS_MATCH_12400244273289328300_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_output_container
        NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
            LayoutMarginTop {
                inner: PixelValue::const_px(10),
            },
        ))),
    ];
    const CSS_MATCH_12400244273289328300: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12400244273289328300_PROPERTIES);


    const CSS_MATCH_14906563417280941890_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .outputs
        NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(0),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Relative,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth {
                inner: PixelValue::const_px(0),
            },
        ))),
    ];
    const CSS_MATCH_14906563417280941890: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14906563417280941890_PROPERTIES);

    const CSS_MATCH_16946967739775705757_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .inputs
        NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(0),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Relative,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth {
                inner: PixelValue::const_px(0),
            },
        ))),
    ];
    const CSS_MATCH_16946967739775705757: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_16946967739775705757_PROPERTIES);

    const CSS_MATCH_1739273067404038547_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_label
        NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
            StyleFontSize {
                inner: PixelValue::const_px(18),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight {
                inner: PixelValue::const_px(50),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(5),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
            LayoutPaddingTop {
                inner: PixelValue::const_px(10),
            },
        ))),
    ];
    const CSS_MATCH_1739273067404038547: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1739273067404038547_PROPERTIES);

    const CSS_MATCH_2008162367868363199_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_output_connection_label
        NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
            StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
            StyleFontSize {
                inner: PixelValue::const_px(12),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight {
                inner: PixelValue::const_px(15),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Left,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth {
                inner: PixelValue::const_px(100),
            },
        ))),
    ];
    const CSS_MATCH_2008162367868363199: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2008162367868363199_PROPERTIES);

    const CSS_MATCH_2639191696846875011_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_configuration_field_container
        NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
            LayoutPaddingTop {
                inner: PixelValue::const_px(3),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(
            LayoutPaddingBottomValue::Exact(LayoutPaddingBottom {
                inner: PixelValue::const_px(3),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(5),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(
            LayoutPaddingRightValue::Exact(LayoutPaddingRight {
                inner: PixelValue::const_px(5),
            }),
        )),
    ];
    const CSS_MATCH_2639191696846875011: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2639191696846875011_PROPERTIES);



    const CSS_MATCH_3354247437065914166_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_body
        NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Relative,
        ))),
    ];
    const CSS_MATCH_3354247437065914166: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_3354247437065914166_PROPERTIES);

    const CSS_MATCH_4700400755767504372_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_input_connection_label_wrapper
        NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_11936041127084538304_ITEMS,
            )),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(
            LayoutPaddingRightValue::Exact(LayoutPaddingRight {
                inner: PixelValue::const_px(5),
            }),
        )),
    ];
    const CSS_MATCH_4700400755767504372: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4700400755767504372_PROPERTIES);

    const CSS_MATCH_705881630351954657_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_input_wrapper
        NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Visible,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Right(LayoutRightValue::Exact(
            LayoutRight {
                inner: PixelValue::const_px(0),
            },
        ))),
    ];
    const CSS_MATCH_705881630351954657: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_705881630351954657_PROPERTIES);

    const CSS_MATCH_7395766480280098891_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_close_button
        NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
            LayoutAlignItems::Center,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_17648039690071193942_ITEMS,
            )),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 153,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowBottom(
            StyleBoxShadowValue::Exact(StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::Cursor(StyleCursorValue::Exact(
            StyleCursor::Pointer,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
            StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_11383897783350685780_ITEMS),
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight {
                inner: PixelValue::const_px(20),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
            StyleTextAlign::Center,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(
            StyleTransformVec::from_const_slice(STYLE_TRANSFORM_14683950870521466298_ITEMS),
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth {
                inner: PixelValue::const_px(20),
            },
        ))),
    ];
    const CSS_MATCH_7395766480280098891: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7395766480280098891_PROPERTIES);

    const CSS_MATCH_7432473243011547380_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_content_wrapper
        NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_15813232491335471489_ITEMS,
            )),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowBottom(
            StyleBoxShadowValue::Exact(StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::const_new(1),
            },
        ))),
    ];
    const CSS_MATCH_7432473243011547380: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7432473243011547380_PROPERTIES);

    const CSS_MATCH_9863994880298313101_PROPERTIES: &[NodeDataInlineCssProperty] = &[
        // .node_input_container
        NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(
            LayoutDisplay::Flex,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(
            LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
            LayoutMarginTop {
                inner: PixelValue::const_px(10),
            },
        ))),
    ];
    const CSS_MATCH_9863994880298313101: NodeDataInlineCssPropertyVec =
        NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9863994880298313101_PROPERTIES);

    // NODE RENDER FUNCTION BEGIN

    let node_transform = StyleTransformTranslate2D {
        x: PixelValue::px(graph_offset.0 + node.position.x),
        y: PixelValue::px(graph_offset.1 + node.position.y),
    };

    // get names and colors for inputs / outputs
    let inputs = node_info.inputs.iter().filter_map(|io_id| {
        let node_graph_ref = node_local_dataset.backref.downcast_ref::<NodeGraphLocalDataset>()?;
        let io_info = node_graph_ref.node_graph.input_output_types.get(io_id)?;
        Some((io_info.data_type.clone(), io_info.color.clone()))
    }).collect::<Vec<_>>();

    let outputs = node_info.outputs.iter().filter_map(|io_id| {
        let node_graph_ref = node_local_dataset.backref.downcast_ref::<NodeGraphLocalDataset>()?;
        let io_info = node_graph_ref.node_graph.input_output_types.get(io_id)?;
        Some((io_info.data_type.clone(), io_info.color.clone()))
    }).collect::<Vec<_>>();

    let node_local_dataset = RefAny::new(node_local_dataset);

    Dom::div()
    .with_callbacks(vec![
        CallbackData {
            event: EventFilter::Hover(HoverEventFilter::MouseOver),
            data: node_local_dataset.clone(),
            callback: Callback { cb: nodegraph_drag_node },
        },
    ].into())
    .with_inline_css_props(vec![
        // .node_graph_node
        NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
            StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_11535310356736632656_ITEMS,
            )),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                inner: ColorU {
                    r: 0,
                    g: 180,
                    b: 219,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                inner: ColorU {
                    r: 0,
                    g: 180,
                    b: 219,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                inner: ColorU {
                    r: 0,
                    g: 180,
                    b: 219,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                inner: ColorU {
                    r: 0,
                    g: 180,
                    b: 219,
                    a: 255,
                },
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::const_px(1),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
            StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowBottom(
            StyleBoxShadowValue::Exact(StyleBoxShadow {
                offset: [
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                    PixelValueNoPercent {
                        inner: PixelValue::const_px(0),
                    },
                ],
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
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(
            StyleTextColor {
                inner: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
            StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight {
                inner: PixelValue::const_px(300),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
            LayoutPaddingTop {
                inner: PixelValue::const_px(10),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(
            LayoutPaddingBottomValue::Exact(LayoutPaddingBottom {
                inner: PixelValue::const_px(10),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::const_px(10),
            },
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(
            LayoutPaddingRightValue::Exact(LayoutPaddingRight {
                inner: PixelValue::const_px(10),
            }),
        )),
        NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
            LayoutPosition::Absolute,
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(
            vec![StyleTransform::Translate(node_transform)].into(),
        ))),
        NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth {
                inner: PixelValue::const_px(250),
            },
        ))),
    ].into())
    .with_ids_and_classes({
        const IDS_AND_CLASSES_4480169002427296613: &[IdOrClass] =
            &[Class(AzString::from_const_str("node_graph_node"))];
        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_4480169002427296613)
    })
    .with_children(DomVec::from_vec(vec![
        Dom::text(AzString::from_const_str("X"))
            .with_inline_css_props(CSS_MATCH_7395766480280098891)
            .with_callbacks(vec![
                CallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    data: node_local_dataset.clone(),
                    callback: Callback { cb: nodegraph_delete_node },
                },
            ].into())
            .with_ids_and_classes({
                const IDS_AND_CLASSES_7122017923389407516: &[IdOrClass] =
                    &[Class(AzString::from_const_str("node_close_button"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_7122017923389407516)
            }),
        Dom::text(node_info.name.clone().into())
            .with_inline_css_props(CSS_MATCH_1739273067404038547)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_15777790571346582635: &[IdOrClass] =
                    &[Class(AzString::from_const_str("node_label"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15777790571346582635)
            }),
        Dom::div()
            .with_inline_css_props(CSS_MATCH_3354247437065914166)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_5590500152394859708: &[IdOrClass] =
                    &[Class(AzString::from_const_str("node_body"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5590500152394859708)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                    .with_inline_css_props(CSS_MATCH_16946967739775705757)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3626404106673061698: &[IdOrClass] =
                            &[Class(AzString::from_const_str("inputs"))];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3626404106673061698)
                    })
                    .with_children(DomVec::from_vec(vec![Dom::div()
                        .with_inline_css_props(CSS_MATCH_705881630351954657)
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

                                Dom::div()
                                    .with_inline_css_props(CSS_MATCH_9863994880298313101)
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
                                        Dom::div()
                                            .with_inline_css_props(
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
                                            .with_children(DomVec::from_vec(vec![Dom::text(
                                                input_label.into(),
                                            )
                                            .with_inline_css_props(
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
                                        Dom::div()
                                            .with_callbacks(vec![
                                                CallbackData {
                                                    event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                                                    data: RefAny::new(NodeInputOutputLocalDataset {
                                                        io_id: Input(io_id),
                                                        backref: node_local_dataset.clone(),
                                                    }),
                                                    callback: Callback { cb: nodegraph_input_output_connect },
                                                },
                                                CallbackData {
                                                    event: EventFilter::Hover(HoverEventFilter::RightMouseUp),
                                                    data: RefAny::new(NodeInputOutputLocalDataset {
                                                        io_id: Input(io_id),
                                                        backref: node_local_dataset.clone(),
                                                    }),
                                                    callback: Callback { cb: nodegraph_input_output_disconnect },
                                                },
                                            ].into())
                                            .with_inline_css_props(NodeDataInlineCssPropertyVec::from_vec(vec![
                                                    // .node_input
                                                    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
                                                        StyleBackgroundContentVecValue::Exact(vec![StyleBackgroundContent::Color(input_color)].into()),
                                                    )),
                                                    NodeDataInlineCssProperty::Normal(CssProperty::Cursor(StyleCursorValue::Exact(
                                                        StyleCursor::Pointer,
                                                    ))),
                                                    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
                                                        LayoutHeight {
                                                            inner: PixelValue::const_px(15),
                                                        },
                                                    ))),
                                                    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
                                                        LayoutWidth {
                                                            inner: PixelValue::const_px(15),
                                                        },
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
                Dom::div()
                    .with_inline_css_props(CSS_MATCH_7432473243011547380)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_746059979773622802: &[IdOrClass] =
                            &[Class(AzString::from_const_str("node_content_wrapper"))];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_746059979773622802)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::div()
                            .with_inline_css_props(CSS_MATCH_2639191696846875011)
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
                                Dom::text(AzString::from_const_str("Key"))
                                    .with_inline_css_props(CSS_MATCH_1198521124955124418)
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
                                Dom::text(AzString::from_const_str("Value"))
                                    .with_inline_css_props(CSS_MATCH_1173826950760010563)
                                    .with_ids_and_classes({
                                        const IDS_AND_CLASSES_1997748148349826621:
                                            &[IdOrClass] =
                                            &[Class(AzString::from_const_str(
                                                "node_configuration_field_value",
                                            ))];
                                        IdOrClassVec::from_const_slice(
                                            IDS_AND_CLASSES_1997748148349826621,
                                        )
                                    })
                                    .with_tab_index(TabIndex::Auto),
                            ])),
                        Dom::div()
                            .with_inline_css_props(CSS_MATCH_2639191696846875011)
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
                                Dom::text(AzString::from_const_str("Key"))
                                    .with_inline_css_props(CSS_MATCH_1198521124955124418)
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
                                Dom::text(AzString::from_const_str("Value"))
                                    .with_inline_css_props(CSS_MATCH_1173826950760010563)
                                    .with_ids_and_classes({
                                        const IDS_AND_CLASSES_1997748148349826621:
                                            &[IdOrClass] =
                                            &[Class(AzString::from_const_str(
                                                "node_configuration_field_value",
                                            ))];
                                        IdOrClassVec::from_const_slice(
                                            IDS_AND_CLASSES_1997748148349826621,
                                        )
                                    })
                                    .with_tab_index(TabIndex::Auto),
                            ])),
                        Dom::div()
                            .with_inline_css_props(CSS_MATCH_2639191696846875011)
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
                                Dom::text(AzString::from_const_str("Key"))
                                    .with_inline_css_props(CSS_MATCH_1198521124955124418)
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
                                Dom::text(AzString::from_const_str("Value"))
                                    .with_inline_css_props(CSS_MATCH_1173826950760010563)
                                    .with_ids_and_classes({
                                        const IDS_AND_CLASSES_1997748148349826621:
                                            &[IdOrClass] =
                                            &[Class(AzString::from_const_str(
                                                "node_configuration_field_value",
                                            ))];
                                        IdOrClassVec::from_const_slice(
                                            IDS_AND_CLASSES_1997748148349826621,
                                        )
                                    })
                                    .with_tab_index(TabIndex::Auto),
                            ])),
                        Dom::div()
                            .with_inline_css_props(CSS_MATCH_2639191696846875011)
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
                                Dom::text(AzString::from_const_str("Key"))
                                    .with_inline_css_props(CSS_MATCH_1198521124955124418)
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
                                Dom::text(AzString::from_const_str("Value"))
                                    .with_inline_css_props(CSS_MATCH_1173826950760010563)
                                    .with_ids_and_classes({
                                        const IDS_AND_CLASSES_1997748148349826621:
                                            &[IdOrClass] =
                                            &[Class(AzString::from_const_str(
                                                "node_configuration_field_value",
                                            ))];
                                        IdOrClassVec::from_const_slice(
                                            IDS_AND_CLASSES_1997748148349826621,
                                        )
                                    })
                                    .with_tab_index(TabIndex::Auto),
                            ])),
                        Dom::div()
                            .with_inline_css_props(CSS_MATCH_2639191696846875011)
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
                                Dom::text(AzString::from_const_str("Key"))
                                    .with_inline_css_props(CSS_MATCH_1198521124955124418)
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
                                Dom::text(AzString::from_const_str("Value"))
                                    .with_inline_css_props(CSS_MATCH_1173826950760010563)
                                    .with_ids_and_classes({
                                        const IDS_AND_CLASSES_1997748148349826621:
                                            &[IdOrClass] =
                                            &[Class(AzString::from_const_str(
                                                "node_configuration_field_value",
                                            ))];
                                        IdOrClassVec::from_const_slice(
                                            IDS_AND_CLASSES_1997748148349826621,
                                        )
                                    })
                                    .with_tab_index(TabIndex::Auto),
                            ])),
                    ])),
                Dom::div()
                    .with_inline_css_props(CSS_MATCH_14906563417280941890)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_4737474624251936466: &[IdOrClass] =
                            &[Class(AzString::from_const_str("outputs"))];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_4737474624251936466)
                    })
                    .with_children(DomVec::from_vec(vec![Dom::div()
                        .with_inline_css_props(CSS_MATCH_10339190304804100510)
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
                                Dom::div()
                                    .with_inline_css_props(CSS_MATCH_12400244273289328300)
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
                                        Dom::div()
                                            .with_callbacks(vec![
                                                CallbackData {
                                                    event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                                                    data: RefAny::new(NodeInputOutputLocalDataset {
                                                        io_id: Output(io_id),
                                                        backref: node_local_dataset.clone(),
                                                    }),
                                                    callback: Callback { cb: nodegraph_input_output_connect },
                                                },
                                                CallbackData {
                                                    event: EventFilter::Hover(HoverEventFilter::RightMouseUp),
                                                    data: RefAny::new(NodeInputOutputLocalDataset {
                                                        io_id: Output(io_id),
                                                        backref: node_local_dataset.clone(),
                                                    }),
                                                    callback: Callback { cb: nodegraph_input_output_disconnect },
                                                },
                                            ].into())
                                            .with_inline_css_props(
                                                NodeDataInlineCssPropertyVec::from_vec(vec![
                                                    // .node_output
                                                    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
                                                        StyleBackgroundContentVecValue::Exact(vec![
                                                            StyleBackgroundContent::Color(output_color)
                                                        ].into()),
                                                    )),
                                                    NodeDataInlineCssProperty::Normal(CssProperty::Cursor(StyleCursorValue::Exact(
                                                        StyleCursor::Pointer,
                                                    ))),
                                                    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
                                                        LayoutHeight {
                                                            inner: PixelValue::const_px(15),
                                                        },
                                                    ))),
                                                    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
                                                        LayoutWidth {
                                                            inner: PixelValue::const_px(15),
                                                        },
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
                                        Dom::div()
                                            .with_inline_css_props(
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
                                            .with_children(DomVec::from_vec(vec![Dom::text(
                                                output_label.into(),
                                            )
                                            .with_inline_css_props(
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
    .with_dataset(node_local_dataset)
}

fn render_connections(node_graph: &NodeGraph,root_marker_nodedata: RefAny) -> Dom {

    const THEME_RED: ColorU = ColorU { r: 255,  g: 0,  b: 0,  a: 255 }; // #484c52
    const BACKGROUND_THEME_RED: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(THEME_RED)];
    const BACKGROUND_COLOR_RED: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_RED);

    static NODEGRAPH_CONNECTIONS_CONTAINER_CLASS: &[IdOrClass] = &[
        Class(AzString::from_const_str("nodegraph-connections-container"))
    ];

    static NODEGRAPH_CONNECTIONS_CONTAINER_PROPS: &[NodeDataInlineCssProperty] = &[
        Normal(CssProperty::position(LayoutPosition::Absolute)),
        Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
    ];

    Dom::div()
    .with_ids_and_classes(IdOrClassVec::from_const_slice(NODEGRAPH_CONNECTIONS_CONTAINER_CLASS))
    .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(NODEGRAPH_CONNECTIONS_CONTAINER_PROPS))
    .with_dataset(root_marker_nodedata)
    .with_children({
        let mut children = Vec::new();

        for (node_id, node) in node_graph.nodes.iter() {

            for (output_index, (in_node_id, in_index)) in node.connect_out.iter() {

                let cld = ConnectionLocalDataset {
                    out_node_id: *node_id,
                    out_idx: *output_index,
                    in_node_id: *in_node_id,
                    in_idx: *in_index,
                };

                let rect = match get_rect(&node_graph, cld) {
                    Some(s) => s,
                    None => continue,
                };

                // let connection_background = draw_connection(rect.width, rect.height, connection_dot_height);

                children.push(Dom::div()
                .with_dataset(RefAny::new(cld))
                .with_inline_css_props(vec![
                    Normal(CssProperty::position(LayoutPosition::Absolute)),
                    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(
                        vec![StyleTransform::Translate(StyleTransformTranslate2D {
                            x: PixelValue::px(node_graph.offset_x + rect.origin.x),
                            y: PixelValue::px(node_graph.offset_y + rect.origin.y),
                        })].into(),
                    ))),
                    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
                        LayoutWidth { inner: PixelValue::px(rect.size.width) },
                    ))),
                    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
                        LayoutHeight { inner: PixelValue::px(rect.size.height) },
                    ))),

                    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
                        StyleBackgroundContentVecValue::Exact(
                            BACKGROUND_COLOR_RED // ImageRef::new(connection_background)
                        ),
                    )),

                ].into()))
             }
        }

        children.into()
    })
}

// calculates the rect on which the connection is drawn in the UI
fn get_rect(node_graph: &NodeGraph, connection: ConnectionLocalDataset) -> Option<LogicalRect> {

    let ConnectionLocalDataset { out_node_id, out_idx, in_node_id, in_idx } = connection;
    let out_node = node_graph.nodes.get(&out_node_id)?;
    let in_node = node_graph.nodes.get(&in_node_id)?;

    let node_width = 250.0;
    let v_offset = 71.0;
    let dist_between_nodes = 10.0;
    let connection_dot_height = 15.0;

    let x_out = out_node.position.x + node_width;
    let y_out = out_node.position.y + v_offset + (out_idx as f32 * (dist_between_nodes + connection_dot_height));

    let x_in = in_node.position.x;
    let y_in = in_node.position.y + v_offset + (in_idx as f32 * (dist_between_nodes + connection_dot_height));

    let width = (x_in - x_out).abs();
    let height = (y_in - y_out).abs() + connection_dot_height;

    let x = x_in.min(x_out);
    let y = y_in.min(y_out);

    Some(LogicalRect {
        size: LogicalSize { width, height },
        origin: LogicalPosition { x, y },
    })
}

// fn draw_connection(width: f32, height: f32, connection_dot_height: f32) -> ImageRef {
//     let start = (0.0, connection_dot_height / 2.0);
//     let end = (width, height - (connection_dot_height / 2.0));
//     let clip = RawImage::allocate_clip_mask(LayoutSize { });
//     let line = ImageRef::raw_image();
// }

extern "C" fn nodegraph_drag_graph(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut data = match data.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let prev = match info.get_previous_mouse_state().into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    let cur = info.get_current_mouse_state();
    if !(cur.left_down && prev.left_down) {
        // event is not a drag event
        return Update::DoNothing;
    }

    let (current_mouse_pos, previous_mouse_pos) = match (cur.cursor_position, prev.cursor_position) {
        (InWindow(c), InWindow(p)) => (c, p),
        _ => return Update::DoNothing,
    };

    let dx = current_mouse_pos.x - previous_mouse_pos.x;
    let dy = current_mouse_pos.y - previous_mouse_pos.y;

    let nodegraph_node = info.get_hit_node();
    let result = match data.callbacks.on_node_graph_dragged.as_mut() {
        Some((cb, data)) => (cb)(data, GraphDragAmount { x: dx, y: dy }, &mut info),
        None => Update::DoNothing,
    };

    data.node_graph.offset_x += dx;
    data.node_graph.offset_y += dy;

    // Update the visual node positions
    let node_container = match info.get_first_child(nodegraph_node).into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let mut node = match info.get_first_child(node_container).into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    loop {

        let mut node_local_dataset = match info.get_dataset(node) {
            None => return Update::DoNothing,
            Some(s) => s,
        };

        let mut node_graph_node_id = match node_local_dataset.downcast_ref::<NodeLocalDataset>() {
            Some(s) => s,
            None => continue,
        };

        let node_graph_node_id = node_graph_node_id.node_id;

        let node_position = match data.node_graph.nodes.get(&node_graph_node_id) {
            Some(s) => s.position,
            None => continue,
        };

        let node_transform = CssProperty::transform(vec![
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::px(node_position.x + data.node_graph.offset_x),
                y: PixelValue::px(node_position.y + data.node_graph.offset_y),
            })
        ].into());

        info.set_css_property(node, node_transform);

        node = match info.get_next_sibling(node).into_option() {
            Some(s) => s,
            None => break,
        };
    }

    info.stop_propagation();

    result
}

extern "C" fn nodegraph_drag_node(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut data = match data.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let prev = match info.get_previous_mouse_state().into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let cur = info.get_current_mouse_state();

    if !(cur.left_down && prev.left_down) {
        // event is not a drag event
        return Update::DoNothing;
    }

    let (current_mouse_pos, previous_mouse_pos) = match (cur.cursor_position, prev.cursor_position) {
        (InWindow(c), InWindow(p)) => (c, p),
        _ => return Update::DoNothing,
    };

    let node_graph_node_id = data.node_id;

    let dx = current_mouse_pos.x - previous_mouse_pos.x;
    let dy = current_mouse_pos.y - previous_mouse_pos.y;

    let data = &mut *data;
    let backref = &mut data.backref;
    let node_connection_marker = &mut data.node_connection_marker;

    let mut backref = match backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let nodegraph_node = info.get_hit_node();
    let result = match backref.callbacks.on_node_dragged.as_mut() {
        Some((cb, data)) => (cb)(data, node_graph_node_id, NodeDragAmount { x: dx, y: dy }, &mut info),
        None => Update::DoNothing,
    };

    // update the visual transform of the node in the UI
    let node_position = match backref.node_graph.nodes.get_mut(&node_graph_node_id) {
        Some(s) => {
            s.position.x += dx;
            s.position.y += dy;
            s.position
        },
        None => return Update::DoNothing,
    };

    info.set_css_property(info.get_hit_node(), CssProperty::transform(vec![
        StyleTransform::Translate(StyleTransformTranslate2D {
            x: PixelValue::px(node_position.x + backref.node_graph.offset_x),
            y: PixelValue::px(node_position.y + backref.node_graph.offset_y),
        })
    ].into()));

    info.stop_propagation();

    // get the NodeId of the node containing all the connection lines
    let connection_container_nodeid = match info.get_node_id_of_root_dataset(node_connection_marker.clone()).into_option() {
        Some(s) => s,
        None => return result,
    };

    // animate all the connections
    let mut first_connection_child = info.get_first_child(connection_container_nodeid).into_option();

    while let Some(connection_nodeid) = first_connection_child {

        first_connection_child = info.get_next_sibling(connection_nodeid).into_option();

        let mut dataset = match info.get_dataset(connection_nodeid) {
            Some(s) => s,
            None => continue,
        };

        let cld = match dataset.downcast_ref::<ConnectionLocalDataset>() {
            Some(s) => s,
            None => continue,
        };

        if !(cld.out_node_id == node_graph_node_id || cld.in_node_id == node_graph_node_id) {
            continue; // connection does not need to be modified
        }

        let new_rect = match get_rect(&backref.node_graph, *cld) {
            Some(s) => s,
            None => continue,
        };

        info.set_css_property(connection_nodeid, CssProperty::transform(vec![
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::px(backref.node_graph.offset_x + new_rect.origin.x),
                y: PixelValue::px(backref.node_graph.offset_y + new_rect.origin.y),
            })
        ].into()));

        info.set_css_property(connection_nodeid, CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth { inner: PixelValue::px(new_rect.size.width) },
        )));

        info.set_css_property(connection_nodeid, CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight { inner: PixelValue::px(new_rect.size.height) },
        )));

        /*
            NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
                StyleBackgroundContentVecValue::Exact(
                    BACKGROUND_COLOR_RED // ImageRef::new(connection_background)
                ),
            )),
        */

        // info.update_image(connection_nodeid, render_connection(), ImageType::Background);
    }


    result
}

extern "C" fn nodegraph_duplicate_node(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut data = match data.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    Update::DoNothing // TODO
}

extern "C" fn nodegraph_delete_node(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut data = match data.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = data.node_id.clone();

    let mut backref = match data.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = match backref.callbacks.on_node_removed.as_mut() {
        Some((cb, data)) => (cb)(data, node_id, &mut info),
        None => Update::DoNothing
    };

    result
}

extern "C" fn nodegraph_root_node_special_button_clicked(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut data = match data.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let root_node_id = data.node_id;

    let mut backref = match data.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = match backref.callbacks.on_node_root_button_clicked.as_mut() {
        Some((cb, data)) => (cb)(data, root_node_id, &mut info),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_context_menu_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut data = match data.downcast_mut::<ContextMenuEntryLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let new_node_type = data.node_type.clone();

    let mut backref = match data.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let new_node_position = info.get_cursor_relative_to_node().into_option().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));

    let result = match backref.callbacks.on_node_added.as_mut() {
        Some((cb, data)) => (cb)(data, new_node_type, NodePosition { x: new_node_position.0, y: new_node_position.1 }, &mut info),
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_input_output_connect(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    use self::InputOrOutput::*;

    let mut data = match data.downcast_mut::<NodeInputOutputLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let io_id = data.io_id.clone();

    let mut backref = match data.backref.downcast_mut::<NodeLocalDataset>() {
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
            },
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

    let result = match backref.callbacks.on_node_connected.as_mut() {
        Some((cb, data)) => {
            let r = (cb)(data, input_node, input_index, output_node, output_index, &mut info);
            backref.last_input_or_output_clicked = None;
            r
        },
        None => Update::DoNothing,
    };

    result
}

extern "C" fn nodegraph_input_output_disconnect(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    use self::InputOrOutput::*;

    let mut data = match data.downcast_mut::<NodeInputOutputLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let io_id = data.io_id.clone();

    let mut backref = match data.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_id = backref.node_id.clone();

    let mut backref = match backref.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = match io_id {
        Input(i) => {
            match backref.callbacks.on_node_input_disconnected.as_mut() {
                Some((cb, data)) => (cb)(data, node_id, i, &mut info),
                None => Update::DoNothing,
            }
        },
        Output(o) => {
            match backref.callbacks.on_node_output_disconnected.as_mut() {
                Some((cb, data)) => (cb)(data, node_id, o, &mut info),
                None => Update::DoNothing,
            }
        }
    };

    result
}