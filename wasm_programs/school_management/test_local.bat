@echo off
echo Testing school management WASM program locally...

REM Check if onvm CLI is installed
where onvm >nul 2>nul
if %errorlevel% neq 0 (
    echo ONVM CLI is not installed.
    pause
    exit /b 1
)

REM Initialize a local node if it doesn't exist
if not exist "test_node" (
    echo Initializing test node...
    onvm init --data-dir ./test_node
)

REM Start a local node in the background
echo Starting local node...
start /b onvm run-node --data-dir ./test_node --listen /ip4/127.0.0.1/tcp/37001 --rpc 127.0.0.1:8081
timeout /t 3 /nobreak >nul

REM Deploy the program to the local node
echo Deploying program to local node...
for /f "delims=" %%i in ('onvm deploy --rpc 127.0.0.1:8081 --file school_management.wasm --entrypoint onvm_main 2^>^&1') do (
    set DEPLOY_RESULT=%%i
    echo %%i
)

REM Check if deployment was successful
echo %DEPLOY_RESULT% | findstr /c "\"id\"" >nul
if %errorlevel% equ 0 (
    for /f "tokens=4 delims=\"" %%a in ("%DEPLOY_RESULT%") do set PROGRAM_ID=%%a
    echo Program deployed with ID: %PROGRAM_ID%
    
    REM Test a simple operation
    echo Testing add_student operation...
    for /f "delims=" %%i in ('onvm execute --rpc 127.0.0.1:8081 --program_id %PROGRAM_ID% --input interactions/add_student.json 2^>^&1') do (
        echo Test result:
        echo %%i
    )
) else (
    echo Deployment failed:
    echo %DEPLOY_RESULT%
)

echo Local test completed.
pause
