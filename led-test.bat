@echo off
echo ============================================================
echo  fanatec-tuner LED test
echo ============================================================
echo.

echo Stopping Fanatec Wheel Service...
net stop FWPnpService 2>&1
echo Waiting for service to stop...
timeout /t 3 /nobreak >nul
echo.

echo Running LED test (output to fanatec-led-test.txt)...
echo Watch the wheel — LEDs and display should cycle through colours.
echo.
fanatec-tuner.exe led-test > fanatec-led-test.txt 2>&1

echo.
echo ============================================================
echo  Results:
echo ============================================================
type fanatec-led-test.txt
echo.

echo Restarting Fanatec Wheel Service...
net start FWPnpService 2>&1
echo.
echo Done. Full log saved to fanatec-led-test.txt
pause
