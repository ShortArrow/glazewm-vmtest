<#
.SYNOPSIS
Builds the answer-file ISO that the guest's unattended install reads from its second DVD drive.

.DESCRIPTION
The ISO holds autounattend.xml, with the image name and the guest password filled in, and the sshd setup that its
first logon runs: setup-ssh.ps1, the ssh-key-only.ps1 beside it, and authorized_key.pub. Windows Setup reads autounattend.xml from the root of any
removable medium. The ISO is written with IMAPI2, which ships with Windows, so no Windows ADK is needed.

.PARAMETER AnswerFile
autounattend.xml with the placeholders __IMAGE_NAME__ and __GUEST_PASSWORD__.

.PARAMETER SshSetup
The script the first logon runs to enable sshd; authorized_key.pub and ssh-key-only.ps1 are read from its own folder.

.PARAMETER AuthorizedKey
The public key the harness connects with.

.PARAMETER ImageName
The /IMAGE/NAME of the edition in the installation media's install.wim, as Get-WindowsImage lists it.

.PARAMETER GuestPassword
The password of the user fixture, who logs on automatically.

.PARAMETER Output
Where the ISO is written. An existing file is replaced.
#>
param(
    [Parameter(Mandatory)] [string] $AnswerFile,
    [Parameter(Mandatory)] [string] $SshSetup,
    [Parameter(Mandatory)] [string] $AuthorizedKey,
    [Parameter(Mandatory)] [string] $ImageName,
    [Parameter(Mandatory)] [string] $GuestPassword,
    [Parameter(Mandatory)] [string] $Output
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System.IO;
using System.Runtime.InteropServices.ComTypes;

public static class ImageStreamWriter
{
    /// <summary>Copies an IMAPI2 result image stream to a file.</summary>
    public static void Save(object imageStream, string path)
    {
        var stream = (IStream)imageStream;
        var buffer = new byte[1 << 16];
        var read = System.Runtime.InteropServices.Marshal.AllocHGlobal(4);
        try
        {
            using (var file = File.Create(path))
            {
                while (true)
                {
                    stream.Read(buffer, buffer.Length, read);
                    var count = System.Runtime.InteropServices.Marshal.ReadInt32(read);
                    if (count == 0) { return; }
                    file.Write(buffer, 0, count);
                }
            }
        }
        finally
        {
            System.Runtime.InteropServices.Marshal.FreeHGlobal(read);
        }
    }
}
'@

$staging = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    (Get-Content -Raw -Encoding UTF8 $AnswerFile).
        Replace('__IMAGE_NAME__', [Security.SecurityElement]::Escape($ImageName)).
        Replace('__GUEST_PASSWORD__', [Security.SecurityElement]::Escape($GuestPassword)) |
        Set-Content -Encoding UTF8 -NoNewline (Join-Path $staging 'autounattend.xml')
    Copy-Item $SshSetup (Join-Path $staging 'setup-ssh.ps1')
    Copy-Item (Join-Path (Split-Path $SshSetup) 'ssh-key-only.ps1') (Join-Path $staging 'ssh-key-only.ps1')
    Copy-Item $AuthorizedKey (Join-Path $staging 'authorized_key.pub')

    $image = New-Object -ComObject IMAPI2FS.MsftFileSystemImage
    $image.FileSystemsToCreate = 3
    $image.VolumeName = 'ANSWER'
    $image.Root.AddTree($staging, $false)
    [ImageStreamWriter]::Save($image.CreateResultImage().ImageStream, $Output)
} finally {
    Remove-Item -Recurse -Force $staging
}
