<#
.SYNOPSIS
Installs the Microsoft Visual C++ 2015-2022 x64 redistributable in the guest, which glazewm's release build imports
vcruntime140.dll from.

.DESCRIPTION
Downloads Microsoft's permanent link for the latest redistributable, refuses it unless its Authenticode signature is
valid and issued to Microsoft Corporation, and installs it quietly. Runs elevated, over SSH as an administrator.
#>
$ErrorActionPreference = 'Stop'

$installer = Join-Path $env:TEMP 'vc_redist.x64.exe'
Invoke-WebRequest -UseBasicParsing -Uri 'https://aka.ms/vs/17/release/vc_redist.x64.exe' -OutFile $installer

$signature = Get-AuthenticodeSignature $installer
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
    throw "Refusing $installer : signature $($signature.Status), signer $($signature.SignerCertificate.Subject)"
}

$process = Start-Process -FilePath $installer -ArgumentList '/install', '/quiet', '/norestart' -Wait -PassThru
if ($process.ExitCode -notin 0, 3010) { throw "vc_redist exited with $($process.ExitCode)" }
