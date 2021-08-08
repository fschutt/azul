#![windows_subsystem = "windows"]

use std::collections::BTreeMap;
use azul::prelude::*;
use azul::widgets::*;
use azul::prelude::String as AzString;
use std::string::String;

// Custom node graph data model
struct MyNodeGraph {
    node_types: BTreeMap<NodeTypeId, NodeTypeInfo>,
    input_output_types: BTreeMap<InputOutputTypeId, InputOutputInfo>,
    nodes: BTreeMap<NodeGraphNodeId, MyNode>,
    offset: LogicalPosition,
}

impl Default for MyNodeGraph {
    fn default() -> Self {
        Self {
            node_types: Self::get_available_node_types(),
            input_output_types: Self::get_available_input_output_types(),
            offset: LogicalPosition { x: 0.0, y: 0.0 },
            nodes: BTreeMap::new(),
        }
    }
}

impl MyNodeGraph {

    pub fn get_available_node_types() -> BTreeMap<NodeTypeId, NodeTypeInfo> {
        let mut map = BTreeMap::new();

        map.insert(NodeTypeId { inner: 0 }, NodeTypeInfo {
            is_root: false,
            name: "My Custom Node Type".into(),
            inputs: vec![InputOutputTypeId { inner: 0 }, InputOutputTypeId { inner: 1 }].into(),
            outputs: vec![InputOutputTypeId { inner: 0 }, InputOutputTypeId { inner: 1 }].into(),
        });

        map
    }

    pub fn get_available_input_output_types() -> BTreeMap<InputOutputTypeId, InputOutputInfo> {
        let mut map = BTreeMap::new();

        map.insert(InputOutputTypeId { inner: 0 }, InputOutputInfo {
            data_type: "MyData".into(),
            color: ColorU { r: 255, g: 0, b: 0, a: 255 },
        });

        map.insert(InputOutputTypeId { inner: 1 }, InputOutputInfo {
            data_type: "MyOtherDataType".into(),
            color: ColorU { r: 255, g: 0, b: 0, a: 255 },
        });

        map
    }
}

struct MyNode {
    node_type: NodeTypeId,
    position: NodePosition,
    connect_in: BTreeMap<usize, BTreeMap<NodeGraphNodeId, usize>>,
    connect_out: BTreeMap<usize, BTreeMap<NodeGraphNodeId, usize>>,
    data: MyNodeType,
}

// Custom node type
enum MyNodeType {
    MyTypeVariant {
        field1: String,
        field2: String,
    },
}

impl MyNodeType {

    pub fn get_ids_and_classes(&self) -> NodeTypeId {
        match self {
            Self::MyTypeVariant { .. } => NodeTypeId { inner: 0 },
        }
    }

    pub fn create_default(type_id: NodeTypeId) -> Self {
        match type_id.inner {
            _ => Self::MyTypeVariant {
                field1: String::new(),
                field2: String::new(),
            }
        }
    }

    pub fn get_fields(&self) -> Vec<NodeTypeField> {
        match self {
            Self::MyTypeVariant { field1, field2 } => {
                vec![
                    NodeTypeField {
                        key: "Field 1".into(),
                        value: NodeTypeFieldValue::TextInput(field1.clone().into()),
                    },
                    NodeTypeField {
                        key: "Field 2".into(),
                        value: NodeTypeFieldValue::TextInput(field2.clone().into()),
                    }
                ]
            }
        }
    }

    pub fn edit_field(
        &mut self,
        field_idx: usize,
        node_type: NodeTypeId,
        new_value: NodeTypeFieldValue
    ) {
        use self::MyNodeType::*;
        match (self, node_type) {
            (MyTypeVariant { field1, field2 }, NodeTypeId { inner: 0 }) => {
                match (field_idx, new_value) {
                    (0, NodeTypeFieldValue::TextInput(s)) => *field1 = s.as_str().into(),
                    (1, NodeTypeFieldValue::TextInput(s)) => *field2 = s.as_str().into(),
                    _ => { },
                }
            },
            _ => { }
        }

    }
}

// translate from custom node graph to azuls internal model
fn translate_node_graph(ng: &MyNodeGraph, data: RefAny) -> azul::widgets::NodeGraph {
    azul::widgets::NodeGraph {
        node_types: ng.node_types.iter().map(|(k, v)| NodeTypeIdInfoMap {
            node_type_id: *k,
            node_type_info: v.clone(),
        }).collect::<Vec<_>>().into(),
        input_output_types: ng.input_output_types.iter().map(|(k, v)| InputOutputTypeIdInfoMap {
            io_type_id: k.clone(),
            io_info: v.clone(),
        }).collect::<Vec<_>>().into(),
        nodes: ng.nodes.iter().map(|(k, v)| NodeIdNodeMap {
            node_id: k.clone(),
            node: translate_node(v),
        }).collect::<Vec<_>>().into(),
        allow_multiple_root_nodes: false,
        offset: ng.offset,
        add_node_str: "Add Node".into(),
        callbacks: NodeGraphCallbacks {
            on_node_graph_dragged: Some(NodeGraphOnNodeGraphDragged {
                data: data.clone(),
                callback: NodeGraphOnNodeGraphDraggedCallback { cb: userfunc_on_node_graph_dragged }
            }).into(),
            on_node_dragged: Some(NodeGraphOnNodeDragged {
                data: data.clone(),
                callback: NodeGraphOnNodeDraggedCallback { cb: userfunc_on_node_dragged }
            }).into(),
            on_node_added: Some(NodeGraphOnNodeAdded {
                data: data.clone(),
                callback: NodeGraphOnNodeAddedCallback { cb: userfunc_on_node_added }
            }).into(),
            on_node_removed: Some(NodeGraphOnNodeRemoved {
                data: data.clone(),
                callback: NodeGraphOnNodeRemovedCallback { cb: userfunc_on_node_removed }
            }).into(),
            on_node_connected: Some(NodeGraphOnNodeConnected {
                data: data.clone(),
                callback: NodeGraphOnNodeConnectedCallback { cb: userfunc_on_node_connected }
            }).into(),
            on_node_input_disconnected: Some(NodeGraphOnNodeInputDisconnected {
                data: data.clone(),
                callback: NodeGraphOnNodeInputDisconnectedCallback { cb: userfunc_on_node_input_disconnected }
            }).into(),
            on_node_output_disconnected: Some(NodeGraphOnNodeOutputDisconnected {
                data: data.clone(),
                callback: NodeGraphOnNodeOutputDisconnectedCallback { cb: userfunc_on_node_output_disconnected }
            }).into(),
            on_node_field_edited: Some(NodeGraphOnNodeFieldEdited {
                data: data.clone(),
                callback: NodeGraphOnNodeFieldEditedCallback { cb: userfunc_on_node_field_edited }
            }).into()
        },
        style: NodeGraphStyle::Default,
    }
}

