param([ValidateSet('code','read','approve','ctrlc')][string]$Action,[ValidateSet('committed','precommit','control')][string]$Case)
$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/approval-cancel-boundary-r20-20261030';$dir=Join-Path $run $Case
if($Action -eq 'ctrlc' -and (Test-Path -LiteralPath (Join-Path $dir 'console-ctrlc.json'))){throw 'Refuse a second Ctrl-C in this finite case'}
$keeperId=[uint32][IO.File]::ReadAllText((Join-Path $dir 'keeper.pid'))
$keeper=Get-CimInstance Win32_Process -Filter "ProcessId=$keeperId"
if($keeper.Name -ne 'powershell.exe' -or $keeper.CommandLine -notlike '*establish-node-access-approval-cancel-r20-20261030/keeper.ps1*'){throw 'not owned keeper'}
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.ComponentModel;
public static class OwnedConsole {
 [StructLayout(LayoutKind.Sequential)]public struct Coord{public short X,Y;}
 [StructLayout(LayoutKind.Sequential)]public struct Key{public int Down;public ushort Repeat,VKey,Scan,Unicode;public uint Control;}
 [StructLayout(LayoutKind.Explicit,Size=20)]public struct Input{[FieldOffset(0)]public ushort Type;[FieldOffset(4)]public Key Event;}
 public delegate bool Handler(uint code);public static Handler Guard=code=>true;
 [DllImport("kernel32.dll")]static extern bool FreeConsole();
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool AttachConsole(uint id);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern IntPtr CreateFileW(string n,uint a,uint s,IntPtr sec,uint d,uint f,IntPtr t);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern bool ReadConsoleOutputCharacterW(IntPtr h,StringBuilder b,uint n,Coord p,out uint read);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool WriteConsoleInputW(IntPtr h,Input[] r,uint n,out uint written);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool GenerateConsoleCtrlEvent(uint e,uint group);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool SetConsoleCtrlHandler(Handler h,bool add);
 [DllImport("kernel32.dll")]static extern bool CloseHandle(IntPtr h);
 public static string Run(uint id,string action){FreeConsole();if(!AttachConsole(id))throw new Win32Exception(Marshal.GetLastWin32Error(),"owned AttachConsole");IntPtr h=IntPtr.Zero;try{if(action=="ctrlc"){if(!SetConsoleCtrlHandler(Guard,true)||!GenerateConsoleCtrlEvent(0,0))throw new Win32Exception(Marshal.GetLastWin32Error(),"owned Ctrl-C");System.Threading.Thread.Sleep(250);return "ctrlc-sent";}h=CreateFileW(action=="approve"?"CONIN$":"CONOUT$",0xC0000000,3,IntPtr.Zero,3,0,IntPtr.Zero);if(h==new IntPtr(-1))throw new Win32Exception(Marshal.GetLastWin32Error());if(action=="approve"){var r=new Input[2];r[0]=new Input{Type=1,Event=new Key{Down=1,Repeat=1,Unicode=121}};r[1]=new Input{Type=1,Event=new Key{Down=1,Repeat=1,Unicode=13,VKey=13}};uint w;if(!WriteConsoleInputW(h,r,2,out w)||w!=2)throw new Win32Exception(Marshal.GetLastWin32Error());return "approval-written";}var b=new StringBuilder(24000);uint count;if(!ReadConsoleOutputCharacterW(h,b,24000,new Coord(),out count))throw new Win32Exception(Marshal.GetLastWin32Error());return b.ToString();}finally{if(h!=IntPtr.Zero&&h!=new IntPtr(-1))CloseHandle(h);FreeConsole();}}
}
'@
$text=[OwnedConsole]::Run($keeperId,$Action)
if($Action -eq 'code'){
 $m=[regex]::Match($text,'pairingCode=([0-9A-Z]{4}-[0-9A-Z]{4})')
 if(-not $m.Success){throw 'pair code not yet visible'}
 # This output is consumed only through child_process.execFile into the browser driver's memory.
 [Console]::WriteLine((@{code=$m.Groups[1].Value}|ConvertTo-Json -Compress));exit
}
$idMatch=[regex]::Match($text,'clientId=([A-Za-z0-9_-]{22})')
$publicId=if($idMatch.Success){$idMatch.Groups[1].Value}else{$null}
$record=@{case=$Case;action=$Action;promptVisible=$text.Contains('Approve this client? [y/N]');cancelledBeforeApproval=$text.Contains('Pairing cancelled before approval.');trustedMessage=$text.Contains('The client is now trusted.');revokeHint=([regex]::IsMatch($text,'device\s+revoke'));pairCodeVisible=([regex]::IsMatch($text,'pairingCode=[0-9A-Z]{4}-[0-9A-Z]{4}'));signalSent=($text -eq 'ctrlc-sent');approvalWritten=($text -eq 'approval-written');keeperPid=$keeperId;decisionInterrupted=$text.Contains('Pairing decision interrupted;');conditionalCommitted=$text.Contains('approval may already have committed');deviceListHint=([regex]::IsMatch($text,'device\s+list'));cancellationNotRevocation=([regex]::IsMatch($text,'cancellation\s+does\s+not\s+revoke\s+trust'));unconfirmed=$text.Contains('Approval was not confirmed.');checkUnavailable=$text.Contains('Approval could not be checked.');clientId=$publicId;fullRevokeCommand=($publicId -and [regex]::IsMatch($text,('device\s+revoke\s+--device-id\s+'+[regex]::Escape($publicId))));conditionalBeforeConfirmedId=($idMatch.Success -and $text.IndexOf('Pairing decision interrupted;') -ge 0 -and $text.IndexOf('Pairing decision interrupted;') -lt $idMatch.Index);sampleAt=[DateTimeOffset]::UtcNow.ToString('O')}
$commandMatch=[regex]::Match($text,'acp-remote\s+--config\s+"[^"]+"\s+device\s+revoke\s+--device-id\s+([A-Za-z0-9_-]{22})')
if($commandMatch.Success -and $commandMatch.Groups[1].Value -eq $publicId){$record.revokeCommand=$commandMatch.Value;$record.exactCommandClientId=$commandMatch.Groups[1].Value}else{$record.revokeCommand=$null;$record.exactCommandClientId=$null}
[IO.File]::WriteAllText((Join-Path $dir "console-$Action.json"),($record|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
$record|ConvertTo-Json -Compress
