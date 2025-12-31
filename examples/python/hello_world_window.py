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
        StyledDom: The styled DOM tree to render
    """
    # Create a simple label
    label = azul.Dom.text(f"Hello from Python! Counter: {data.counter}")
    
    # Create a button that increments the counter
    button = azul.Button.new("Click me!")
    button_dom = button.dom()
    
    # Build the body
    body = azul.Dom.body()
    body.add_child(label)
    body.add_child(button_dom)
    
    # Apply default styling
    return azul.StyledDom.from_dom(body)

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
