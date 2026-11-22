@echo off
chcp 65001 >nul
echo 正在关闭星传...
taskkill /IM StarDrop.exe /F >nul 2>&1
echo 正在删除快捷方式...
del "%USERPROFILE%\Desktop\星传 StarDrop.lnk" >nul 2>&1
del "%APPDATA%\Microsoft\Windows\Start Menu\Programs\星传 StarDrop.lnk" >nul 2>&1
echo 正在删除程序...
rmdir /S /Q "%LOCALAPPDATA%\Programs\StarDrop" >nul 2>&1
rmdir /S /Q "%LOCALAPPDATA%\StarDrop" >nul 2>&1
echo.
echo 已卸载星传。已经收到的图片仍保留在：
echo %USERPROFILE%\Pictures\星传
echo.
pause
