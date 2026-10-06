@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-ssh.ps1" > "%~dp0setup-ssh.log" 2>&1
