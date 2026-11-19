using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;

partial class Launcher {
    static readonly string ZoneFile=Path.Combine(Data,"zones.json");
    readonly Button editZones=new Button(), editRecording=new Button();
    Zone[] layout;
    string recording;
    int imageSlot;
    void ZoneCaptions(){
        editZones.Text=Editor.T("Редактор зон · OBS","Zone editor · OBS");
        editRecording.Text=Editor.T("Редактор зон · відеофайл","Zone editor · video file");
    }
    void ZoneButtons(){
        editZones.SetBounds(26,581,300,36);editRecording.SetBounds(342,581,340,36);
        Controls.Add(editZones);Controls.Add(editRecording);
        editZones.Click+=async delegate{await EditZones(null);};
        editRecording.Click+=async delegate{
            using(var pick=new OpenFileDialog{Filter="Video|*.mkv;*.mp4;*.mov;*.avi;*.webm"}){
                if(pick.ShowDialog(this)==DialogResult.OK)await EditZones(pick.FileName);
            }
        };
        if(File.Exists(ZoneFile))try{
            var saved=new JavaScriptSerializer().Deserialize<Zone[]>(File.ReadAllText(ZoneFile));
            if(saved!=null && saved.Length>=1 && saved.Length<=8 && saved.All(z=>z!=null && z.Valid()))layout=saved;
        }catch(Exception e){MessageBox.Show(this,e.Message,"SubZir");}
    }
    async Task EditZones(string video){
        if(busy)return;
        Lock(true);SetStatus(Msg.GettingFrame);
        string path=Path.Combine(Data,"zones-"+Guid.NewGuid().ToString("N")+".bmp");
        try{
            var input=video==null?"-f dshow -i "+Quote("video=OBS Virtual Camera"):"-i "+Quote(video);
            var args="-nostdin -loglevel error "+input+" -vf scale=1280:720 -frames:v 1 -threads:v 1 "+Quote(path);
            using(var p=new Process{StartInfo=Info(Path.Combine(App,"bin","ffmpeg.exe"),args)}){
                p.Start();running=p;p.BeginOutputReadLine();p.BeginErrorReadLine();
                if(!await Task.Run(()=>p.WaitForExit(12000))){p.Kill();p.WaitForExit();}
                running=null;if(closing)return;
                if(p.ExitCode!=0 || !File.Exists(path)){SetStatus(Msg.NoFrame);return;}
            }
            var json=new JavaScriptSerializer();
            var initial=layout==null?new[]{new Zone(Editor.T("Діалог","Dialogue"),Editor.T("Тримайся поруч.","Stay close."),.23f,.80f,.54f)}:
                json.Deserialize<Zone[]>(json.Serialize(layout));
            using(var photo=new Bitmap(path))using(var editor=new Editor(photo,initial)){
                if(editor.ShowDialog(this)!=DialogResult.OK){SetStatus(Msg.Cancelled);return;}
                var zones=editor.LayoutZones;Ui.WriteJson(ZoneFile,zones);layout=zones;recording=video;
                SetStatus(Msg.Saved," · "+(video==null?"OBS":Path.GetFileName(video)));
            }
        }catch(Exception e){if(!closing)SetStatus(Msg.FrameError,e.Message);}
        finally{running=null;try{File.Delete(path);}catch(IOException){}if(!closing)Lock(false);}
    }
    string PublishZones(object values){
        var captions=(System.Collections.IList)values;
        if(layout==null || captions.Count!=layout.Length)throw new InvalidDataException("Zone count mismatch");
        int next=1-imageSlot;
        string path=Path.Combine(Data,"zones-"+next+".png");
        string tmp=path+"."+Guid.NewGuid().ToString("N");
        try {
            var style=SubtitleStyle.Load();
            using(var bmp=new Bitmap(1280,720,PixelFormat.Format32bppArgb))
            using(var g=Graphics.FromImage(bmp)){
                for(int i=0;i<layout.Length;i++){
                    string text=captions[i] as string;
                    if(!String.IsNullOrEmpty(text))Stage.DrawCaption(g,layout[i],text,style);
                }
                bmp.Save(tmp,ImageFormat.Png);
            }
            ReplaceFile(tmp,path);
            File.WriteAllText(tmp,"zones-"+next+"-"+Guid.NewGuid().ToString("N")+"\n",Utf8);
            ReplaceFile(tmp,Path.Combine(Data,"caption.txt"));imageSlot=next;
        }finally{if(File.Exists(tmp))File.Delete(tmp);}
        return String.Join(Environment.NewLine,captions.Cast<string>().Where(text=>!String.IsNullOrEmpty(text)));
    }
    static void ReplaceFile(string from,string to){
        if(File.Exists(to))File.Replace(from,to,null);else File.Move(from,to);
    }
}
