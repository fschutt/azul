//! Implementation of native menus on GNOME
//! 
//! TODO: needs fallback for non-GNOME WMs

use dbus::{
    arg::{PropMap, Variant},
    blocking::Connection,
    channel::MatchingReceiver,
    message::MatchRule,
    Path,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// First, let's define our menu structure
#[derive(Clone, Debug)]
struct MenuItem {
    id: u32,
    label: String,
    enabled: bool,
    children: Vec<MenuItem>,
    action: Option<String>,
    parameter: Option<Variant<Box<dyn dbus::arg::RefArg + 'static>>>,
}

// Menu manager to handle our menu structure and DBus interaction
struct MenuManager {
    app_name: String,
    conn: Connection,
    menus: Arc<Mutex<HashMap<u32, Vec<MenuItem>>>>,
    actions: Arc<Mutex<HashMap<String, Action>>>,
    object_path: String,
}

// A simple action definition
struct Action {
    name: String,
    enabled: bool,
    parameter_type: Option<String>,
    state: Option<Variant<Box<dyn dbus::arg::RefArg + 'static>>>,
    callback: Box<dyn Fn(Option<Variant<Box<dyn dbus::arg::RefArg + 'static>>>) + Send + Sync>,
}

impl MenuManager {
    fn new(app_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::new_session()?;
        
        // Register the application name on the bus
        // This is required for GNOME to find our menus
        let bus_name = format!("org.gtk.{}", app_name);
        conn.request_name(bus_name.clone(), false, true, false)?;
        
        let object_path = format!("/org/gtk/{}", app_name.replace('.', "/"));
        
        Ok(Self {
            app_name: app_name.to_string(),
            conn,
            menus: Arc::new(Mutex::new(HashMap::new())),
            actions: Arc::new(Mutex::new(HashMap::new())),
            object_path,
        })
    }
    
    // Add a menu group (subscription group in GNOME terms)
    fn add_menu_group(&self, group_id: u32, menu_items: Vec<MenuItem>) -> Result<(), Box<dyn std::error::Error>> {
        let mut menus = self.menus.lock().unwrap();
        menus.insert(group_id, menu_items);
        Ok(())
    }
    
    // Add an action
    fn add_action<F>(&self, name: &str, enabled: bool, callback: F) -> Result<(), Box<dyn std::error::Error>> 
    where 
        F: Fn(Option<Variant<Box<dyn dbus::arg::RefArg + 'static>>>) + Send + Sync + 'static,
    {
        let mut actions = self.actions.lock().unwrap();
        actions.insert(name.to_string(), Action {
            name: name.to_string(),
            enabled,
            parameter_type: None,
            state: None,
            callback: Box::new(callback),
        });
        Ok(())
    }
    
    // Start the DBus services
    fn start_services(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create a tree for DBus objects
        let factory = dbus::tree::Factory::new_fn::<()>();
        
        // Clone the data we need to share with the handlers
        let menus_data = self.menus.clone();
        let actions_data = self.actions.clone();
        
        // Implement org.gtk.Menus interface
        let menus_interface = factory
            .interface("org.gtk.Menus", ())
            .add_m(factory.method("Start", (), move |m| {
                let groups: Vec<u32> = m.msg.read1()?;
                let menus = menus_data.lock().unwrap();
                
                // Create response: array of (group_id, menu_id, items)
                let mut response = Vec::new();
                for group_id in groups {
                    if let Some(menu_items) = menus.get(&group_id) {
                        let items = Self::menu_items_to_dbus_format(menu_items);
                        response.push((group_id, 0u32, items)); // menu_id is 0 for simplicity
                    }
                }
                
                Ok(vec![m.msg.method_return().append1(response)])
            }))
            .add_m(factory.method("End", (), move |m| {
                // We don't need to do anything complex here for a simple implementation
                let _groups: Vec<u32> = m.msg.read1()?;
                Ok(vec![m.msg.method_return()])
            }));
        
        // Implement org.gtk.Actions interface
        let actions_interface = factory
            .interface("org.gtk.Actions", ())
            .add_m(factory.method("List", (), move |m| {
                let actions = actions_data.lock().unwrap();
                let action_names: Vec<&str> = actions.keys().map(|k| k.as_str()).collect();
                Ok(vec![m.msg.method_return().append1(action_names)])
            }))
            .add_m(factory.method("Describe", (), move |m| {
                let name: &str = m.msg.read1()?;
                let actions = actions_data.lock().unwrap();
                
                if let Some(action) = actions.get(name) {
                    // Format: (enabled, param_type, state)
                    let param_type = action.parameter_type.clone().unwrap_or_default();
                    let empty_state: Vec<Variant<Box<dyn dbus::arg::RefArg + 'static>>> = Vec::new();
                    let state = match &action.state {
                        Some(s) => vec![s.clone()],
                        None => empty_state,
                    };
                    
                    Ok(vec![m.msg.method_return().append3(action.enabled, param_type, state)])
                } else {
                    Err(dbus::Error::new_custom("org.gtk.Actions.Error", "Action not found"))
                }
            }))
            .add_m(factory.method("DescribeAll", (), move |m| {
                let actions = actions_data.lock().unwrap();
                
                let mut descriptions = HashMap::new();
                for (name, action) in actions.iter() {
                    let param_type = action.parameter_type.clone().unwrap_or_default();
                    let empty_state: Vec<Variant<Box<dyn dbus::arg::RefArg + 'static>>> = Vec::new();
                    let state = match &action.state {
                        Some(s) => vec![s.clone()],
                        None => empty_state,
                    };
                    
                    descriptions.insert(name.clone(), (action.enabled, param_type, state));
                }
                
                Ok(vec![m.msg.method_return().append1(descriptions)])
            }))
            .add_m(factory.method("Activate", (), move |m| {
                let actions = actions_data.lock().unwrap();
                let (name, param, _platform_data): (String, Vec<Variant<Box<dyn dbus::arg::RefArg + 'static>>>, PropMap) = m.msg.read3()?;
                
                if let Some(action) = actions.get(&name) {
                    let param = if param.is_empty() { None } else { Some(param[0].clone()) };
                    // Call the action callback
                    (action.callback)(param);
                    Ok(vec![m.msg.method_return()])
                } else {
                    Err(dbus::Error::new_custom("org.gtk.Actions.Error", "Action not found"))
                }
            }));
        
        // Build the DBus tree
        let tree = factory
            .tree(())
            .add(
                factory
                    .object_path(self.object_path.clone(), ())
                    .add(menus_interface)
                    .add(actions_interface),
            )
            .add(
                factory
                    .object_path(format!("{}/menus/MenuBar", self.object_path), ())
                    .add(menus_interface.clone()),
            )
            .add(
                factory
                    .object_path(format!("{}/menus/AppMenu", self.object_path), ())
                    .add(menus_interface.clone()),
            );
        
        // Register the tree with DBus
        tree.set_registered(&self.conn, true)?;
        
        // Create a handler to process DBus messages
        let c = self.conn.clone();
        std::thread::spawn(move || {
            c.start_receive(
                MatchRule::new_method_call(),
                Box::new(move |msg, conn| {
                    let _ = tree.handle_message(msg, conn);
                    true
                }),
            );
            
            loop {
                c.process(Duration::from_millis(1000))?;
            }
            
            #[allow(unreachable_code)]
            Ok::<(), dbus::Error>(())
        });
        
        Ok(())
    }
    
    // Set X11 window properties to tell GNOME where to find our menus
    fn set_x11_window_properties(&self, window_id: u64, xlib: &Rc<Xlib>, display: *mut Display) -> Result<(), Box<dyn std::error::Error>> {
        // We need to set several X11 properties for GNOME to find our menus
        
        // 1. Application ID
        let app_id_atom = unsafe {
            (xlib.XInternAtom)(
                display,
                encode_ascii("_GTK_APPLICATION_ID").as_ptr() as *const i8,
                0,
            )
        };
        
        let app_id = encode_ascii(&self.app_name);
        unsafe {
            XChangeProperty(
                display,
                window_id as c_ulong,
                app_id_atom,
                XA_STRING,
                8,
                PropModeReplace,
                app_id.as_ptr(),
                app_id.len() as c_int,
            );
        }
        
        // 2. Unique bus name
        let bus_name_atom = unsafe {
            (xlib.XInternAtom)(
                display,
                encode_ascii("_GTK_UNIQUE_BUS_NAME").as_ptr() as *const i8,
                0,
            )
        };
        
        let bus_name = encode_ascii(&format!("org.gtk.{}", self.app_name));
        unsafe {
            XChangeProperty(
                display,
                window_id as c_ulong,
                bus_name_atom,
                XA_STRING,
                8,
                PropModeReplace,
                bus_name.as_ptr(),
                bus_name.len() as c_int,
            );
        }
        
        // 3. Application object path
        let app_path_atom = unsafe {
            (xlib.XInternAtom)(
                display,
                encode_ascii("_GTK_APPLICATION_OBJECT_PATH").as_ptr() as *const i8,
                0,
            )
        };
        
        let app_path = encode_ascii(&self.object_path);
        unsafe {
            XChangeProperty(
                display,
                window_id as c_ulong,
                app_path_atom,
                XA_STRING,
                8,
                PropModeReplace,
                app_path.as_ptr(),
                app_path.len() as c_int,
            );
        }
        
        // 4. App menu object path
        let app_menu_atom = unsafe {
            (xlib.XInternAtom)(
                display,
                encode_ascii("_GTK_APP_MENU_OBJECT_PATH").as_ptr() as *const i8,
                0,
            )
        };
        
        let app_menu_path = encode_ascii(&format!("{}/menus/AppMenu", self.object_path));
        unsafe {
            XChangeProperty(
                display,
                window_id as c_ulong,
                app_menu_atom,
                XA_STRING,
                8,
                PropModeReplace,
                app_menu_path.as_ptr(),
                app_menu_path.len() as c_int,
            );
        }
        
        // 5. Menu bar object path
        let menubar_atom = unsafe {
            (xlib.XInternAtom)(
                display,
                encode_ascii("_GTK_MENUBAR_OBJECT_PATH").as_ptr() as *const i8,
                0,
            )
        };
        
        let menubar_path = encode_ascii(&format!("{}/menus/MenuBar", self.object_path));
        unsafe {
            XChangeProperty(
                display,
                window_id as c_ulong,
                menubar_atom,
                XA_STRING,
                8,
                PropModeReplace,
                menubar_path.as_ptr(),
                menubar_path.len() as c_int,
            );
        }
        
        Ok(())
    }
    
    // Helper function to convert our menu items to the format expected by org.gtk.Menus
    fn menu_items_to_dbus_format(items: &[MenuItem]) -> Vec<HashMap<String, Variant<Box<dyn dbus::arg::RefArg + 'static>>>> {
        let mut result = Vec::new();
        
        for item in items {
            let mut map = HashMap::new();
            
            // Add label
            map.insert("label".to_string(), Variant(Box::new(item.label.clone())));
            
            // Add action if present
            if let Some(action) = &item.action {
                map.insert("action".to_string(), Variant(Box::new(action.clone())));
                
                // Add parameter if present
                if let Some(param) = &item.parameter {
                    map.insert("target".to_string(), param.clone());
                }
            }
            
            // Handle submenu or section
            if !item.children.is_empty() {
                // For simplicity, we're using the same subscription group (0)
                // In a real app, you might want to organize differently
                map.insert(
                    "submenu".to_string(), 
                    Variant(Box::new((0u32, item.id)))
                );
                
                // We'll need to add the submenu separately
                // In a full implementation, you'd handle recursive adding
            }
            
            result.push(map);
        }
        
        result
    }
}

// You'll need to implement this function to match the one in the X11 module
// It encodes a string to ASCII and adds a null terminator
fn encode_ascii(input: &str) -> Vec<u8> {
    input
        .chars()
        .filter(|c| c.is_ascii())
        .map(|c| c as u8)
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>()
}

// Additional X11 functions we'll need (not included in the original code)
extern "C" {
    fn XChangeProperty(
        display: *mut Display,
        window: c_ulong,
        property: c_ulong,
        type_: c_ulong,
        format: c_int,
        mode: c_int,
        data: *const u8,
        nelements: c_int,
    ) -> c_int;
}

// X11 constants
const XA_STRING: c_ulong = 31;
const PropModeReplace: c_int = 0;

// Example usage
fn add_menu_to_window(window: &mut X11Window, app_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::rc::Rc;
    
    // Create the menu manager
    let menu_manager = MenuManager::new(app_name)?;
    
    // Define a simple menu structure
    let file_menu = MenuItem {
        id: 1,
        label: "File".to_string(),
        enabled: true,
        children: vec![
            MenuItem {
                id: 2,
                label: "New".to_string(),
                enabled: true,
                action: Some("app.new".to_string()),
                parameter: None,
                children: vec![],
            },
            MenuItem {
                id: 3,
                label: "Open".to_string(),
                enabled: true,
                action: Some("app.open".to_string()),
                parameter: None,
                children: vec![],
            },
            MenuItem {
                id: 4,
                label: "Save".to_string(),
                enabled: true,
                action: Some("app.save".to_string()),
                parameter: None,
                children: vec![],
            },
            MenuItem {
                id: 5,
                label: "Quit".to_string(),
                enabled: true,
                action: Some("app.quit".to_string()),
                parameter: None,
                children: vec![],
            },
        ],
    };
    
    let edit_menu = MenuItem {
        id: 6,
        label: "Edit".to_string(),
        enabled: true,
        children: vec![
            MenuItem {
                id: 7,
                label: "Cut".to_string(),
                enabled: true,
                action: Some("app.cut".to_string()),
                parameter: None,
                children: vec![],
            },
            MenuItem {
                id: 8,
                label: "Copy".to_string(),
                enabled: true,
                action: Some("app.copy".to_string()),
                parameter: None,
                children: vec![],
            },
            MenuItem {
                id: 9,
                label: "Paste".to_string(),
                enabled: true,
                action: Some("app.paste".to_string()),
                parameter: None,
                children: vec![],
            },
        ],
    };
    
    // Add the main menu to group 0
    menu_manager.add_menu_group(0, vec![file_menu.clone(), edit_menu.clone()])?;
    
    // Add file submenu to group 0
    menu_manager.add_menu_group(0, vec![file_menu])?;
    
    // Add edit submenu to group 0
    menu_manager.add_menu_group(0, vec![edit_menu])?;
    
    // Add menu actions with callbacks
    menu_manager.add_action("app.new", true, |_| {
        println!("New action triggered");
    })?;
    
    menu_manager.add_action("app.open", true, |_| {
        println!("Open action triggered");
    })?;
    
    menu_manager.add_action("app.save", true, |_| {
        println!("Save action triggered");
    })?;
    
    menu_manager.add_action("app.quit", true, |_| {
        println!("Quit action triggered");
        std::process::exit(0);
    })?;
    
    menu_manager.add_action("app.cut", true, |_| {
        println!("Cut action triggered");
    })?;
    
    menu_manager.add_action("app.copy", true, |_| {
        println!("Copy action triggered");
    })?;
    
    menu_manager.add_action("app.paste", true, |_| {
        println!("Paste action triggered");
    })?;
    
    // Start the DBus services
    menu_manager.start_services()?;
    
    // Set the X11 window properties to tell GNOME where to find the menus
    menu_manager.set_x11_window_properties(window.id, &window.xlib, window.dpy.get())?;
    
    Ok(())
}