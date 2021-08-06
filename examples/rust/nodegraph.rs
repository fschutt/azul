use azul::prelude::*;

// custom node types
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
        vec![
            NodeTypeField {
                key: "Field 1".into(),
                value: TextInput(self.field1.clone().into()),
            },
            NodeTypeField {
                key: "Field 2".into(),
                value: TextInput(self.field2.clone().into()),
            }
        ]
    }

    pub fn edit_field(
        &mut self,
        field_idx: usize,
        node_type: NodeTypeId,
        new_value: NodeTypeFieldValue
    ) {
        match (self, node_type) {
            (MyTypeVariant { field1, field2 }, NodeTypeId { inner: 0 }) => {
                match (field_idx, new_value) {
                    (0, TextInput(s)) => *field1 = s,
                    (1, TextInput(s)) => *field2 = s,
                    _ => { },
                }
            },
            _ => { }
        }

    }
}

#[derive(Default)]
struct MyNodeGraph {
    node_types: BTreeMap<NodeTypeId, NodeTypeInfo>,
    input_output_types: BTreeMap<InputOutputTypeId, InputOutputInfo>,
    nodes: BTreeMap<NodeGraphNodeId, MyNode>,
    offset: LogicalPosition,
}

// translate from custom node graph to azuls internal model
fn translate_node_graph(ng: &NodeGraph) -> azul::widgets::NodeGraph {
    azul::widgets::NodeGraph {
        node_types: ng.node_types.clone(),
        input_output_types: ng.input_output_types.clone(),
        nodes: ng.nodes.iter().map(|(k, v)| (*k, v.into())).collect(),
        multiple_root_nodes: false,
        offset_x: ng.offset.x,
        offset_y: ng.offset.y,
    }
}

struct MyNode {
    node_type: NodeTypeId,
    position: NodePosition,
    connect_in: BTreeMap<usize, BTreeMap<NodeGraphNodeId, usize>>,
    connect_out: BTreeMap<usize, BTreeMap<NodeGraphNodeId, usize>>,
    data: MyNodeType,
}

fn translate_node(n: &MyNode) -> azul::widgets::Node {
    azul::widgets::Node {
        node_type: t.node_type,
        position: t.position,
        connect_in: t.connect_in.clone(),
        connect_out: t.connect_out.clone(),
        fields: node.data.get_fields(),
    }
}

// editing functions
extern "C"
fn userfunc_on_graph_dragged(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    drag: GraphDragAmount
) -> Update {

    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    nodegraph.offset_x += drag.x;
    nodegraph.offset_y += drag.y;

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

    node_graph.nodes[node_id].position.x += drag.x;
    node_graph.nodes[node_id].position.y += drag.y;

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

    node_graph.nodes.insert(node_id, MyNode {
        node_type,
        position: initial_position,
        data: MyNodeType::create_default(node_type),
        connect_in: BTreeMap::new(),
        connect_out: BTreeMap::new(),
    });

    Update::RefreshDom
}

extern "C"
fn userfunc_on_node_added(
     data: &mut RefAny,
     info: &mut CallbackInfo,
     node_id: NodeGraphNodeId,
) -> Update {
    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    node_graph.nodes.remove(node_id);

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

    node_graph.nodes[input_node_id].connect_in
        .entry(input_index)
        .or_insert_with(|| BTreeMap::new())
        .insert(output_node_id, output_index);

    node_graph.nodes[output_node_id].connect_in
        .entry(output_index)
        .or_insert_with(|| BTreeMap::new())
        .insert(input_node_id, input_index);

    Update::RefreshDom
}

pub extern "C" fn userfunc_on_node_input_disconnected(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    input_node_id: NodeGraphNodeId,
    input_index: usize,
) -> Update {
    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    node_graph.nodes[input_node_id].connect_in.remove(input_index);

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

    node_graph.nodes[output_node_id].connect_out.remove(output_index);

    Update::RefreshDom
}

extern "C"
fn userfunc_on_node_field_edited(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    node_id: NodeGraphNodeId,
    node_type: NodeTypeId,
    field_idx: usize,
    new_field_value: NodeTypeFieldValue,
) -> Update {
    let mut nodegraph = match data.downcast_mut::<MyNodeGraph>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    node_graph.nodes[node_id].data.edit_field(node_type, field_idx, new_field_value);

    Update::DoNothing
}

fn render_node_graph_dom(graph: &MyNodeGraph, data: RefAny /* RefAny<MyNodeGraph> */) -> Dom {
    translate_node_graph(graph)
    .with_callbacks(NodeGraphCallbacks {
        on_node_graph_dragged: Some(userfunc_on_node_graph_dragged, data.clone()).into(),
        on_node_dragged: Some(userfunc_on_node_dragged, data.clone()).into(),
        on_node_added: Some(userfunc_on_node_added, data.clone()).into(),
        on_node_removed: Some(userfunc_on_node_removed, data.clone()).into(),
        on_node_connected: Some(userfunc_on_node_connected, data.clone()).into(),
        on_node_input_disconnected: Some(userfunc_on_node_input_disconnected, data.clone()).into(),
        on_node_output_disconnected: Some(userfunc_on_node_output_disconnected, data.clone()).into(),
        on_node_field_edited: Some(userfunc_on_node_field_edited, data).into(),
    })
    .dom()
}

extern "C" fn layout_window(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let data_clone = data.clone();
    match data.downcast_ref::<MyNodeGraph>() {
        Some(s) => render_node_graph_dom(s, data_clone).style(Css::default()),
        None => StyledDom::default(),
    }
}

fn main() {
    let data = RefAny::new(NodeGraph::default());
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(layout_window));
}