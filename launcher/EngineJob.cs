using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

// A worker includes FFmpeg, Python and possibly llama-server. Closing the
// launcher's job stops the complete tree, including children holding camera or pipe handles.
sealed class EngineJob : SafeHandleZeroOrMinusOneIsInvalid {
    private EngineJob():base(true){}
    [StructLayout(LayoutKind.Sequential)]
    struct Limits {
        public long processTime,jobTime;
        public uint flags;
        public UIntPtr minSet,maxSet;
        public uint active;
        public UIntPtr affinity;
        public uint priority,scheduling;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct ExtendedLimits {
        public Limits basic;
        public ulong reads,writes,other,readBytes,writeBytes,otherBytes;
        public UIntPtr processMemory,jobMemory,peakProcess,peakJob;
    }
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern EngineJob CreateJobObject(IntPtr attributes,string name);
    [DllImport("kernel32.dll")] static extern bool SetInformationJobObject(EngineJob job,int info,ref ExtendedLimits limits,uint size);
    [DllImport("kernel32.dll")] static extern bool AssignProcessToJobObject(EngineJob job,IntPtr process);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr obj);
    protected override bool ReleaseHandle(){return CloseHandle(handle);}
    internal static EngineJob Attach(Process process){
        var job=CreateJobObject(IntPtr.Zero,null);
        var limits=new ExtendedLimits();limits.basic.flags=0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if(!job.IsInvalid && SetInformationJobObject(job,9,ref limits,(uint)Marshal.SizeOf(typeof(ExtendedLimits))) &&
           AssignProcessToJobObject(job,process.Handle))return job;
        job.Dispose();return null;
    }
}
