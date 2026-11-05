<#
.SYNOPSIS
Enables the OpenSSH server in the guest, trusts the harness key for administrators, and refuses passwords.

.DESCRIPTION
Runs once, from install.bat, at the end of the unattended install.
Expects authorized_key.pub and ssh-key-only.ps1 beside this script.
#>
$ErrorActionPreference = 'Stop'

Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service -Name sshd

if (-not (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue)) {
    New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' `
        -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
}
Set-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -Profile Any

$authorizedKeys = 'C:\ProgramData\ssh\administrators_authorized_keys'
Get-Content -Path (Join-Path $PSScriptRoot 'authorized_key.pub') |
    Set-Content -Path $authorizedKeys -Encoding ascii
icacls.exe $authorizedKeys /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F'

New-ItemProperty -Path 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell `
    -Value 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -PropertyType String -Force

& (Join-Path $PSScriptRoot 'ssh-key-only.ps1')
