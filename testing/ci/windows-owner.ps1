# Dot-source in the same PowerShell process that launches the Windows tests.
# Hosted runners may default new object ownership to the Administrators group.
# Change only the process token default owner, never existing filesystem owners.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;
public static class CiObjectOwner {
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool GetTokenInformation(IntPtr token, int kind, IntPtr buffer, int size, out int needed);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool SetTokenInformation(IntPtr token, int kind, ref IntPtr owner, int size);
    public static void SetCurrentUser() {
        IntPtr token;
        if (!OpenProcessToken(GetCurrentProcess(), 0x0080 | 0x0008, out token))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        IntPtr buffer = IntPtr.Zero;
        try {
            int needed;
            GetTokenInformation(token, 1, IntPtr.Zero, 0, out needed); // TokenUser
            if (needed <= 0) throw new Win32Exception(Marshal.GetLastWin32Error());
            buffer = Marshal.AllocHGlobal(needed);
            if (!GetTokenInformation(token, 1, buffer, needed, out needed))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            IntPtr user = Marshal.ReadIntPtr(buffer);
            if (!SetTokenInformation(token, 4, ref user, IntPtr.Size)) // TokenOwner
                throw new Win32Exception(Marshal.GetLastWin32Error());
        } finally {
            if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
            CloseHandle(token);
        }
    }
}
"@
[CiObjectOwner]::SetCurrentUser()

# Assert the actual filesystem behavior, not just success of SetTokenInformation.
$ownerProbe = Join-Path ([IO.Path]::GetTempPath()) ('acp-ci-owner-' + [guid]::NewGuid().ToString('N'))
try {
    New-Item -ItemType Directory -Path $ownerProbe | Out-Null
    $expectedOwner = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $actualOwner = (Get-Acl -LiteralPath $ownerProbe).GetOwner([Security.Principal.SecurityIdentifier])
    if ($actualOwner -ne $expectedOwner) {
        throw 'CI temporary directory owner does not match the process user.'
    }
    Write-Host 'CI new-object ownership matches the process user.'
} finally {
    if (Test-Path -LiteralPath $ownerProbe) { Remove-Item -LiteralPath $ownerProbe }
}
