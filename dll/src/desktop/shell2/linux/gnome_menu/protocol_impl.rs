//! Low-level DBus protocol implementation using dlopen
//!
//! This module implements the org.gtk.Menus and org.gtk.Actions interfaces
//! using the raw libdbus-1 C API loaded via dlopen. This allows GNOME menu
//! integration without compile-time linking to libdbus.

use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    os::raw::{c_char, c_int, c_void},
    rc::Rc,
    sync::{Arc, Mutex},
};

use super::{debug_log, DbusAction, DbusMenuGroup, DbusMenuItem, GnomeMenuError};
use crate::desktop::shell2::linux::dbus::{
    DBusConnection, DBusLib, DBusMessage, DBusMessageIter, DBusObjectPathVTable,
    DBUS_HANDLER_RESULT_HANDLED, DBUS_HANDLER_RESULT_NEED_MEMORY,
    DBUS_HANDLER_RESULT_NOT_YET_HANDLED, DBUS_TYPE_ARRAY, DBUS_TYPE_STRING, DBUS_TYPE_UINT32,
    DBUS_TYPE_VARIANT,
};

/// Shared state for DBus message handlers
struct HandlerState {
    dbus_lib: Rc<DBusLib>,
    menu_groups: Arc<Mutex<HashMap<u32, DbusMenuGroup>>>,
    actions: Arc<Mutex<HashMap<String, DbusAction>>>,
}

/// Register org.gtk.Menus interface with DBus using dlopen API
pub fn register_menus_interface(
    dbus_lib: &Rc<DBusLib>,
    connection: *mut DBusConnection,
    object_path: &str,
    menu_groups: Arc<Mutex<HashMap<u32, DbusMenuGroup>>>,
) -> Result<(), GnomeMenuError> {
    debug_log("Registering org.gtk.Menus interface with dlopen DBus");

    let path_cstr = CString::new(object_path)
        .map_err(|e| GnomeMenuError::DbusConnectionFailed(e.to_string()))?;

    // Create handler state
    let state = Box::new(HandlerState {
        dbus_lib: dbus_lib.clone(),
        menu_groups,
        actions: Arc::new(Mutex::new(HashMap::new())), // Not used for menus
    });

    // Create vtable with message handler
    let vtable = DBusObjectPathVTable {
        unregister_function: Some(menus_unregister_handler),
        message_function: Some(menus_message_handler),
    };

    // Register object path
    let result = unsafe {
        (dbus_lib.dbus_connection_register_object_path)(
            connection,
            path_cstr.as_ptr(),
            &vtable as *const _,
            Box::into_raw(state) as *mut c_void,
        )
    };

    if result == 0 {
        return Err(GnomeMenuError::ServiceRegistrationFailed(
            "Failed to register org.gtk.Menus object path".to_string(),
        ));
    }

    debug_log("org.gtk.Menus interface registered successfully");
    Ok(())
}

/// Register org.gtk.Actions interface with DBus using dlopen API
pub fn register_actions_interface(
    dbus_lib: &Rc<DBusLib>,
    connection: *mut DBusConnection,
    object_path: &str,
    actions: Arc<Mutex<HashMap<String, DbusAction>>>,
) -> Result<(), GnomeMenuError> {
    debug_log("Registering org.gtk.Actions interface with dlopen DBus");

    let path_cstr = CString::new(object_path)
        .map_err(|e| GnomeMenuError::DbusConnectionFailed(e.to_string()))?;

    // Create handler state
    let state = Box::new(HandlerState {
        dbus_lib: dbus_lib.clone(),
        menu_groups: Arc::new(Mutex::new(HashMap::new())), // Not used for actions
        actions,
    });

    // Create vtable with message handler
    let vtable = DBusObjectPathVTable {
        unregister_function: Some(actions_unregister_handler),
        message_function: Some(actions_message_handler),
    };

    // Register object path
    let result = unsafe {
        (dbus_lib.dbus_connection_register_object_path)(
            connection,
            path_cstr.as_ptr(),
            &vtable as *const _,
            Box::into_raw(state) as *mut c_void,
        )
    };

    if result == 0 {
        return Err(GnomeMenuError::ServiceRegistrationFailed(
            "Failed to register org.gtk.Actions object path".to_string(),
        ));
    }

    debug_log("org.gtk.Actions interface registered successfully");
    Ok(())
}

