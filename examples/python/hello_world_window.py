#!/usr/bin/env python3
"""
Azul GUI Hello World - Opens a window with a simple label.

To run:
    cd /path/to/azul
    python3 examples/python/hello_world_window.py
"""

import sys
import os

# Add the directory containing azul.so to Python path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', 'target', 'release'))

import azul

class AppData:
    """Application state - can hold any Python data"""
    def __init__(self):
        self.counter = 0

def layout(data, info):
    """
    Layout callback - creates the DOM structure for the window.
    
    Args:
        data: The AppData instance (or whatever was passed to App())
        info: LayoutCallbackInfo with window size, theme, etc.
    
    Returns:
        Dom: The styled DOM tree to render
    """
    # Create a simple label
    label = azul.Dom.create_text(f"Hello from Python! Counter: {data.counter}")

    # Create a button that increments the counter
    button_dom = azul.Button.create("Click me!").dom()

    # Build the body via the builder API (set_*/add_* aren't exposed
    # in the Python binding for Dom; use with_child / with_css).
    return (azul.Dom.create_body()
            .with_child(label)
            .with_child(button_dom))

def main():
    print("Starting Azul GUI application...")
    
    # Create app data
    data = AppData()
    
    # Create the application with our data and config
    app = azul.App.create(data, azul.AppConfig.create())
    
    # Configure the window with a layout callback
    window_options = azul.WindowCreateOptions.create(layout)
    
    # Run the app with the initial window (this blocks until the window is closed)
    app.run(window_options)
    
    print("Application closed.")

if __name__ == "__main__":
    main()
