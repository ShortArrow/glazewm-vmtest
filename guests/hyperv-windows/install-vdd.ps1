<#
.SYNOPSIS
Installs the IddCx virtual display driver in the guest, without user interaction.

.DESCRIPTION
Trusts the catalog's signer as a publisher, creates the root-enumerated device Root\MttVDD, installs the driver on it
with pnputil, and puts vdd_settings.xml where the driver reads it. A device that already exists is reused.
Runs elevated, over SSH as an administrator.

.PARAMETER DriverFolder
The VirtualDisplayDriver folder of the release asset VirtualDisplayDriver-x86.Driver.Only.zip: mttvdd.cat, MttVDD.dll,
MttVDD.inf and vdd_settings.xml.
#>
param(
    [Parameter(Mandatory)] [string] $DriverFolder
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class RootDevice
{
    [StructLayout(LayoutKind.Sequential)]
    struct SP_DEVINFO_DATA { public int cbSize; public Guid ClassGuid; public int DevInst; public IntPtr Reserved; }

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern IntPtr SetupDiCreateDeviceInfoList(ref Guid classGuid, IntPtr hwndParent);
    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiCreateDeviceInfoW(IntPtr set, string name, ref Guid classGuid, string description, IntPtr hwndParent, int flags, ref SP_DEVINFO_DATA data);
    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiSetDeviceRegistryPropertyW(IntPtr set, ref SP_DEVINFO_DATA data, int property, byte[] buffer, int size);
    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiCallClassInstaller(int function, IntPtr set, ref SP_DEVINFO_DATA data);
    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiDestroyDeviceInfoList(IntPtr set);

    /// <summary>Registers a root-enumerated device with the given class and hardware ID, as devcon install does before the driver is added.</summary>
    public static void Create(Guid classGuid, string className, string hardwareId)
    {
        var set = SetupDiCreateDeviceInfoList(ref classGuid, IntPtr.Zero);
        if (set == new IntPtr(-1)) { throw new System.ComponentModel.Win32Exception(); }
        try
        {
            var data = new SP_DEVINFO_DATA { cbSize = Marshal.SizeOf(typeof(SP_DEVINFO_DATA)) };
            const int DICD_GENERATE_ID = 1, SPDRP_HARDWAREID = 1, DIF_REGISTERDEVICE = 0x19;
            var ids = System.Text.Encoding.Unicode.GetBytes(hardwareId + "\0\0");
            if (!SetupDiCreateDeviceInfoW(set, className, ref classGuid, null, IntPtr.Zero, DICD_GENERATE_ID, ref data)
                || !SetupDiSetDeviceRegistryPropertyW(set, ref data, SPDRP_HARDWAREID, ids, ids.Length)
                || !SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set, ref data))
            {
                throw new System.ComponentModel.Win32Exception();
            }
        }
        finally
        {
            SetupDiDestroyDeviceInfoList(set);
        }
    }
}
'@

$hardwareId = 'Root\MttVDD'
$displayClass = [Guid]'4d36e968-e325-11ce-bfc1-08002be10318'

$signer = (Get-AuthenticodeSignature (Join-Path $DriverFolder 'mttvdd.cat')).SignerCertificate
$publishers = New-Object Security.Cryptography.X509Certificates.X509Store('TrustedPublisher', 'LocalMachine')
$publishers.Open('ReadWrite')
try { $publishers.Add($signer) } finally { $publishers.Close() }

if (-not (Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object { $_.HardwareID -contains $hardwareId })) {
    [RootDevice]::Create($displayClass, 'Display', $hardwareId)
}

pnputil.exe /add-driver (Join-Path $DriverFolder 'MttVDD.inf') /install
if ($LASTEXITCODE -ne 0) { throw "pnputil exited with $LASTEXITCODE" }

New-Item -ItemType Directory -Force -Path 'C:\VirtualDisplayDriver' | Out-Null
Copy-Item (Join-Path $DriverFolder 'vdd_settings.xml') 'C:\VirtualDisplayDriver\vdd_settings.xml' -Force
