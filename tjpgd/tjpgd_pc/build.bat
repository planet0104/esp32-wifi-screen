@echo off
REM Build script for TJpgDec Windows PC version
REM Requires GCC (MinGW or TDM-GCC)

echo ====================================
echo Building TJpgDec for Windows PC
echo ====================================
echo.

REM Check if clang is available
where clang >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    echo Using Clang compiler...
    set CC=clang
    goto :compile
)

REM Check if gcc is available
where gcc >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    echo Using GCC compiler...
    set CC=gcc
    goto :compile
)

REM Check if cl (MSVC) is available
where cl >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    echo Using MSVC compiler...
    set CC=cl
    goto :compile
)

echo Error: No C compiler found in PATH
echo Please install one of the following:
echo   LLVM/Clang: https://releases.llvm.org/
echo   MinGW: http://www.mingw.org/
echo   TDM-GCC: https://jmeubank.github.io/tdm-gcc/
pause
exit /b 1

:compile
echo Compiling...
%CC% -o tjpgd_test.exe main.c tjpgd.c -O2 -Wall

if %ERRORLEVEL% EQU 0 (
    echo.
    echo ====================================
    echo Build completed successfully!
    echo Executable: tjpgd_test.exe
    echo ====================================
    echo.
    echo Usage: tjpgd_test.exe input.jpg [output.ppm]
) else (
    echo.
    echo ====================================
    echo Build failed!
    echo ====================================
)

echo.
