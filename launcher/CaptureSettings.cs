using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Web.Script.Serialization;

partial class Launcher {
    Zone[] savedZones;
    bool preserveZoneFile;

    Zone[] ZonesForEditor() {
        if(savedZones==null)return new[]{new Zone(Editor.T("Діалог","Dialogue"),Editor.T("Тримайся поруч.","Stay close."),.23f,.80f,.54f)};
        var json=new JavaScriptSerializer();
        return json.Deserialize<Zone[]>(json.Serialize(savedZones));
    }

    static bool ValidCrop(string text) {
        var fields=text.Split(':');
        if(fields.Length!=4)return false;
        var n=new int[4];
        for(int i=0;i<n.Length;i++)
            if(!Int32.TryParse(fields[i],NumberStyles.None,CultureInfo.InvariantCulture,out n[i]))return false;
        return n[0]>0 && n[1]>0 && n[2]<=Int32.MaxValue-n[0] && n[3]<=Int32.MaxValue-n[1];
    }
    static string ReadCrop(string path) {
        if(!File.Exists(path))return null;
        var text=File.ReadAllText(path).Trim();
        return ValidCrop(text)?text:null;
    }
    static string ZoneBackup() {
        return Path.Combine(Data,"zones-original-"+Guid.NewGuid().ToString("N")+".json");
    }
    static bool InvalidProfile(Exception e) {
        return e is ArgumentException || e is InvalidOperationException || e is FormatException
            || e is OverflowException || e is InvalidCastException;
    }
    Zone[] ReadZones(List<string> notices) {
        if(!File.Exists(ZoneFile))return null;
        var json=new JavaScriptSerializer();
        var rows=json.DeserializeObject(File.ReadAllText(ZoneFile)) as object[];
        var zones=new List<Zone>();
        bool changed=false;
        if(rows!=null)foreach(var row in rows) {
            try {
                var fields=row as Dictionary<string,object>;
                if(fields==null || zones.Count==8){changed=true;continue;}
                object anchor;
                if(fields.TryGetValue("Anchor",out anchor)) {
                    CaptionAnchor value;
                    if(!Enum.TryParse(Convert.ToString(anchor,CultureInfo.InvariantCulture),true,out value) ||
                       !Enum.IsDefined(typeof(CaptionAnchor),value)) {
                        fields["Anchor"]=(int)CaptionAnchor.Manual;
                        changed=true;
                    }
                }
                var z=json.ConvertToType<Zone>(fields);
                if(z.Valid())zones.Add(z);else changed=true;
            } catch(Exception e) {
                if(!InvalidProfile(e))throw;
                changed=true;
            }
        }
        if(zones.Count==0){
            preserveZoneFile=true;
            notices.Add(Editor.T("Перевірте zones.json у папці runtime.","Check zones.json in the runtime folder."));
            return null;
        }
        var result=zones.ToArray();
        if(changed){
            var backup=ZoneBackup();
            Ui.WriteJson(ZoneFile,result,backup);
            notices.Add(Editor.T("Зони відновлено. Оригінал: ","Zones recovered. Original: ")+Path.GetFileName(backup));
        }
        return result;
    }
    void LoadCaptureSettings() {
        var notices=new List<string>();
        try {
            roi=ReadCrop(CropFile);
            if(roi==null && (File.Exists(CropFile) || File.Exists(PreviousCrop))){
                roi=ReadCrop(PreviousCrop);
                notices.Add(roi==null?Ui.Get(Msg.SelectArea):Editor.T("Попередню область відновлено.","Previous area restored."));
            }
        }catch(IOException e){notices.Add(e.Message);}
        catch(UnauthorizedAccessException e){notices.Add(e.Message);}
        try{savedZones=ReadZones(notices);}
        catch(Exception e){
            if(!InvalidProfile(e) && !(e is IOException) && !(e is UnauthorizedAccessException))throw;
            preserveZoneFile=true;
            notices.Add("zones.json · "+e.Message);
        }
        layout=roi!=null && File.Exists(SingleModeFile)?null:savedZones;
        SetStatus(roi==null && layout==null?Msg.ConnectFirst:Msg.AreaReady,
            notices.Count==0?"":" "+String.Join(" ",notices));
    }
    void SaveArea(Rectangle area) {
        var text=String.Format(CultureInfo.InvariantCulture,"{0}:{1}:{2}:{3}",area.Width,area.Height,area.X,area.Y);
        if(!ValidCrop(text))throw new InvalidDataException(Ui.Get(Msg.SelectArea));
        Ui.WriteText(CropFile,text,ReadCrop(CropFile)==null?null:PreviousCrop);
        roi=text;
        if(!File.Exists(SingleModeFile))Ui.WriteText(SingleModeFile,"single");
        layout=null;
        recording=null;
    }
    void SaveZones(Zone[] zones,string video) {
        if(zones==null || zones.Length<1 || zones.Length>8 || !zones.All(z=>z!=null && z.Valid()))
            throw new InvalidDataException(Editor.T("Перевірте параметри зон.","Check the zone settings."));
        Ui.WriteJson(ZoneFile,zones,preserveZoneFile?ZoneBackup():null);
        preserveZoneFile=false;
        savedZones=zones;
        File.Delete(SingleModeFile);
        layout=zones;
        recording=video;
    }
}
