@echo off
echo Deploying school management WASM program to ONVM...

REM Check if onvm CLI is installed
where onvm >nul 2>nul
if %errorlevel% neq 0 (
    echo ONVM CLI is not installed. Please install ONVM CLI to deploy this program.
    pause
    exit /b 1
)

REM Check if the WASM file exists
if not exist "school_management.wasm" (
    echo school_management.wasm not found. Please build the program first.
    echo Run: build.bat
    pause
    exit /b 1
)

REM Deploy the program
echo Deploying program...
for /f "delims=" %%i in ('onvm deploy --file school_management.wasm --entrypoint onvm_main 2^>^&1') do (
    set DEPLOY_OUTPUT=%%i
    echo %%i
)

if %errorlevel% equ 0 (
    echo Deployment successful!
) else (
    echo Deployment failed!
    pause
    exit /b 1
)

pause