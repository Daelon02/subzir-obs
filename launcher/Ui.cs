using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Web.Script.Serialization;

enum Msg { Window, Title, Subtitle, Connect, ConnectHint, Test, TestHint, SelectArea, SelectHint, Start, Guide, ConnectFirst, AreaReady, Help, ConnectTitle, Stopped, Loading, Checking, Active, TestSuccess, TestNoAck, ResponseError, Waiting, Stop, OcrMissing, CheckCamera, TestFailed, TestEmpty, StartError, GettingFrame, NoFrame, Cancelled, Saved, FrameError, AlreadyOpen, LaunchError, Picker, Save, Cancel, Language, Interface, Target, Apply, ChooseLanguages, SettingsError, GuideBody, AddScene, Model, Automatic, Appearance, Font, Background, Opacity, Outline, ModelError }

static class Ui {
    static readonly string Root = AppDomain.CurrentDomain.BaseDirectory;
    static readonly string Settings = Path.Combine(Root, "runtime", "languages.json");
    static readonly JavaScriptSerializer Json = new JavaScriptSerializer();
    static readonly Dictionary<string,string> English;
    static Dictionary<string,string> messages;
    public static readonly Dictionary<string,string> Names;
    public static string Language { get; private set; }
    public static string Target { get; private set; }
    public static string Model { get; private set; }
    static readonly Dictionary<string,string> HyLanguages;
    const string CustomPrefix = "gguf:";
    static readonly string CustomDirectory = Path.Combine(Root,"app","models");
    public static bool IsGpuModel(string model) {
        return model=="hy" || (model!=null && model.StartsWith(CustomPrefix,StringComparison.Ordinal));
    }
    public static IEnumerable<KeyValuePair<string,string>> CustomModels(string target) {
        if(!HyLanguages.ContainsKey(target) || !File.Exists(Path.Combine(Root,"app","hy-runtime","llama-server.exe")))
            yield break;
        string[] paths;
        try { paths=Directory.GetFiles(CustomDirectory,"*.gguf"); }
        catch(IOException) { yield break; }
        catch(UnauthorizedAccessException) { yield break; }
        foreach(string path in paths.OrderBy(p=>p,StringComparer.OrdinalIgnoreCase)) {
            if(String.Equals(Path.GetExtension(path),".gguf",StringComparison.OrdinalIgnoreCase))
                yield return new KeyValuePair<string,string>(CustomPrefix+Path.GetFileName(path),Path.GetFileName(path)+" (GPU)");
        }
    }
    public static bool Allows(string model,string target) {
        switch(model) {
            case "auto": return true;
            case "m2m100": return target!="en";
            case "quickmt": return target=="uk";
            case "hy": return HyLanguages.ContainsKey(target) &&
                File.Exists(Path.Combine(Root,"app","hy-runtime","llama-server.exe")) &&
                File.Exists(Path.Combine(Root,"app","hy-runtime","Hy-MT2-1.8B-Q6_K.gguf"));
            default: return IsGpuModel(model) && CustomModels(target).Any(pair=>pair.Key==model);
        }
    }
    public static bool IsRtl { get { return new[]{"ar","fa","he","ps","sd","ur","yi"}.Contains(Language); } }

    static Ui() {
        using(var reader = new StreamReader(typeof(Ui).Assembly.GetManifestResourceStream("SubZir.English.json"))) {
            English = Json.Deserialize<Dictionary<string,string>>(reader.ReadToEnd());
        }
        Names = Read(Path.Combine(Root,"app","languages.json")) ?? new Dictionary<string,string>{{"en","English"}};
        var prefs = Read(Settings) ?? new Dictionary<string,string>();
        string ui, target;
        if (!prefs.TryGetValue("ui",out ui) || !Names.ContainsKey(ui ?? "")) {
            ui = CultureInfo.CurrentUICulture.TwoLetterISOLanguageName;
        }
        Language = Names.ContainsKey(ui) ? ui : "en";
        Target = prefs.TryGetValue("target",out target) && Names.ContainsKey(target ?? "") ? target : "uk";
        HyLanguages = Read(Path.Combine(Root,"app","hy-languages.json")) ?? new Dictionary<string,string>();
        string model;
        Model = prefs.TryGetValue("model",out model) && Allows(model,Target) ? model : "auto";
        messages = Load(Language);
    }
    static Dictionary<string,string> Read(string path) {
        try { return Json.Deserialize<Dictionary<string,string>>(File.ReadAllText(path, Encoding.UTF8)); }
        catch(IOException) { return null; }
        catch(UnauthorizedAccessException) { return null; }
        catch(ArgumentException) { return null; }
        catch(InvalidOperationException) { return null; }
    }
    static Dictionary<string,string> Load(string code) {
        return Read(Path.Combine(Root,"app","locales",code + ".json")) ?? English;
    }
    public static void Save(string ui,string target,string model) {
        if(!Names.ContainsKey(ui) || !Names.ContainsKey(target) || !Allows(model,target)) throw new ArgumentException();
        WriteJson(Settings,new {ui=ui,target=target,model=model});
        Language=ui; Target=target; Model=model; messages=Load(ui);
    }
    public static void WriteJson(string path,object value) {
        Directory.CreateDirectory(Path.GetDirectoryName(path));
        string temp = path + "." + Guid.NewGuid().ToString("N");
        try {
            File.WriteAllText(temp,Json.Serialize(value),new UTF8Encoding(false));
            if(File.Exists(path)) File.Replace(temp,path,null);
            else File.Move(temp,path);
        } finally { if(File.Exists(temp)) File.Delete(temp); }
    }
    public static string Get(Msg key) {
        string value;
        return messages.TryGetValue(key.ToString(),out value) && !String.IsNullOrWhiteSpace(value) ? value : English[key.ToString()];
    }
}
