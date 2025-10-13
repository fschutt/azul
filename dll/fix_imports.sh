#!/bin/bash
# Script to fix imports in azul-dll after refactoring

cd "$(dirname "$0")"

# Callbacks moved to azul_layout
find src -name "*.rs" -exec sed -i '' 's/azul_core::callbacks::CallbackInfo/azul_layout::callbacks::CallbackInfo/g' {} \;

# window_state (some might have been missed)
find src -name "*.rs" -exec sed -i '' 's/use azul_core::window_state/use azul_layout::window_state/g' {} \;

# display_list moved
find src -name "*.rs" -exec sed -i '' 's/azul_core::display_list/azul_layout::display_list/g' {} \;

# WindowCreateOptions moved to azul_layout
find src -name "*.rs" -exec sed -i '' 's/azul_core::window::WindowCreateOptions/azul_layout::window::WindowCreateOptions/g' {} \;

# LogicalRect, LogicalSize, etc moved to geom
find src -name "*.rs" -exec sed -i '' 's/azul_core::window::LogicalRect/azul_core::geom::LogicalRect/g' {} \;
find src -name "*.rs" -exec sed -i '' 's/azul_core::window::LogicalSize/azul_core::geom::LogicalSize/g' {} \;
find src -name "*.rs" -exec sed -i '' 's/azul_core::window::LogicalPosition/azul_core::geom::LogicalPosition/g' {} \;

# CSS types moved
find src -name "*.rs" -exec sed -i '' 's/use azul_css::ColorU/use azul_css::props::basic::color::ColorU/g' {} \;
find src -name "*.rs" -exec sed -i '' 's/azul_css::LayoutSize/azul_core::geom::LayoutSize/g' {} \;
find src -name "*.rs" -exec sed -i '' 's/azul_css::LayoutPoint/azul_core::geom::LayoutPoint/g' {} \;
find src -name "*.rs" -exec sed -i '' 's/azul_css::LayoutRect/azul_core::geom::LayoutRect/g' {} \;

# LayoutResult moved
find src -name "*.rs" -exec sed -i '' 's/azul_core::ui_solver::LayoutResult/azul_layout::solver3::LayoutResult/g' {} \;

echo "Import fixes applied!"
