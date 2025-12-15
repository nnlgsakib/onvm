@echo off
echo Building school management WASM program...

REM Check if tinygo is installed
where tinygo >nul 2>nul
if %errorlevel% neq 0 (
    echo TinyGo is not installed. Please install TinyGo to build this program.
    echo Visit: https://tinygo.org/getting-started/
    pause
    exit /b 1
)

REM Build the WASM module
tinygo build -o school_management.wasm -target wasi main.go

if %errorlevel% equ 0 (
    echo Build successful! school_management.wasm created.
    echo You can now deploy this program to ONVM:
    echo   onvm deploy --file school_management.wasm --entrypoint onvm_main
) else (
    echo Build failed!
    pause
    exit /b 1
)

pause