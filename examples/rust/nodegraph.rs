#![windows_subsystem = "windows"]

use std::collections::BTreeMap;
use azul::prelude::*;
use azul::widgets::*;
use azul::prelude::String as AzString;
use std::string::String;

// Custom node graph data model
#[derive(Debug)]
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

        map.insert(NodeTypeId { inner: 1 }, NodeTypeInfo {
            is_root: false,
            name: "My Custom Node Type 2!".into(),
            inputs: vec![InputOutputTypeId { inner: 0 }, InputOutputTypeId { inner: 1 }].into(),
            outputs: vec![InputOutputTypeId { inner: 0 }, InputOutputTypeId { inner: 1 }].into(),
        });

        map
    }

    pub fn get_available_input_output_types() -> BTreeMap<InputOutputTypeId, InputOutputInfo> {
        let mut map = BTreeMap::new();

        map.insert(InputOutputTypeId { inner: 0 }, InputOutputInfo {
            data_type: "MyData".into(),
            color: ColorU::from_str("#ff0000"),
        });

        map.insert(InputOutputTypeId { inner: 1 }, InputOutputInfo {
            data_type: "MyOtherDataType".into(),
            color: ColorU::from_str("#00ff00"),
        });

        map
    }
}

#[derive(Debug)]
struct MyNode {
    node_type: NodeTypeId,
    position: NodePosition,
    connect_in: BTreeMap<usize, BTreeMap<NodeGraphNodeId, usize>>,
    connect_out: BTreeMap<usize, BTreeMap<NodeGraphNodeId, usize>>,
    data: MyNodeType,
}

impl MyNode {
    pub fn create_default(node_type: NodeTypeId) -> Self {
        Self {
            node_type,
            position: NodePosition { x: 0.0, y: 0.0 },
            connect_in: BTreeMap::new(),
            connect_out: BTreeMap::new(),
            data: MyNodeType::create_default(node_type),
        }
    }
}

// Custom node type
#[derive(Debug)]
enum MyNodeType {
    MyTypeVariant1 {
        textfield1: String,
        color1: ColorU,
        fileinput1: Option<String>,
    },
    MyTypeVariant2 {
        checkbox1: bool,
        numberinput1: f32,
        textfield2: String,
    }
}

impl MyNodeType {

    pub fn get_ids_and_classes(&self) -> NodeTypeId {
        match self {
            Self::MyTypeVariant1 { .. } => NodeTypeId { inner: 0 },
            Self::MyTypeVariant2 { .. } => NodeTypeId { inner: 1 },
        }
    }

    pub fn create_default(type_id: NodeTypeId) -> Self {
        match type_id.inner {
            0 => Self::MyTypeVariant1 {
                textfield1: String::new(),
                color1: ColorU { r: 0, g: 200, b: 0, a: 255 },
                fileinput1: None,
            },
            _ => Self::MyTypeVariant2 {
                checkbox1: false,
                numberinput1: 0.0,
                textfield2: String::new(),
            }
        }
    }

    pub fn get_fields(&self) -> Vec<NodeTypeField> {
        match self {
            Self::MyTypeVariant1 { textfield1, color1, fileinput1 } => {
                vec![
                    NodeTypeField {
                        key: "Text 1".into(),
                        value: NodeTypeFieldValue::TextInput(textfield1.clone().into()),
                    },
                    NodeTypeField {
                        key: "Color".into(),
                        value: NodeTypeFieldValue::ColorInput(color1.clone().into()),
                    },
                    NodeTypeField {
                        key: "File".into(),
                        value: NodeTypeFieldValue::FileInput(fileinput1.as_ref().map(|s| s.clone().into()).into()),
                    }
                ]
            },
            Self::MyTypeVariant2 { checkbox1, numberinput1, textfield2 } => {
                vec![
                    NodeTypeField {
                        key: "Check".into(),
                        value: NodeTypeFieldValue::CheckBox(*checkbox1),
                    },
                    NodeTypeField {
                        key: "Number".into(),
                        value: NodeTypeFieldValue::NumberInput(*numberinput1),
                    },
                    NodeTypeField {
                        key: "Text 2".into(),
                        value: NodeTypeFieldValue::TextInput(textfield2.clone().into()),
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
        match (node_type.inner, self) {
            (0, MyTypeVariant1 { textfield1, color1, fileinput1 }) => {
                match (field_idx, new_value) {
                    (0, NodeTypeFieldValue::TextInput(s)) => *textfield1 = s.as_str().into(),
                    (1, NodeTypeFieldValue::ColorInput(s)) => *color1 = s,
                    (2, NodeTypeFieldValue::FileInput(s)) => *fileinput1 = s.into_option().map(|s| s.as_str().to_string()),
                    _ => { },
                }
            },
            (1, MyTypeVariant2 { checkbox1, numberinput1, textfield2 }) => {
                match (field_idx, new_value) {
                    (0, NodeTypeFieldValue::CheckBox(s)) => *checkbox1 = s,
                    (1, NodeTypeFieldValue::NumberInput(s)) => *numberinput1 = s,
                    (2, NodeTypeFieldValue::TextInput(s)) => *textfield2 = s.as_str().into(),
                    _ => { },
                }
            },
            _ => { },
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
        scale_factor: 0.6,
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

    nodegraph.nodes.get_mut(&output_node_id).unwrap().connect_out
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

    let outputs = nodegraph.nodes
    .get_mut(&input_node_id)
    .unwrap()
    .connect_in
    .remove(&input_index)
    .unwrap_or_default();

    for (output_node_id, output_index) in outputs {
        nodegraph.nodes.get_mut(&output_node_id).unwrap().connect_out.remove(&output_index);
    }

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

    let inputs = nodegraph.nodes
    .get_mut(&output_node_id)
    .unwrap()
    .connect_out
    .remove(&output_index)
    .unwrap_or_default();

    for (input_node_id, input_index) in inputs {
        nodegraph.nodes.get_mut(&input_node_id).unwrap().connect_in.remove(&input_index);
    }

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
        Some(s) => translate_node_graph(&*s, data_clone).dom().style(Css::empty()),
        None => StyledDom::default(),
    }
}

fn main() {

    let mut node_graph = MyNodeGraph::default();
    node_graph.nodes.insert(
        NodeGraphNodeId { inner: 0 },
        MyNode::create_default(NodeTypeId { inner: 0 })
    );

    let data = RefAny::new(node_graph);
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(layout_window);
    window.state.flags.frame = WindowFrame::Maximized;
    app.run(window);
}