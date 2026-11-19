using System;
using System.IO;
using System.Net;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text.RegularExpressions;
using System.Web.Script.Serialization;

internal static class Channel {
    internal const string SetupName = "SubZir-Setup.exe";
    internal const string LauncherMutex = @"Local\PS5QuickMTLauncher";
    internal const string BaselineId = "SubZir/0.4.0";
    internal static readonly Version Version = Assembly.GetExecutingAssembly().GetName().Version;
    internal static readonly JavaScriptSerializer Json = new JavaScriptSerializer();
    internal static readonly Config Settings = Json.Deserialize<Config>(Resource("channel"));
    internal class Config { public string repository {get;set;} public string latest {get;set;} }
    internal class Asset { public string name {get;set;} public string digest {get;set;} public long size {get;set;} public string browser_download_url {get;set;} }
    internal class Release { public string tag_name {get;set;} public bool draft {get;set;} public bool prerelease {get;set;} public Asset[] assets {get;set;} }
    internal static string Resource(string name) {
        using(var r=new StreamReader(Assembly.GetExecutingAssembly().GetManifestResourceStream(name))) return r.ReadToEnd();
    }
    internal static string DownloadUrl(string tag,string file) {
        return Settings.repository.TrimEnd('/')+"/releases/download/"+Uri.EscapeDataString(tag)+"/"+Uri.EscapeDataString(file);
    }
    internal static HttpWebRequest Request(string url) {
        var r=(HttpWebRequest)WebRequest.Create(url);
        r.UserAgent="SubZir/"+Version.ToString(3); r.Timeout=45000; r.ReadWriteTimeout=60000;
        return r;
    }
    internal static string Hash(Stream stream) {
        using(var sha=SHA256.Create())return BitConverter.ToString(sha.ComputeHash(stream)).Replace("-","").ToLowerInvariant();
    }
    internal static string HashFile(string file) { using(var f=File.OpenRead(file))return Hash(f); }
    internal static Asset Newer(Release release) {
        Version next;
        if(release==null || release.draft || release.prerelease || release.tag_name==null ||
            !Regex.IsMatch(release.tag_name,@"^v\d+\.\d+\.\d+$") || !System.Version.TryParse(release.tag_name.Substring(1)+".0",out next))
            throw new InvalidDataException("Invalid stable release.");
        if(next<=Version)return null;
        var asset=(release.assets??new Asset[0]).SingleOrDefault(a=>a.name==SetupName);
        if(asset==null || asset.size<=0 || !Regex.IsMatch(asset.digest??"", "^sha256:[0-9a-f]{64}$") ||
            asset.browser_download_url!=DownloadUrl(release.tag_name,SetupName))
            throw new InvalidDataException("Missing verified "+SetupName+" ("+release.tag_name+").");
        return asset;
    }
}
