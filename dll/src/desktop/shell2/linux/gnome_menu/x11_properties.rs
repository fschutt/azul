//! X11 Window Properties
//!
//! Sets X11 window properties to advertise DBus menu services to GNOME Shell.
//!
//! ## Properties Set
//!
//! - `_GTK_APPLICATION_ID` - Application identifier
//! - `_GTK_UNIQUE_BUS_NAME` - DBus service name
//! - `_GTK_APPLICATION_OBJECT_PATH` - DBus object path
//! - `_GTK_APP_MENU_OBJECT_PATH` - App menu path (GNOME 3.x)
//! - `_GTK_MENUBAR_OBJECT_PATH` - Menu bar path

use std::ffi::CString;
use super::{GnomeMenuError, debug_log};

/// X11 property utilities
pub struct X11Properties;

impl X11Properties {
    /// Set all required X11 window properties for GNOME menu integration
    pub fn set_properties(
        window_id: u64,
        display: *mut std::ffi::c_void,
        app_name: &str,
        bus_name: &str,
        object_path: &str,
    ) -> Result<(), GnomeMenuError> {
        debug_log("Setting X11 window properties");

        #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
        {
            use crate::desktop::shell2::linux::x11::dlopen::Xlib;
            
            let xlib = Xlib::new()
                .map_err(|e| GnomeMenuError::X11PropertyFailed(format!("Failed to load Xlib: {:?}", e)))?;

            // 1. _GTK_APPLICATION_ID
            Self::set_property(
                &xlib,
                display,
                window_id,
                "_GTK_APPLICATION_ID",
                app_name.as_bytes(),
            )?;

            // 2. _GTK_UNIQUE_BUS_NAME
            Self::set_property(
                &xlib,
                display,
                window_id,
                "_GTK_UNIQUE_BUS_NAME",
                bus_name.as_bytes(),
            )?;

            // 3. _GTK_APPLICATION_OBJECT_PATH
            Self::set_property(
                &xlib,
                display,
                window_id,
                "_GTK_APPLICATION_OBJECT_PATH",
                object_path.as_bytes(),
            )?;

            // 4. _GTK_APP_MENU_OBJECT_PATH
            let app_menu_path = format!("{}/menus/AppMenu", object_path);
            Self::set_property(
                &xlib,
                display,
                window_id,
                "_GTK_APP_MENU_OBJECT_PATH",
                app_menu_path.as_bytes(),
            )?;

            // 5. _GTK_MENUBAR_OBJECT_PATH
            let menubar_path = format!("{}/menus/MenuBar", object_path);
            Self::set_property(
                &xlib,
                display,
                window_id,
                "_GTK_MENUBAR_OBJECT_PATH",
                menubar_path.as_bytes(),
            )?;

            debug_log("All X11 properties set successfully");
            Ok(())
        }
        
        #[cfg(not(all(target_os = "linux", feature = "gnome-menus")))]
        Err(GnomeMenuError::NotImplemented)
    }

    /// Set a single X11 window property
    #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
    fn set_property(
        xlib: &std::rc::Rc<crate::desktop::shell2::linux::x11::dlopen::Xlib>,
        display: *mut std::ffi::c_void,
        window_id: u64,
        property_name: &str,
        value: &[u8],
    ) -> Result<(), GnomeMenuError> {
        use std::os::raw::{c_int, c_ulong};
        
        // Intern the atom for this property
        let property_cstr = CString::new(property_name)
            .map_err(|e| GnomeMenuError::X11PropertyFailed(e.to_string()))?;
        
        let atom = unsafe {
            (xlib.XInternAtom)(
                display as *mut _,
                property_cstr.as_ptr(),
                0,
            )
        };

        if atom == 0 {
            return Err(GnomeMenuError::X11PropertyFailed(
                format!("Failed to intern atom: {}", property_name)
            ));
        }

        // Property type atom (STRING)
        const XA_STRING: c_ulong = 31;
        const PROP_MODE_REPLACE: c_int = 0;
        
        // Set the property
        unsafe {
            (xlib.XChangeProperty)(
                display as *mut _,
                window_id as c_ulong,
                atom,
                XA_STRING,
                8, // 8-bit format (char)
                PROP_MODE_REPLACE,
                value.as_ptr(),
                value.len() as c_int,
            );
        }

        debug_log(&format!("Set property: {} = {:?}", property_name, 
            String::from_utf8_lossy(value)));
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(all(target_os = "linux", feature = "gnome-menus")))]
    fn test_set_properties_returns_not_implemented() {
        // Until X11 integration is complete, this should return NotImplemented on non-Linux
        let result = X11Properties::set_properties(
            123,
            std::ptr::null_mut(),
            "test.app",
            "org.gtk.test_app",
            "/org/gtk/test/app",
        );
        
        assert!(result.is_err());
        match result.unwrap_err() {
            GnomeMenuError::NotImplemented => {},
            _ => panic!("Expected NotImplemented error"),
        }
    }
}

