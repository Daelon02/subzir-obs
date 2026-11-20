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
            using(var photo=new Bitmap(path))using(var editor=new Editor(photo,ZonesForEditor())){
                if(editor.ShowDialog(this)!=DialogResult.OK){SetStatus(Msg.Cancelled);return;}
                try {SaveZones(editor.LayoutZones,video);}
                catch(IOException e){SetStatus(Msg.CaptureSaveError,e.Message);return;}
                catch(UnauthorizedAccessException e){SetStatus(Msg.CaptureSaveError,e.Message);return;}
                SetStatus(Msg.Saved," · "+(video==null?"OBS":Path.GetFileName(video)));
            }
        }catch(Exception e){if(!closing)SetStatus(Msg.FrameError,e.Message);}
        finally{running=null;try{File.Delete(path);}catch(IOException){}if(!closing)Lock(false);}
    }
    string PublishZones(object values){
        string marker;
        return WriteZones(values,out marker);
    }
    static string ReadMarker(string path){
        try{
            using(var file=new FileStream(path,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete))
            using(var reader=new StreamReader(file))return reader.ReadLine();
        }catch(IOException){return null;}
    }
    async Task ShowZoneTest(string output){
        var row=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(output);
        string marker;
        line.Text=WriteZones(row["captions"],out marker);
        string captionFile=Path.Combine(Data,"caption.txt");
        try{
            if(String.IsNullOrWhiteSpace(line.Text)){SetStatus(Msg.TestEmpty);return;}
            string ack=Path.Combine(Data,"ack.txt");
            var elapsed=Stopwatch.StartNew();
            bool shown=false;
            while(!closing && elapsed.ElapsedMilliseconds<1000){
                shown=ReadMarker(ack)==marker;
                if(shown)break;
                await Task.Delay(40);
            }
            if(closing)return;
            SetStatus(shown?Msg.TestSuccess:Msg.TestNoAck);
            await Task.Delay(4000);
        }finally{
            if(!closing && ReadMarker(captionFile)==marker)
                Ui.WriteText(captionFile,"stop-"+DateTime.UtcNow.Ticks+"\n");
        }
    }
    string WriteZones(object values,out string marker){
        var captions=(System.Collections.IList)values;
        if(layout==null || captions.Count!=layout.Length)throw new InvalidDataException("Zone count mismatch");
        int next=1-imageSlot;
        var display=new List<string>();
        var legends=new LegendSnapshot[layout.Length];
        var occupied=new List<RectangleF>();
        var json=new JavaScriptSerializer();
        for(int i=0;i<layout.Length;i++){
            if(!layout[i].UsesEntries || captions[i]==null)continue;
            legends[i]=json.ConvertToType<LegendSnapshot>(captions[i]);
            LegendOverlay.Protect(layout[i],legends[i],occupied);
            if(layout[i].Output!=null && legends[i].rows.Length>0)occupied.Add(layout[i].Output.Bounds());
        }
        string path=Path.Combine(Data,"zones-"+next+".png");
        string tmp=path+"."+Guid.NewGuid().ToString("N");
        try {
            var style=SubtitleStyle.Load();
            using(var bmp=new Bitmap(1280,720,PixelFormat.Format32bppArgb))
            using(var g=Graphics.FromImage(bmp)){
                for(int i=0;i<layout.Length;i++){
                    string text=captions[i] as string;
                    if(!String.IsNullOrEmpty(text)){
                        occupied.Add(Stage.DrawCaption(g,layout[i],text,style));display.Add(text);
                    }
                }
                for(int i=0;i<layout.Length;i++){
                    if(legends[i]!=null)LegendOverlay.Draw(g,layout[i],legends[i],style,occupied,display);
                }
                bmp.Save(tmp,ImageFormat.Png);
            }
            ReplaceFile(tmp,path);
            marker="zones-"+next+"-"+Guid.NewGuid().ToString("N");
            File.WriteAllText(tmp,marker+"\n",Utf8);
            ReplaceFile(tmp,Path.Combine(Data,"caption.txt"));imageSlot=next;
        }finally{if(File.Exists(tmp))File.Delete(tmp);}
        return String.Join(Environment.NewLine,display);
    }
    static void ReplaceFile(string from,string to){
        if(File.Exists(to))File.Replace(from,to,null);else File.Move(from,to);
    }
}
