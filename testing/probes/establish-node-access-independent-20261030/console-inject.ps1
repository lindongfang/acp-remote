param([Parameter(Mandatory=$true)][uint32]$TargetProcessId,
      [Parameter(Mandatory=$true)][string]$EventFile,
      [Parameter(Mandatory=$true)][string]$ResultFile)
$ErrorActionPreference = 'Stop'
$target = Get-CimInstance Win32_Process -Filter "ProcessId=$TargetProcessId"
if ($target.Name -ne 'acp-remote.exe' -or $target.CommandLine -notlike '*approval-r5a-acceptance-20261030*device pair*') { throw 'not the owned r5a pair console' }
$events = @(Get-Content -LiteralPath $EventFile -Raw | ConvertFrom-Json)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.ComponentModel;
public static class IndependentConsoleInput {
  [StructLayout(LayoutKind.Sequential)] public struct KeyRecord { public int Down; public ushort Repeat; public ushort VirtualKey; public ushort Scan; public ushort Unicode; public uint Control; }
  [StructLayout(LayoutKind.Explicit, Size=20)] public struct InputRecord { [FieldOffset(0)] public ushort Type; [FieldOffset(4)] public KeyRecord Key; }
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool FreeConsole();
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool AttachConsole(uint pid);
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateFileW(string name,uint access,uint share,IntPtr security,uint disposition,uint flags,IntPtr template);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool WriteConsoleInputW(IntPtr input,InputRecord[] records,uint count,out uint written);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
  public static uint Inject(uint pid,ushort[] units,ushort[] repeats) {
    FreeConsole();
    if(!AttachConsole(pid)) throw new Win32Exception(Marshal.GetLastWin32Error(),"AttachConsole");
    IntPtr handle=new IntPtr(-1);
    try {
      handle=CreateFileW("CONIN$",0xC0000000,3,IntPtr.Zero,3,0,IntPtr.Zero);
      if(handle==new IntPtr(-1))throw new Win32Exception(Marshal.GetLastWin32Error(),"CONIN$");
      var records=new InputRecord[units.Length];
      for(int i=0;i<records.Length;i++)records[i]=new InputRecord{Type=1,Key=new KeyRecord{Down=1,Repeat=repeats[i],Unicode=units[i],VirtualKey=(ushort)(units[i]==13?13:0)}};
      uint written;
      if(!WriteConsoleInputW(handle,records,(uint)records.Length,out written))throw new Win32Exception(Marshal.GetLastWin32Error(),"WriteConsoleInputW");
      return written;
    } finally { if(handle!=new IntPtr(-1))CloseHandle(handle); FreeConsole(); }
  }
}
'@
try {
  $written=[IndependentConsoleInput]::Inject($TargetProcessId,[ushort[]]@($events|ForEach-Object{$_.unit}),[ushort[]]@($events|ForEach-Object{$_.repeat}))
  [IO.File]::WriteAllText($ResultFile,(@{ok=($written -eq $events.Count);written=$written;requested=$events.Count;target=$TargetProcessId}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
  if($written -ne $events.Count){exit 1}
} catch {
  [IO.File]::WriteAllText($ResultFile,(@{ok=$false;error=$_.Exception.ToString();target=$TargetProcessId}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
  exit 1
}