```

use std::ffi::CString;
use super::{GnomeMenuError, debug_log};

/// X11 property utilities
pub struct X11Properties;

impl X11Properties {
    /// Set all required X11 window properties for GNOME menu integration
    pub fn set_properties(
        window_id: u64,
        display: *mut std::ffi::c_void,
        app_name: &str,
        bus_name: &str,
        object_path: &str,
    ) -> Result<(), GnomeMenuError> {
        debug_log("Setting X11 window properties");

        // TODO: Implement actual X11 property setting when Xlib access is available
        // For now, return NotImplemented to trigger fallback
        Err(GnomeMenuError::NotImplemented)

        // Future implementation:
        /*
        use crate::desktop::shell2::linux::x11::dlopen::Xlib;
        
        let xlib = Xlib::new()
            .map_err(|e| GnomeMenuError::X11PropertyFailed(format!("Failed to load Xlib: {:?}", e)))?;

        let display = display as *mut Display;

        // Property type atom (STRING)
        const XA_STRING: u64 = 31;
        const PROP_MODE_REPLACE: i32 = 0;

        // 1. _GTK_APPLICATION_ID
        Self::set_property(
            &xlib,
            display,
            window_id,
            "_GTK_APPLICATION_ID",
            app_name.as_bytes(),
        )?;

        // 2. _GTK_UNIQUE_BUS_NAME
        Self::set_property(
            &xlib,
            display,
            window_id,
            "_GTK_UNIQUE_BUS_NAME",
            bus_name.as_bytes(),
        )?;

        // 3. _GTK_APPLICATION_OBJECT_PATH
        Self::set_property(
            &xlib,
            display,
            window_id,
            "_GTK_APPLICATION_OBJECT_PATH",
            object_path.as_bytes(),
        )?;

        // 4. _GTK_APP_MENU_OBJECT_PATH
        let app_menu_path = format!("{}/menus/AppMenu", object_path);
        Self::set_property(
            &xlib,
            display,
            window_id,
            "_GTK_APP_MENU_OBJECT_PATH",
            app_menu_path.as_bytes(),
        )?;

        // 5. _GTK_MENUBAR_OBJECT_PATH
        let menubar_path = format!("{}/menus/MenuBar", object_path);
        Self::set_property(
            &xlib,
            display,
            window_id,
            "_GTK_MENUBAR_OBJECT_PATH",
            menubar_path.as_bytes(),
        )?;

        debug_log("All X11 properties set successfully");
        Ok(())
        */
    }

    /// Set a single X11 window property
    #[allow(dead_code)]
    fn set_property(
        _xlib: &crate::desktop::shell2::linux::x11::dlopen::Xlib,
        _display: *mut std::ffi::c_void,
        _window_id: u64,
        _property_name: &str,
        _value: &[u8],
    ) -> Result<(), GnomeMenuError> {
        // TODO: Implement when Xlib is accessible
        /*
        use std::os::raw::{c_int, c_ulong};
        
        // Intern the atom for this property
        let property_cstr = CString::new(property_name)
            .map_err(|e| GnomeMenuError::X11PropertyFailed(e.to_string()))?;
        
        let atom = unsafe {
            (xlib.XInternAtom)(
                display as *mut _,
                property_cstr.as_ptr(),
                0,
            )
        };

        if atom == 0 {
            return Err(GnomeMenuError::X11PropertyFailed(
                format!("Failed to intern atom: {}", property_name)
            ));
        }

        // Set the property
        const XA_STRING: c_ulong = 31;
        const PROP_MODE_REPLACE: c_int = 0;
        
        let result = unsafe {
            XChangeProperty(
                display as *mut _,
                window_id as c_ulong,
                atom,
                XA_STRING,
                8, // 8-bit format
                PROP_MODE_REPLACE,
                value.as_ptr(),
                value.len() as c_int,
            )
        };

        if result != 0 {
            return Err(GnomeMenuError::X11PropertyFailed(
                format!("XChangeProperty failed for: {}", property_name)
            ));
        }

        debug_log(&format!("Set property: {} = {:?}", property_name, 
            String::from_utf8_lossy(value)));
        */
        
        Ok(())
    }
}

// X11 function signature (for future implementation)
#[allow(dead_code)]
extern "C" {
    fn XChangeProperty(
        display: *mut std::ffi::c_void,
        window: u64,
        property: u64,
        type_: u64,
        format: i32,
        mode: i32,
        data: *const u8,
        nelements: i32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_properties_returns_not_implemented() {
        // Until X11 integration is complete, this should return NotImplemented
        let result = X11Properties::set_properties(
            123,
            std::ptr::null_mut(),
            "test.app",
            "org.gtk.test_app",
            "/org/gtk/test/app",
        );
        
        assert!(result.is_err());
        match result.unwrap_err() {
            GnomeMenuError::NotImplemented => {},
            _ => panic!("Expected NotImplemented error"),
        }
    }
}
