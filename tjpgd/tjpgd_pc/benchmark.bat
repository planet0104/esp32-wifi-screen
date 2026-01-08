@echo off
REM Benchmark script for TJpgDec

echo ====================================
echo TJpgDec Performance Benchmark
echo ====================================
echo.

if not exist "tjpgd_test.exe" (
    echo Error: tjpgd_test.exe not found
    echo Please run build.bat first
    pause
    exit /b 1
)

if not exist "test.jpg" (
    echo Error: test.jpg not found
    echo Please provide a test image
    pause
    exit /b 1
)

echo Running benchmark (10 iterations)...
echo.

set count=0
set /a total_time=0

:loop
if %count% geq 10 goto :done

echo Iteration %count%...
tjpgd_test.exe test.jpg benchmark_output.ppm >nul 2>&1

set /a count+=1
goto :loop

:done
echo.
echo ====================================
echo Benchmark completed!
echo Iterations: 10
echo ====================================
echo.
echo Note: For accurate timing, use PowerShell benchmark script
echo or measure with external tools.
echo.
pause

