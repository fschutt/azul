#!/bin/bash
set -e
EXECUTABLE_PATH="$1"
APP_NAME=$(basename "$EXECUTABLE_PATH")
APP_BUNDLE_PATH="$(dirname "$EXECUTABLE_PATH")/${APP_NAME}.app"
echo "Deploying ${APP_BUNDLE_PATH}..."
ios-deploy --bundle "${APP_BUNDLE_PATH}" --justlaunch
