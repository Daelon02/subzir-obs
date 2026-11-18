using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Web.Script.Serialization;

enum Msg { Window, Title, Subtitle, Connect, ConnectHint, Test, TestHint, SelectArea, SelectHint, Start, Guide, ConnectFirst, AreaReady, Help, ConnectTitle, Stopped, Loading, Checking, Active, TestSuccess, TestNoAck, ResponseError, Waiting, Stop, OcrMissing, CheckCamera, TestFailed, TestEmpty, StartError, GettingFrame, NoFrame, Cancelled, Saved, FrameError, AlreadyOpen, LaunchError, Picker, Save, Cancel, Language, Interface, Target, Apply, ChooseLanguages, SettingsError, GuideBody, AddScene }

static class Ui {
    static readonly string Root = AppDomain.CurrentDomain.BaseDirectory;
    static readonly string Settings = Path.Combine(Root, "runtime", "languages.json");
    static readonly JavaScriptSerializer Json = new JavaScriptSerializer();
    static readonly Dictionary<string,string> English;
    static Dictionary<string,string> messages;
    public static readonly Dictionary<string,string> Names;
    public static string Language { get; private set; }
    public static string Target { get; private set; }
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
    public static void Save(string ui,string target) {
        if(!Names.ContainsKey(ui) || !Names.ContainsKey(target)) throw new ArgumentException();
        Directory.CreateDirectory(Path.GetDirectoryName(Settings));
        string temp = Settings + "." + Guid.NewGuid().ToString("N");
        try {
            File.WriteAllText(temp,Json.Serialize(new { ui=ui, target=target }),new UTF8Encoding(false));
            if(File.Exists(Settings)) File.Replace(temp,Settings,null);
            else File.Move(temp,Settings);
        } finally { if(File.Exists(temp)) File.Delete(temp); }
        Language=ui; Target=target; messages=Load(ui);
    }
    public static string Get(Msg key) {
        string value;
        return messages.TryGetValue(key.ToString(),out value) && !String.IsNullOrWhiteSpace(value) ? value : English[key.ToString()];
    }
}
