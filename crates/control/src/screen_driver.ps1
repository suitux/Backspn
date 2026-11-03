# Puts the WinUSB driver that ships with Windows on a controller's screen
# interface and registers the interface for raw USB access, so Backspn can
# draw on the screens. Runs elevated; Backspn sets $HardwareId before this
# script, for example 'USB\VID_17CC&PID_1720&MI_04'.
#
# Exit codes: 0 done, 3 no such interface plugged in, 4 the driver could not
# be installed, 5 the interface could not be registered.

$ErrorActionPreference = 'Stop'

# The same interface class for every screen, so raw USB finds it.
$InterfaceGuid = '{9C236364-BB55-4FBB-A44E-6B19DB09F530}'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class BackspnScreenDriver
{
    const int DI_ENUMSINGLEINF = 0x00010000;
    const int DI_FLAGSEX_ALLOWEXCLUDEDDRVS = 0x00000800;
    const int SPDIT_CLASSDRIVER = 1;
    const int DETAIL_SIZE = 1584;
    const int SECTION_OFFSET = 32;

    [StructLayout(LayoutKind.Sequential)]
    struct DevInfo
    {
        public int cbSize;
        public Guid ClassGuid;
        public int DevInst;
        public IntPtr Reserved;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct InstallParams
    {
        public int cbSize;
        public int Flags;
        public int FlagsEx;
        public IntPtr hwndParent;
        public IntPtr InstallMsgHandler;
        public IntPtr InstallMsgHandlerContext;
        public IntPtr FileQueue;
        public IntPtr ClassInstallReserved;
        public int Reserved;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)]
        public string DriverPath;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct DriverInfo
    {
        public int cbSize;
        public int DriverType;
        public IntPtr Reserved;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
        public string Description;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
        public string MfgName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
        public string ProviderName;
        public long DriverDate;
        public long DriverVersion;
    }

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern IntPtr SetupDiCreateDeviceInfoList(IntPtr classGuid, IntPtr parent);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiOpenDeviceInfoW(IntPtr set, string instanceId, IntPtr parent, int flags, ref DevInfo dev);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiGetDeviceInstallParamsW(IntPtr set, ref DevInfo dev, ref InstallParams p);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiSetDeviceInstallParamsW(IntPtr set, ref DevInfo dev, ref InstallParams p);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiBuildDriverInfoList(IntPtr set, ref DevInfo dev, int type);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiEnumDriverInfoW(IntPtr set, ref DevInfo dev, int type, int index, ref DriverInfo drv);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiGetDriverInfoDetailW(IntPtr set, ref DevInfo dev, ref DriverInfo drv, IntPtr detail, int size, out int required);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiDestroyDeviceInfoList(IntPtr set);

    [DllImport("newdev.dll", SetLastError = true)]
    static extern bool DiInstallDevice(IntPtr hwnd, IntPtr set, ref DevInfo dev, ref DriverInfo drv, int flags, out bool reboot);

    static string Fail(string step)
    {
        return step + " failed (error " + Marshal.GetLastWin32Error() + ")";
    }

    /// Installs the WinUSB driver from Windows' own winusb.inf on the device;
    /// returns null when done, or what went wrong.
    public static string Install(string instanceId, string inf)
    {
        IntPtr set = SetupDiCreateDeviceInfoList(IntPtr.Zero, IntPtr.Zero);
        if (set == new IntPtr(-1)) return Fail("Creating the device list");
        IntPtr detail = Marshal.AllocHGlobal(8192);
        try
        {
            DevInfo dev = new DevInfo();
            dev.cbSize = Marshal.SizeOf(typeof(DevInfo));
            if (!SetupDiOpenDeviceInfoW(set, instanceId, IntPtr.Zero, 0, ref dev)) return Fail("Opening the device");

            InstallParams p = new InstallParams();
            p.cbSize = Marshal.SizeOf(typeof(InstallParams));
            if (!SetupDiGetDeviceInstallParamsW(set, ref dev, ref p)) return Fail("Reading the install settings");
            p.Flags |= DI_ENUMSINGLEINF;
            p.FlagsEx |= DI_FLAGSEX_ALLOWEXCLUDEDDRVS;
            p.DriverPath = inf;
            if (!SetupDiSetDeviceInstallParamsW(set, ref dev, ref p)) return Fail("Pointing at winusb.inf");
            if (!SetupDiBuildDriverInfoList(set, ref dev, SPDIT_CLASSDRIVER)) return Fail("Listing the drivers");

            for (int i = 0; ; i++)
            {
                DriverInfo drv = new DriverInfo();
                drv.cbSize = Marshal.SizeOf(typeof(DriverInfo));
                if (!SetupDiEnumDriverInfoW(set, ref dev, SPDIT_CLASSDRIVER, i, ref drv)) break;
                Marshal.WriteInt32(detail, DETAIL_SIZE);
                int required;
                if (!SetupDiGetDriverInfoDetailW(set, ref dev, ref drv, detail, 8192, out required)) continue;
                string section = Marshal.PtrToStringUni(IntPtr.Add(detail, SECTION_OFFSET));
                if (!string.Equals(section, "WINUSB", StringComparison.OrdinalIgnoreCase)) continue;
                bool reboot;
                if (!DiInstallDevice(IntPtr.Zero, set, ref dev, ref drv, 0, out reboot)) return Fail("Installing WinUSB");
                return null;
            }
            return "winusb.inf offers no WinUSB driver for this device";
        }
        finally
        {
            Marshal.FreeHGlobal(detail);
            SetupDiDestroyDeviceInfoList(set);
        }
    }
}
'@

$devices = @(Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -like "$HardwareId\*" })
if ($devices.Count -eq 0) {
    Write-Host "No screen interface $HardwareId is plugged in."
    exit 3
}

$inf = Join-Path $env:windir 'INF\winusb.inf'
foreach ($device in $devices) {
    $id = $device.InstanceId
    $service = (Get-PnpDeviceProperty -InstanceId $id -KeyName 'DEVPKEY_Device_Service').Data
    if ($service -ne 'WINUSB') {
        $why = [BackspnScreenDriver]::Install($id, $inf)
        if ($why) {
            Write-Host "${id}: $why"
            exit 4
        }
    }
    try {
        $key = "HKLM:\SYSTEM\CurrentControlSet\Enum\$id\Device Parameters"
        New-ItemProperty -Path $key -Name DeviceInterfaceGUIDs -PropertyType MultiString `
            -Value @($InterfaceGuid) -Force | Out-Null
    } catch {
        Write-Host "${id}: $_"
        exit 5
    }
    # The interface reads its registration when it starts.
    pnputil /restart-device "$id" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        try {
            Disable-PnpDevice -InstanceId $id -Confirm:$false
            Enable-PnpDevice -InstanceId $id -Confirm:$false
        } catch {
            Write-Host "${id}: unplug the controller and plug it in again."
        }
    }
}
exit 0
