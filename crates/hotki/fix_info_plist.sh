#!/bin/bash
# Script to fix Info.plist after dx build to ensure LSUIElement is present

PLIST_PATH="../../target/dx/hotki/debug/macos/Hotki.app/Contents/Info.plist"

# Check if LSUIElement already exists
if ! grep -q "LSUIElement" "$PLIST_PATH"; then
    echo "Adding LSUIElement to Info.plist..."
    # Use sed to add LSUIElement before closing </dict>
    sed -i '' 's|</dict>|\t\t<key>LSUIElement</key>\
\t\t<true/>\
\t</dict>|' "$PLIST_PATH"
    echo "LSUIElement added successfully"
else
    echo "LSUIElement already present in Info.plist"
fi