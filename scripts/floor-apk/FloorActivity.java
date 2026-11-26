package com.azul.floor;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import android.view.View;

/**
 * The floor of the startup measurements: an Activity that puts one empty window on the screen and does nothing else. A demo app
 * (a NativeActivity with the engine behind it) is measured against this one, which is what the platform itself needs to start
 * a process and show a window.
 */
public class FloorActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        Log.i("AzFloor", "onCreate");
        setContentView(new View(this));
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            Log.i("AzFloor", "the window has the focus");
        }
    }
}