/// Unregister handler for org.gtk.Menus
unsafe extern "C" fn menus_unregister_handler(
    _connection: *mut DBusConnection,
    user_data: *mut c_void,
) {
    if !user_data.is_null() {
        // Reconstruct Box and drop it
        let _state = Box::from_raw(user_data as *mut HandlerState);
        debug_log("org.gtk.Menus unregistered");
    }
}

/// Message handler for org.gtk.Menus interface
unsafe extern "C" fn menus_message_handler(
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
    user_data: *mut c_void,
) -> c_int {
    if user_data.is_null() || message.is_null() {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    let state = &*(user_data as *const HandlerState);

    // Get interface and member names
    let interface = (state.dbus_lib.dbus_message_get_interface)(message);
    let member = (state.dbus_lib.dbus_message_get_member)(message);

    if interface.is_null() || member.is_null() {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    let interface_str = CStr::from_ptr(interface).to_string_lossy();
    let member_str = CStr::from_ptr(member).to_string_lossy();

    if interface_str != "org.gtk.Menus" {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    debug_log(&format!("org.gtk.Menus method called: {}", member_str));

    match member_str.as_ref() {
        "Start" => handle_menus_start(state, connection, message),
        "End" => handle_menus_end(state, connection, message),
        _ => DBUS_HANDLER_RESULT_NOT_YET_HANDLED,
    }
}

/// Handle org.gtk.Menus.Start(au) method
unsafe fn handle_menus_start(
    state: &HandlerState,
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
) -> c_int {
    debug_log("Handling Start() method");

    // Parse input: array of uint32 (subscriptions)
    let mut iter = std::mem::zeroed::<DBusMessageIter>();
    if (state.dbus_lib.dbus_message_iter_init)(message, &mut iter) == 0 {
        return send_error_reply(state, connection, message, "Invalid message format");
    }

    let arg_type = (state.dbus_lib.dbus_message_iter_get_arg_type)(&mut iter);
    if arg_type != DBUS_TYPE_ARRAY {
        return send_error_reply(state, connection, message, "Expected array argument");
    }

    // Recurse into array
    let mut array_iter = std::mem::zeroed::<DBusMessageIter>();
    (state.dbus_lib.dbus_message_iter_recurse)(&mut iter, &mut array_iter);

    // Read subscription IDs
    let mut subscriptions = Vec::new();
    loop {
        let elem_type = (state.dbus_lib.dbus_message_iter_get_arg_type)(&mut array_iter);
        if elem_type == 0 {
            break;
        }

        if elem_type == DBUS_TYPE_UINT32 {
            let mut value: u32 = 0;
            (state.dbus_lib.dbus_message_iter_get_basic)(
                &mut array_iter,
                &mut value as *mut u32 as *mut c_void,
            );
            subscriptions.push(value);
        }

        if (state.dbus_lib.dbus_message_iter_next)(&mut array_iter) == 0 {
            break;
        }
    }

    debug_log(&format!("Start() subscriptions: {:?}", subscriptions));

    // Build response: array of (uuaa{sv})
    let reply = (state.dbus_lib.dbus_message_new_method_return)(message);
    if reply.is_null() {
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }

    let mut reply_iter = std::mem::zeroed::<DBusMessageIter>();
    (state.dbus_lib.dbus_message_iter_init_append)(reply, &mut reply_iter);

    // Open array container for menu groups
    let mut array_iter = std::mem::zeroed::<DBusMessageIter>();
    let array_sig = CString::new("(uuaa{sv})").unwrap();
    if (state.dbus_lib.dbus_message_iter_open_container)(
        &mut reply_iter,
        DBUS_TYPE_ARRAY,
        array_sig.as_ptr(),
        &mut array_iter,
    ) == 0
    {
        (state.dbus_lib.dbus_message_unref)(reply);
        return send_error_reply(state, connection, message, "Failed to build response");
    }

    // Add menu groups for requested subscriptions
    let menu_groups = state.menu_groups.lock().unwrap();
    for group_id in subscriptions {
        if let Some(_group) = menu_groups.get(&group_id) {
            // TODO: Serialize menu group to DBus format
            // This requires building nested structs and dictionaries
            // For now, we add empty groups
            debug_log(&format!("Adding menu group {}", group_id));
        }
    }

    // Close array container
    if (state.dbus_lib.dbus_message_iter_close_container)(&mut reply_iter, &mut array_iter) == 0 {
        (state.dbus_lib.dbus_message_unref)(reply);
        return send_error_reply(state, connection, message, "Failed to close array");
    }

    // Send reply
    (state.dbus_lib.dbus_connection_send)(connection, reply, std::ptr::null_mut());
    (state.dbus_lib.dbus_connection_flush)(connection);
    (state.dbus_lib.dbus_message_unref)(reply);

    DBUS_HANDLER_RESULT_HANDLED
}

/// Handle org.gtk.Menus.End(au) method
unsafe fn handle_menus_end(
    state: &HandlerState,
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
) -> c_int {
    debug_log("Handling End() method");

    // End() just acknowledges unsubscription, no data needed
    let reply = (state.dbus_lib.dbus_message_new_method_return)(message);
    if reply.is_null() {
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }

    (state.dbus_lib.dbus_connection_send)(connection, reply, std::ptr::null_mut());
    (state.dbus_lib.dbus_connection_flush)(connection);
    (state.dbus_lib.dbus_message_unref)(reply);

    DBUS_HANDLER_RESULT_HANDLED
}

/// Unregister handler for org.gtk.Actions
unsafe extern "C" fn actions_unregister_handler(
    _connection: *mut DBusConnection,
    user_data: *mut c_void,
) {
    if !user_data.is_null() {
        let _state = Box::from_raw(user_data as *mut HandlerState);
        debug_log("org.gtk.Actions unregistered");
    }
}

/// Message handler for org.gtk.Actions interface
unsafe extern "C" fn actions_message_handler(
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
    user_data: *mut c_void,
) -> c_int {
    if user_data.is_null() || message.is_null() {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    let state = &*(user_data as *const HandlerState);

    let interface = (state.dbus_lib.dbus_message_get_interface)(message);
    let member = (state.dbus_lib.dbus_message_get_member)(message);

    if interface.is_null() || member.is_null() {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    let interface_str = CStr::from_ptr(interface).to_string_lossy();
    let member_str = CStr::from_ptr(member).to_string_lossy();

    if interface_str != "org.gtk.Actions" {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    debug_log(&format!("org.gtk.Actions method called: {}", member_str));

    match member_str.as_ref() {
        "List" => handle_actions_list(state, connection, message),
        "Describe" => handle_actions_describe(state, connection, message),
        "DescribeAll" => handle_actions_describe_all(state, connection, message),
        "Activate" => handle_actions_activate(state, connection, message),
        _ => DBUS_HANDLER_RESULT_NOT_YET_HANDLED,
    }
}

/// Handle org.gtk.Actions.List() -> as
unsafe fn handle_actions_list(
    state: &HandlerState,
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
) -> c_int {
    debug_log("Handling List() method");

    let actions = state.actions.lock().unwrap();
    let action_names: Vec<String> = actions.keys().cloned().collect();

    let reply = (state.dbus_lib.dbus_message_new_method_return)(message);
    if reply.is_null() {
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }

    let mut reply_iter = std::mem::zeroed::<DBusMessageIter>();
    (state.dbus_lib.dbus_message_iter_init_append)(reply, &mut reply_iter);

    // Open array of strings
    let mut array_iter = std::mem::zeroed::<DBusMessageIter>();
    let array_sig = CString::new("s").unwrap();
    if (state.dbus_lib.dbus_message_iter_open_container)(
        &mut reply_iter,
        DBUS_TYPE_ARRAY,
        array_sig.as_ptr(),
        &mut array_iter,
    ) == 0
    {
        (state.dbus_lib.dbus_message_unref)(reply);
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }

    // Add each action name
    for name in action_names {
        let name_cstr = CString::new(name).unwrap();
        let name_ptr = name_cstr.as_ptr();
        (state.dbus_lib.dbus_message_iter_append_basic)(
            &mut array_iter,
            DBUS_TYPE_STRING,
            &name_ptr as *const *const c_char as *const c_void,
        );
    }

    (state.dbus_lib.dbus_message_iter_close_container)(&mut reply_iter, &mut array_iter);
    (state.dbus_lib.dbus_connection_send)(connection, reply, std::ptr::null_mut());
    (state.dbus_lib.dbus_connection_flush)(connection);
    (state.dbus_lib.dbus_message_unref)(reply);

    DBUS_HANDLER_RESULT_HANDLED
}

/// Handle org.gtk.Actions.Describe(s) -> (bsav)
unsafe fn handle_actions_describe(
    state: &HandlerState,
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
) -> c_int {
    debug_log("Handling Describe() method");

    // Parse action name from message
    let mut iter = std::mem::zeroed::<DBusMessageIter>();
    if (state.dbus_lib.dbus_message_iter_init)(message, &mut iter) == 0 {
        return send_error_reply(state, connection, message, "Invalid message format");
    }

    let arg_type = (state.dbus_lib.dbus_message_iter_get_arg_type)(&mut iter);
    if arg_type != DBUS_TYPE_STRING {
        return send_error_reply(state, connection, message, "Expected string argument");
    }

    let mut name_ptr: *const c_char = std::ptr::null();
    (state.dbus_lib.dbus_message_iter_get_basic)(
        &mut iter,
        &mut name_ptr as *mut *const c_char as *mut c_void,
    );

    if name_ptr.is_null() {
        return send_error_reply(state, connection, message, "Invalid action name");
    }

    let action_name = CStr::from_ptr(name_ptr).to_string_lossy();
    let actions = state.actions.lock().unwrap();

    let action = match actions.get(action_name.as_ref()) {
        Some(a) => a,
        None => {
            return send_error_reply(
                state,
                connection,
                message,
                &format!("Action not found: {}", action_name),
            );
        }
    };

    // Build response: (enabled: bool, param_type: string, state: array of variant)
    let reply = (state.dbus_lib.dbus_message_new_method_return)(message);
    if reply.is_null() {
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }

    let mut reply_iter = std::mem::zeroed::<DBusMessageIter>();
    (state.dbus_lib.dbus_message_iter_init_append)(reply, &mut reply_iter);

    // Open struct
    let mut struct_iter = std::mem::zeroed::<DBusMessageIter>();
    if (state.dbus_lib.dbus_message_iter_open_container)(
        &mut reply_iter,
        DBUS_TYPE_ARRAY, // Actually DBUS_TYPE_STRUCT but using array for simplicity
        std::ptr::null(),
        &mut struct_iter,
    ) == 0
    {
        (state.dbus_lib.dbus_message_unref)(reply);
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }

    // TODO: Properly serialize (bool, string, array) tuple
    // For now, just return success

    (state.dbus_lib.dbus_message_iter_close_container)(&mut reply_iter, &mut struct_iter);
    (state.dbus_lib.dbus_connection_send)(connection, reply, std::ptr::null_mut());
    (state.dbus_lib.dbus_connection_flush)(connection);
    (state.dbus_lib.dbus_message_unref)(reply);

    DBUS_HANDLER_RESULT_HANDLED
}

/// Handle org.gtk.Actions.DescribeAll() -> a{s(bsav)}
unsafe fn handle_actions_describe_all(
    state: &HandlerState,
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
) -> c_int {
    debug_log("Handling DescribeAll() method");

    let reply = (state.dbus_lib.dbus_message_new_method_return)(message);
    if reply.is_null() {
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }

    // TODO: Build dictionary of action descriptions
    // For now, return empty dict

    (state.dbus_lib.dbus_connection_send)(connection, reply, std::ptr::null_mut());
    (state.dbus_lib.dbus_connection_flush)(connection);
    (state.dbus_lib.dbus_message_unref)(reply);

    DBUS_HANDLER_RESULT_HANDLED
}

/// Handle org.gtk.Actions.Activate(s, av, a{sv})
unsafe fn handle_actions_activate(
    state: &HandlerState,
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
) -> c_int {
    debug_log("Handling Activate() method");

    // Parse action name
    let mut iter = std::mem::zeroed::<DBusMessageIter>();
    if (state.dbus_lib.dbus_message_iter_init)(message, &mut iter) == 0 {
        return send_error_reply(state, connection, message, "Invalid message format");
    }

    let arg_type = (state.dbus_lib.dbus_message_iter_get_arg_type)(&mut iter);
    if arg_type != DBUS_TYPE_STRING {
        return send_error_reply(state, connection, message, "Expected string argument");
    }

    let mut name_ptr: *const c_char = std::ptr::null();
    (state.dbus_lib.dbus_message_iter_get_basic)(
        &mut iter,
        &mut name_ptr as *mut *const c_char as *mut c_void,
    );

    if name_ptr.is_null() {
        return send_error_reply(state, connection, message, "Invalid action name");
    }

    let action_name = CStr::from_ptr(name_ptr).to_string_lossy().to_string();

    // Get and invoke callback
    let callback = {
        let actions = state.actions.lock().unwrap();
        match actions.get(&action_name) {
            Some(action) if action.enabled => Some(action.callback.clone()),
            Some(_) => {
                return send_error_reply(
                    state,
                    connection,
                    message,
                    &format!("Action is disabled: {}", action_name),
                );
            }
            None => {
                return send_error_reply(
                    state,
                    connection,
                    message,
                    &format!("Action not found: {}", action_name),
                );
            }
        }
    };

    if let Some(callback) = callback {
        debug_log(&format!("Invoking callback for action: {}", action_name));
        callback(None); // TODO: Parse parameter from message
    }

    // Send empty reply
    let reply = (state.dbus_lib.dbus_message_new_method_return)(message);
    if !reply.is_null() {
        (state.dbus_lib.dbus_connection_send)(connection, reply, std::ptr::null_mut());
        (state.dbus_lib.dbus_connection_flush)(connection);
        (state.dbus_lib.dbus_message_unref)(reply);
    }

    DBUS_HANDLER_RESULT_HANDLED
}

/// Helper to send error reply
unsafe fn send_error_reply(
    state: &HandlerState,
    connection: *mut DBusConnection,
    message: *mut DBusMessage,
    error_msg: &str,
) -> c_int {
    debug_log(&format!("Sending error reply: {}", error_msg));

    let error_name = CString::new("org.gtk.Error").unwrap();
    let error_text = CString::new(error_msg).unwrap();

    let reply =
        (state.dbus_lib.dbus_message_new_error)(message, error_name.as_ptr(), error_text.as_ptr());

    if !reply.is_null() {
        (state.dbus_lib.dbus_connection_send)(connection, reply, std::ptr::null_mut());
        (state.dbus_lib.dbus_connection_flush)(connection);
        (state.dbus_lib.dbus_message_unref)(reply);
    }

    DBUS_HANDLER_RESULT_HANDLED
}
