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
        var recovery=new Timer{Interval=750};
        recovery.Tick+=delegate{
            if(nativeValues==null)return;
            try{
                if(NativeAvailable())return;
                if(ReadMarker(Path.Combine(Data,"caption.txt"))==nativeMarker && layout==nativeLayout){
                    fallbackFrom=nativeMarker;WriteZones(nativeValues,out fallbackTo);
                }
                nativeValues=null;
            }catch(IOException){}catch(UnauthorizedAccessException){}
        };
        Shown+=delegate{recovery.Start();};
        Disposed+=delegate{recovery.Dispose();};
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
                if(marker==fallbackFrom)marker=fallbackTo;
                shown=ReadMarker(ack)==marker;
                if(shown)break;
                await Task.Delay(40);
            }
            if(closing)return;
            SetStatus(shown?Msg.TestSuccess:Msg.TestNoAck);
            await Task.Delay(4000);
        }finally{
            if(marker==fallbackFrom)marker=fallbackTo;
            if(!closing && ReadMarker(captionFile)==marker)
                Ui.WriteText(captionFile,"stop-"+DateTime.UtcNow.Ticks+"\n");
        }
    }
    string WriteZones(object values,out string marker){
        var captions=(System.Collections.IList)values;
        if(layout==null || captions.Count!=layout.Length)throw new InvalidDataException("Zone count mismatch");
        int next=1-imageSlot;
        var style=SubtitleStyle.Load();
        var captionFile=Path.Combine(Data,"caption.txt");
        try{
            // A multiline neighbour can force a bitmap. Keep visible HUD and streamed
            // dialogue on one renderer throughout their lifetimes.
            if(layout.Length==1 && !layout[0].Hud && !layout[0].UsesEntries &&
                captions[0] is string && NativeAvailable()){
                string nativeDisplay;
                if(TryWriteLayers(values,captions,style,next,captionFile,out marker,out nativeDisplay))
                    return nativeDisplay;
            }
        }catch(IOException){}catch(UnauthorizedAccessException){}
        nativeValues=null;
        string path=Path.Combine(Data,"zones-"+next+".png");
        string tmp=path+"."+Guid.NewGuid().ToString("N");
        try {
            string display;
            using(var bmp=new Bitmap(1280,720,PixelFormat.Format32bppArgb))
            using(var g=Graphics.FromImage(bmp)){
                display=DrawZones(g,captions,style,null);
                bmp.Save(tmp,ImageFormat.Png);
            }
            ReplaceFile(tmp,path);
            marker="zones-"+next+"-"+Guid.NewGuid().ToString("N");
            Ui.WriteText(captionFile,marker+"\n");imageSlot=next;
            return display;
        }finally{if(File.Exists(tmp))File.Delete(tmp);}
    }
    bool TryWriteLayers(object values,System.Collections.IList captions,SubtitleStyle style,int next,
                        string captionFile,out string marker,out string display){
        var frame=new NativeFrame(Data,backdrops);
        using(var metric=new Bitmap(1,1))
        using(var g=Graphics.FromImage(metric))display=DrawZones(g,captions,style,frame);
        marker="layers-"+next+"-"+Guid.NewGuid().ToString("N");
        if(!frame.Fits)return false;
        var data=Channel.Json.Serialize(new {id=marker,layers=frame.Layers});
        if(Utf8.GetByteCount(data)>256*1024)return false;
        NativeFrame.Publish(Path.Combine(Data,"layers-"+next+".json"),data);
        NativeFrame.Publish(captionFile,marker+"\n");
        nativeValues=values;nativeLayout=layout;nativeMarker=marker;imageSlot=next;
        return true;
    }
    readonly Dictionary<int,NativeFrame.Backdrop> backdrops=new Dictionary<int,NativeFrame.Backdrop>();
    object nativeValues;
    Zone[] nativeLayout;
    string nativeMarker,fallbackFrom,fallbackTo;
    static bool NativeAvailable(){
        var path=Path.Combine(Data,"obs-layers-v1.txt");
        return DateTime.UtcNow-File.GetLastWriteTimeUtc(path)<TimeSpan.FromSeconds(4) && ReadMarker(path)=="ready";
    }
    string DrawZones(Graphics g,System.Collections.IList captions,SubtitleStyle style,NativeFrame frame){
        var display=new List<string>();
        var legends=new LegendSnapshot[layout.Length];
        var occupied=new List<RectangleF>();
        for(int i=0;i<layout.Length;i++){
            if(!layout[i].UsesEntries || captions[i]==null)continue;
            legends[i]=Channel.Json.ConvertToType<LegendSnapshot>(captions[i]);
            LegendOverlay.Protect(layout[i],legends[i],occupied);
            if(layout[i].Output!=null && legends[i].rows.Length>0)occupied.Add(layout[i].Output.Bounds());
        }
        for(int i=0;i<layout.Length;i++){
            if(frame!=null)frame.ZoneIndex=i;
            string text=captions[i] as string;
            var sentences=captions[i] as System.Collections.IDictionary;
            if(!layout[i].UsesEntries && sentences!=null && sentences["first"] is string){
                string first=(string)sentences["first"],rest=sentences["rest"] as string;
                occupied.Add(Stage.DrawSentences(g,layout[i],first,rest,style));
                display.Add(String.IsNullOrEmpty(rest)?first:first+" "+rest);
                continue;
            }
            if(!String.IsNullOrEmpty(text)){
                occupied.Add(Stage.DrawCaption(g,layout[i],text,style,frame));display.Add(text);
            }
        }
        for(int i=0;i<layout.Length;i++){
            if(frame!=null)frame.ZoneIndex=i;
            if(legends[i]!=null)LegendOverlay.Draw(g,layout[i],legends[i],style,occupied,display,frame);
        }
        return String.Join(Environment.NewLine,display);
    }
    static void ReplaceFile(string from,string to){
        if(File.Exists(to))File.Replace(from,to,null);else File.Move(from,to);
    }
}
