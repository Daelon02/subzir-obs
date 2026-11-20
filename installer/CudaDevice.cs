using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;

[assembly: DefaultDllImportSearchPaths(DllImportSearchPath.System32)]

// Shared by setup and the launcher so CUDA0 in llama.cpp is the chosen card.
sealed class CudaDevice {
    internal enum Limitation { None, Driver, Memory, Architecture }
    internal readonly Limitation Unavailable;
    internal const string Cuda12 = "3080", Cuda13 = "5070-Ti";
    const int MajorAttribute = 75, MinorAttribute = 76;
    internal readonly string Name, Uuid, Edition;
    internal readonly ulong Memory;
    internal CudaDevice(string name,string uuid,ulong bytes,int major,int minor,int driver) {
        Name=name; Uuid=uuid; Memory=bytes;
        // The 7B weights alone occupy 4.62 GB. The runtime also needs working memory.
        string edition=major==8 && (minor==6 || minor==9)?Cuda12:major==12 && minor==0?Cuda13:null;
        if(edition==null)Unavailable=Limitation.Architecture;
        else if(bytes < 5UL*1024*1024*1024)Unavailable=Limitation.Memory;
        else if(driver<(edition==Cuda13?13000:12000))Unavailable=Limitation.Driver;
        else Edition=edition;
    }
    internal static CudaDevice Best(IEnumerable<CudaDevice> devices,string edition) {
        return devices.Where(d=>d.Edition!=null && (edition==null || d.Edition==edition))
            .OrderByDescending(d=>d.Memory).ThenByDescending(d=>d.Edition==Cuda13).FirstOrDefault();
    }
    static class Driver {
        const string Library="nvcuda.dll";
        [DllImport(Library)] internal static extern int cuInit(uint flags);
        [DllImport(Library)] internal static extern int cuDriverGetVersion(out int version);
        [DllImport(Library)] internal static extern int cuDeviceGetCount(out int count);
        [DllImport(Library)] internal static extern int cuDeviceGet(out int device,int ordinal);
        [DllImport(Library)] internal static extern int cuDeviceGetAttribute(out int value,int attribute,int device);
        [DllImport(Library)] internal static extern int cuDeviceTotalMem_v2(out UIntPtr bytes,int device);
        [DllImport(Library,ExactSpelling=true,CharSet=CharSet.Ansi)] internal static extern int cuDeviceGetName(StringBuilder name,int size,int device);
        [DllImport(Library)] internal static extern int cuDeviceGetUuid([Out] byte[] uuid,int device);
    }
    internal static CudaDevice[] Detect() {
        var devices=new List<CudaDevice>();
        try {
            int count,version;
            if(Driver.cuInit(0)!=0 || Driver.cuDriverGetVersion(out version)!=0 || Driver.cuDeviceGetCount(out count)!=0)
                return devices.ToArray();
            for(int i=0;i<count;i++) {
                int dev,major,minor; UIntPtr bytes;
                var name=new StringBuilder(256);var uuid=new byte[16];
                if(Driver.cuDeviceGet(out dev,i)!=0 || Driver.cuDeviceGetAttribute(out major,MajorAttribute,dev)!=0 ||
                   Driver.cuDeviceGetAttribute(out minor,MinorAttribute,dev)!=0 || Driver.cuDeviceTotalMem_v2(out bytes,dev)!=0 ||
                   Driver.cuDeviceGetName(name,name.Capacity,dev)!=0 || Driver.cuDeviceGetUuid(uuid,dev)!=0)continue;
                string hex=BitConverter.ToString(uuid).Replace("-","").ToLowerInvariant();
                devices.Add(new CudaDevice(name.ToString(),"GPU-"+new Guid(hex).ToString("D"),bytes.ToUInt64(),major,minor,version));
            }
        } catch(DllNotFoundException) {} catch(EntryPointNotFoundException) {} catch(BadImageFormatException) {}
        return devices.ToArray();
    }
}
