#!/bin/bash
# Resize tray icon from logo

set -e

cd "$(dirname "$0")/.."
sips -z 22 22 crates/hotki/logo/logo.png --out crates/hotki/logo/tray-icon.png