@echo off
setlocal enabledelayedexpansion

echo Building ONVM Identity Card WASM Program...
echo.

cd /d "%~dp0"

where rustup >nul 2>nul
if %errorlevel% neq 0 (
    echo Error: rustup not found. Please install Rust first.
    exit /b 1
)

echo Adding wasm32-unknown-unknown target...
rustup target add wasm32-unknown-unknown

echo Building in release mode...
cargo build --target wasm32-unknown-unknown --release

set WASM_FILE=target\wasm32-unknown-unknown\release\onvm_identity_card.wasm

if exist "%WASM_FILE%" (
    for %%A in ("%WASM_FILE%") do set SIZE=%%~zA
    echo.
    echo Build successful!
    echo   Output: %WASM_FILE%
    echo   Size: !SIZE! bytes
    echo.
    echo Next steps:
    echo 1. Upload: onvm upload-blob --file %WASM_FILE% --rpc 127.0.0.1:8080
    echo 2. Deploy: onvm deploy --blob-id blob^<64-hex^> --rpc 127.0.0.1:8080
    echo 3. Execute: onvm execute --program-id prog^<64-hex^> --input register.json --rpc 127.0.0.1:8080
) else (
    echo.
    echo Build failed - WASM file not found
    exit /b 1
)
