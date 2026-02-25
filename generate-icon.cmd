@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
uv run --frozen --project "%SCRIPT_DIR%" bash "%SCRIPT_DIR%generate-icon.sh" %*
