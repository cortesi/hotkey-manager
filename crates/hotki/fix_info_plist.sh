#!/bin/bash
# Script to fix Info.plist after dx build to ensure LSUIElement is present and properly formatted

PLIST_PATH="../../target/dx/hotki/debug/macos/Hotki.app/Contents/Info.plist"

# Check if LSUIElement already exists
if ! grep -q "LSUIElement" "$PLIST_PATH"; then
    echo "Adding LSUIElement to Info.plist..."
    # Use sed to add LSUIElement before closing </dict>
    sed -i '' 's|</dict>|\
\t\t<key>LSUIElement</key>\
\t\t<true/>\
\t</dict>|' "$PLIST_PATH"
    echo "LSUIElement added successfully"
else
    echo "LSUIElement found, checking formatting..."
    # Fix any indentation issues with LSUIElement
    sed -i '' 's|^[ \t]*<key>LSUIElement</key>|\t\t<key>LSUIElement</key>|' "$PLIST_PATH"
    sed -i '' '/LSUIElement/,/<true\/>/s|^[ \t]*<true/>|\t\t<true/>|' "$PLIST_PATH"
    echo "LSUIElement formatting verified"
fi

# Validate the plist
if plutil -lint "$PLIST_PATH" > /dev/null 2>&1; then
    echo "Info.plist is valid"
else
    echo "Warning: Info.plist validation failed"
fi