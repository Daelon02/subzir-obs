@echo off
setlocal
chcp 65001 >nul
"%~dp0app\python\python.exe" -B "%~dp0benchmarks\run20.py"
set "result=%errorlevel%"
pause
exit /b %result%
