using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net;
using System.Reflection;
using System.Threading;

// ZipArchive seeks across volumes, including for ZIP64 central-directory offsets.
sealed class Volumes : Stream {
    readonly FileStream[] files;
    readonly long[] ends;
    long position;
    public Volumes(string[] paths) {
        files=new FileStream[paths.Length]; ends=new long[paths.Length];
        try {
            long end=0;
            for(int i=0;i<paths.Length;i++) {
                files[i]=File.OpenRead(paths[i]); end+=files[i].Length; ends[i]=end;
            }
        } catch { Dispose(); throw; }
    }
    public override int Read(byte[] buffer,int offset,int count) {
        if(position>=Length || count==0)return 0;
        int i=Array.FindIndex(ends,end=>position<end);
        files[i].Position=position-(i==0?0:ends[i-1]);
        int n=files[i].Read(buffer,offset,(int)Math.Min(count,ends[i]-position));
        position+=n; return n;
    }
    public override long Seek(long offset,SeekOrigin origin) {
        long next=checked((origin==SeekOrigin.Begin?0:origin==SeekOrigin.End?Length:position)+offset);
        if(next<0)throw new IOException("Invalid ZIP offset.");
        return position=next;
    }
    public override long Length { get{return ends[ends.Length-1];} }
    public override long Position { get{return position;} set{Seek(value,SeekOrigin.Begin);} }
    public override bool CanRead { get{return true;} }
    public override bool CanSeek { get{return true;} }
    public override bool CanWrite { get{return false;} }
    public override void Flush() {}
    public override void SetLength(long n){throw new NotSupportedException();}
    public override void Write(byte[] b,int o,int n){throw new NotSupportedException();}
    protected override void Dispose(bool disposing){if(disposing)foreach(var f in files)if(f!=null)f.Dispose();base.Dispose(disposing);}
}

