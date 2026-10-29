using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
public sealed class ConsoleResult {
    public string Command, Output;
    public int ExitCode;
    public bool TimedOut, ConsoleInput, ConsoleOutput;
}
public static class ConsoleRunner {
    [DllImport("kernel32.dll")] static extern bool AllocConsole();
    [DllImport("kernel32.dll")] static extern IntPtr GetStdHandle(int n);
    [DllImport("kernel32.dll")] static extern bool SetStdHandle(int n, IntPtr h);
    [DllImport("kernel32.dll")] static extern bool SetHandleInformation(IntPtr h,uint mask,uint flags);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateFile(string n,uint a,uint s,IntPtr sa,uint c,uint f,IntPtr t);
    [DllImport("kernel32.dll")] static extern bool GetConsoleMode(IntPtr h,out uint m);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] static extern bool ReadConsoleOutputCharacter(IntPtr h,StringBuilder b,uint n,uint pos,out uint read);
    public static ConsoleResult Run(string exe,string args,int timeout) {
        AllocConsole();
        IntPtr input=CreateFile("CONIN$",0xC0000000,3,IntPtr.Zero,3,0,IntPtr.Zero);
        IntPtr output=CreateFile("CONOUT$",0xC0000000,3,IntPtr.Zero,3,0,IntPtr.Zero);
        SetHandleInformation(input,1,1); SetHandleInformation(output,1,1);
        SetStdHandle(-10,input); SetStdHandle(-11,output); SetStdHandle(-12,output);
        uint mode;
        var r=new ConsoleResult {Command=args,ConsoleInput=GetConsoleMode(GetStdHandle(-10),out mode),ConsoleOutput=GetConsoleMode(GetStdHandle(-11),out mode)};
        var p=Process.Start(new ProcessStartInfo(exe,args) {UseShellExecute=false,CreateNoWindow=false});
        r.TimedOut=!p.WaitForExit(timeout);
        if(r.TimedOut) {p.Kill();p.WaitForExit(3000);}
        r.ExitCode=p.ExitCode;
        var text=new StringBuilder(24000);uint count;
        ReadConsoleOutputCharacter(output,text,24000,0,out count);
        r.Output=System.Text.RegularExpressions.Regex.Replace(text.ToString().TrimEnd('\0',' '),"pairingCode=[^\\s]+","pairingCode=[REDACTED]");
        p.Dispose();return r;
    }
}
