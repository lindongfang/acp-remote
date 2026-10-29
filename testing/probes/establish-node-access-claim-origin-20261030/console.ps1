param([ValidateSet('start','read','approve')][string]$Action,[ValidateSet(30455,30456)][int]$Port=30455)
$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/claim-origin-acceptance-20261030'
if($Action -eq 'start'){
 $p=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node-$Port.toml",'device','pair') -WindowStyle Hidden -PassThru
 [IO.File]::WriteAllText("$run/owned-pair.pid",[string]$p.Id)
 Write-Output "owned pair pid=$($p.Id)";exit
}
$targetId=[uint32][IO.File]::ReadAllText("$run/owned-pair.pid")
$target=Get-CimInstance Win32_Process -Filter "ProcessId=$targetId"
if($target.Name -ne 'acp-remote.exe' -or $target.ExecutablePath -ne "$($run.Replace('/','\'))\bin\acp-remote.exe" -or $target.CommandLine -notlike '*claim-origin-acceptance-20261030*device pair*'){throw 'Not owned pair process'}
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class LossConsole {
 [StructLayout(LayoutKind.Sequential)] public struct Coord { public short X,Y; }
 [StructLayout(LayoutKind.Sequential)] public struct Key { public int Down; public ushort Repeat,VKey,Scan,Unicode; public uint Control; }
 [StructLayout(LayoutKind.Explicit,Size=20)] public struct Input { [FieldOffset(0)]public ushort Type;[FieldOffset(4)]public Key Event; }
 [DllImport("kernel32.dll")]static extern bool FreeConsole();
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool AttachConsole(uint p);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern IntPtr CreateFileW(string n,uint a,uint s,IntPtr sec,uint d,uint f,IntPtr t);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern bool ReadConsoleOutputCharacterW(IntPtr h,StringBuilder b,uint n,Coord c,out uint read);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool WriteConsoleInputW(IntPtr h,Input[] x,uint n,out uint wrote);
 [DllImport("kernel32.dll")]static extern bool CloseHandle(IntPtr h);
 public static string Run(uint p,bool approve){FreeConsole();if(!AttachConsole(p))throw new Win32Exception(Marshal.GetLastWin32Error());IntPtr h=IntPtr.Zero;try{h=CreateFileW(approve?"CONIN$":"CONOUT$",0xC0000000,3,IntPtr.Zero,3,0,IntPtr.Zero);if(h==new IntPtr(-1))throw new Win32Exception(Marshal.GetLastWin32Error());if(approve){var x=new Input[2];x[0]=new Input{Type=1,Event=new Key{Down=1,Repeat=1,Unicode=121}};x[1]=new Input{Type=1,Event=new Key{Down=1,Repeat=1,Unicode=13,VKey=13}};uint w;if(!WriteConsoleInputW(h,x,2,out w)||w!=2)throw new Win32Exception(Marshal.GetLastWin32Error());return "approval console events written";}else{var b=new StringBuilder(20000);uint r;if(!ReadConsoleOutputCharacterW(h,b,20000,new Coord(),out r))throw new Win32Exception(Marshal.GetLastWin32Error());return b.ToString();}}finally{if(h!=IntPtr.Zero&&h!=new IntPtr(-1))CloseHandle(h);FreeConsole();}}
}
'@
$result=[LossConsole]::Run($targetId,($Action -eq 'approve'))
[IO.File]::WriteAllText("$run/console-$Action.txt",$result,[Text.UTF8Encoding]::new($false))
if($Action -eq 'read'){
 $match=[regex]::Match($result,'pairingCode=([0-9A-Z-]+)')
 if($match.Success){[IO.File]::WriteAllText("$run/browser-input.json",(@{code=$match.Groups[1].Value}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))}
}