static class Install {
    internal class Part { public string name{get;set;} public string sha256{get;set;} public long size{get;set;} }
    internal class Manifest { public string baseline{get;set;} public string launcher{get;set;} public Dictionary<string,Part[]> editions{get;set;} }
    internal static readonly Manifest Base=Channel.Json.Deserialize<Manifest>(Channel.Resource("baseline"));
    internal static string Destination(string selected,bool updating) {
        try {
            string root=Path.GetPathRoot(selected);
            if(String.IsNullOrEmpty(root) || root.Length<3)return "";
            string full=Path.GetFullPath(selected);
            root=Path.GetPathRoot(full);
            string dir=full.Length>root.Length?full.TrimEnd(Path.DirectorySeparatorChar):full;
            if(updating || String.Equals(Path.GetFileName(dir),"SubZir",StringComparison.OrdinalIgnoreCase) || Existing(dir))return dir;
            return Path.Combine(dir,"SubZir");
        } catch(ArgumentException){return "";}
        catch(NotSupportedException){return "";}
        catch(IOException){return "";}
        catch(UnauthorizedAccessException){return "";}
    }
    internal static bool Existing(string dir) {
        string file=Path.Combine(dir,"SubZir.exe");
        if(!File.Exists(file) || !File.Exists(Path.Combine(dir,@"app\subtitle-rs.exe")) || !File.Exists(Path.Combine(dir,@"app\worker.py")))return false;
        if(Channel.HashFile(file)==Base.launcher)return true;
        var info=FileVersionInfo.GetVersionInfo(file);
        return info.ProductName=="SubZir" && info.Comments==Channel.BaselineId;
    }
    internal static void Upgrade(string dir) {
        if(!Existing(dir))throw new InvalidDataException("Select a SubZir v0.4.0 or newer installation.");
        var target=Path.Combine(dir,"SubZir.exe");
        if(AssemblyName.GetAssemblyName(target).Version>Channel.Version)throw new IOException("A newer SubZir is already installed.");
        using(var mutex=new Mutex(false,Channel.LauncherMutex)){
            try {if(!mutex.WaitOne(10000))throw new IOException("Close SubZir and retry.");}
            catch(AbandonedMutexException) {}
            string tmp=Path.Combine(dir,".launcher-"+Guid.NewGuid().ToString("N"));
            try {
                using(var input=Assembly.GetExecutingAssembly().GetManifestResourceStream("launcher"))
                using(var dest=new FileStream(tmp,FileMode.CreateNew,FileAccess.Write,FileShare.None))input.CopyTo(dest);
                File.Replace(tmp,target,null);
            } finally {mutex.ReleaseMutex();if(File.Exists(tmp))File.Delete(tmp);}
        }
    }
    internal static void Fetch(Part part,string url,string path,Action<long> progress,CancellationToken cancel) {
        long have=File.Exists(path)?new FileInfo(path).Length:0;
        if(have>part.size){File.Delete(path);have=0;}
        if(have<part.size) {
            var request=Channel.Request(url);
            if(have>0)request.AddRange(have);
            using(cancel.Register(request.Abort))
            using(var response=(HttpWebResponse)request.GetResponse()){
                if(response.StatusCode==HttpStatusCode.OK)have=0;
                else if(response.StatusCode!=HttpStatusCode.PartialContent ||
                    response.Headers["Content-Range"]!="bytes "+have+"-"+(part.size-1)+"/"+part.size)
                    throw new IOException("Unexpected download range.");
                using(var source=response.GetResponseStream())
                using(var file=new FileStream(path,FileMode.OpenOrCreate,FileAccess.Write,FileShare.None)){
                    file.SetLength(have);file.Position=have;
                    byte[] buf=new byte[256*1024];int n;
                    while((n=source.Read(buf,0,buf.Length))!=0){
                        cancel.ThrowIfCancellationRequested();
                        if(have+n>part.size)throw new InvalidDataException(part.name);
                        file.Write(buf,0,n);have+=n;progress(have);
                    }
                }
            }
        }
        if(have!=part.size)throw new EndOfStreamException(part.name);
        cancel.ThrowIfCancellationRequested();
        if(Channel.HashFile(path)!=part.sha256){File.Delete(path);throw new InvalidDataException("SHA-256: "+part.name);}
        progress(have);
    }
    internal static void Fresh(string dir,string edition,string cache,Action<int> progress,CancellationToken cancel) {
        if(File.Exists(dir))throw new IOException("A file already exists at "+dir);
        if(Directory.Exists(dir) && Directory.EnumerateFileSystemEntries(dir).Any())throw new IOException("Choose an empty folder or your existing SubZir folder.");
        var parent=Directory.GetParent(dir);
        if(parent==null)throw new IOException("Select an application folder.");
        var parts=Base.editions[edition];
        Directory.CreateDirectory(cache);
        long complete=0,total=parts.Sum(p=>p.size);
        string[] paths=parts.Select(p=>Path.Combine(cache,p.name)).ToArray();
        for(int i=0;i<parts.Length;i++){
            Fetch(parts[i],Channel.DownloadUrl(Base.baseline,parts[i].name),paths[i],
                n=>progress((int)((complete+n)*80/total)),cancel);
            complete+=parts[i].size;
        }
        Directory.CreateDirectory(parent.FullName);
        string stage=Path.Combine(parent.FullName,".subzir-"+Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(stage);
        try {
            using(var volumes=new Volumes(paths))
            using(var zip=new ZipArchive(volumes,ZipArchiveMode.Read)) {
                long bytes=0,expanded=zip.Entries.Sum(e=>e.Length);
                byte[] buf=new byte[256*1024];
                foreach(var entry in zip.Entries) {
                    cancel.ThrowIfCancellationRequested();
                    if(!entry.FullName.StartsWith("SubZir/",StringComparison.Ordinal) || entry.FullName.Contains(":"))throw new InvalidDataException(entry.FullName);
                    string path=Path.GetFullPath(Path.Combine(stage,entry.FullName));
                    if(!path.StartsWith(Path.Combine(stage,"SubZir")+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase))throw new InvalidDataException(path);
                    if(entry.Name.Length==0){Directory.CreateDirectory(path);continue;}
                    Directory.CreateDirectory(Path.GetDirectoryName(path));
                    using(var input=entry.Open())
                    using(var output=new FileStream(path,FileMode.CreateNew,FileAccess.Write,FileShare.None)){
                        int n;while((n=input.Read(buf,0,buf.Length))!=0){
                            cancel.ThrowIfCancellationRequested();output.Write(buf,0,n);bytes+=n;progress(80+(int)(bytes*19/expanded));
                        }
                    }
                }
            }
            string app=Path.Combine(stage,"SubZir");
            Upgrade(app);
            cancel.ThrowIfCancellationRequested();
            if(Directory.Exists(dir))Directory.Delete(dir,false);
            Directory.Move(app,dir);
        } finally {Directory.Delete(stage,true);}
        foreach(var path in paths)try{File.Delete(path);}catch(IOException){}catch(UnauthorizedAccessException){}
        progress(100);
    }
}
