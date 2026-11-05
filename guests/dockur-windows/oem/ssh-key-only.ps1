<#
.SYNOPSIS
Makes the guest's sshd accept public keys only, then restarts it.

.DESCRIPTION
The guest user is an administrator, and a Hyper-V guest sits on the host's network, so a password login would let
anyone there who guesses the password in. sshd takes the first value it reads for each option, so the two lines go
at the top of sshd_config, ahead of the shipped defaults and any Match block. Running it again changes nothing.
#>
$ErrorActionPreference = 'Stop'

$config = 'C:\ProgramData\ssh\sshd_config'
$keyOnly = @('PasswordAuthentication no', 'KbdInteractiveAuthentication no')
$lines = @(Get-Content -Path $config)
if (($lines | Select-Object -First $keyOnly.Count) -join "`n" -ne ($keyOnly -join "`n")) {
    Set-Content -Path $config -Value ($keyOnly + $lines) -Encoding ascii
}
& "$env:WINDIR\System32\OpenSSH\sshd.exe" -t
if ($LASTEXITCODE -ne 0) { throw "sshd rejects $config" }
Restart-Service -Name sshd