fn translate_node(n: &MyNode) -> azul::widgets::Node {
    azul::widgets::Node {
        node_type: n.node_type,
        position: n.position,
        connect_in: n.connect_in.iter().map(|(k, v)| InputConnection {
            input_index: k.clone(),
            connects_to: v.iter().map(|(k, v)| OutputNodeAndIndex {
                node_id: *k,
                output_index: *v,
            }).collect::<Vec<_>>().into()
        }).collect::<Vec<_>>().into(),
        connect_out: n.connect_out.iter().map(|(k, v)| OutputConnection {
            output_index: k.clone(),
            connects_to: v.iter().map(|(k, v)| InputNodeAndIndex {
                node_id: *k,
                input_index: *v,
            }).collect::<Vec<_>>().into()
        }).collect::<Vec<_>>().into(),
        fields: n.data.get_fields().into(),
    }
}

// editing functions
extern "C"
fn userfunc_on_node_graph_dragged(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    drag: GraphDragAmount
) -> Update {

    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.offset.x += drag.x;
    nodegraph.offset.y += drag.y;

    Update::DoNothing
}

extern "C"
fn userfunc_on_node_dragged(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    node_id: NodeGraphNodeId,
    drag: NodeDragAmount
) -> Update {

    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.nodes.get_mut(&node_id).unwrap().position.x += drag.x;
    nodegraph.nodes.get_mut(&node_id).unwrap().position.y += drag.y;

    Update::DoNothing
}

extern "C"
fn userfunc_on_node_added(
     data: &mut RefAny,
     info: &mut CallbackInfo,
     node_type: NodeTypeId,
     node_id: NodeGraphNodeId,
     initial_position: NodePosition,
) -> Update {

    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.nodes.insert(node_id, MyNode {
        node_type,
        position: initial_position,
        data: MyNodeType::create_default(node_type),
        connect_in: BTreeMap::new(),
        connect_out: BTreeMap::new(),
    });

    Update::RefreshDom
}

extern "C"
fn userfunc_on_node_removed(
     data: &mut RefAny,
     info: &mut CallbackInfo,
     node_id: NodeGraphNodeId,
) -> Update {
    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.nodes.remove(&node_id);

    Update::RefreshDom
}

extern "C"
fn userfunc_on_node_connected(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    input_node_id: NodeGraphNodeId,
    input_index: usize,
    output_node_id: NodeGraphNodeId,
    output_index: usize,
) -> Update {
    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.nodes.get_mut(&input_node_id).unwrap().connect_in
        .entry(input_index)
        .or_insert_with(|| BTreeMap::new())
        .insert(output_node_id, output_index);

    nodegraph.nodes.get_mut(&output_node_id).unwrap().connect_in
        .entry(output_index)
        .or_insert_with(|| BTreeMap::new())
        .insert(input_node_id, input_index);

    Update::RefreshDom
}

extern "C"
fn userfunc_on_node_input_disconnected(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    input_node_id: NodeGraphNodeId,
    input_index: usize,
) -> Update {
    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.nodes.get_mut(&input_node_id).unwrap().connect_in.remove(&input_index);

    Update::RefreshDom
}

extern "C"
fn userfunc_on_node_output_disconnected(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    output_node_id: NodeGraphNodeId,
    output_index: usize,
) -> Update {
    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.nodes.get_mut(&output_node_id).unwrap().connect_out.remove(&output_index);

    Update::RefreshDom
}

extern "C"
fn userfunc_on_node_field_edited(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    node_id: NodeGraphNodeId,
    field_idx: usize,
    node_type: NodeTypeId,
    new_field_value: NodeTypeFieldValue,
) -> Update {

    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.nodes.get_mut(&node_id).unwrap().data.edit_field(field_idx, node_type, new_field_value);

    Update::DoNothing
}

extern "C" fn layout_window(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
    let data_clone = data.clone();
    match data.downcast_ref::<MyNodeGraph>() {
        Some(s) => translate_node_graph(graph, data.clone()).dom().style(Css::empty()),
        None => StyledDom::default(),
    }
}

fn main() {
    let data = RefAny::new(MyNodeGraph::default());
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(layout_window));
}