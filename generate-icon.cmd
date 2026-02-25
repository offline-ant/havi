@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
set "PROJECT_DIR=%SCRIPT_DIR:~0,-1%"
uv run --frozen --project "%PROJECT_DIR%" bash "%SCRIPT_DIR%generate-icon.sh" %*
