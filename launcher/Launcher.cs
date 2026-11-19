using System;
using System.Drawing;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using System.Collections.Generic;
using System.Web.Script.Serialization;

partial class Launcher : Form {
    static readonly string Root = AppDomain.CurrentDomain.BaseDirectory;
    static readonly string App = Path.Combine(Root,"app"), Data = Path.Combine(Root,"runtime");
    static readonly string Lua = Path.Combine(Root,"OBS","PS5-Subtitles.lua");
    static readonly UTF8Encoding Utf8 = new UTF8Encoding(false);
    readonly Button connect = new Button(), demo = new Button(), select = new Button(), play = new Button();
    readonly Label status = new Label(), line = new Label();
    readonly Button language = new Button();
    readonly Dictionary<Control,Msg> captions = new Dictionary<Control,Msg>();
    Msg statusMessage;
    string statusDetail = "";
    void Caption(Control control, Msg key) {
        captions[control] = key;
        control.Text = Ui.Get(key);
    }
    void SetStatus(Msg key, string detail = "") {
        statusMessage = key;
        statusDetail = detail;
        status.Text = Ui.Get(key) + detail;
    }
    void RefreshLanguage() {
        UpdateCaption();ZoneCaptions();
        Text = "SubZir — " + Ui.Get(Msg.Window);
        language.Text = Ui.Get(Msg.Language) + (Ui.Language == "en" ? "" : " / Language");
        language.AccessibleName = Ui.Get(Msg.ChooseLanguages);
        RightToLeft = Ui.IsRtl ? RightToLeft.Yes : RightToLeft.No;
        RightToLeftLayout = Ui.IsRtl;
        foreach (var item in captions) item.Key.Text = Ui.Get(item.Value);
        RefreshModels();
        SetStatus(statusMessage, statusDetail);
    }
    readonly ComboBox models = new ComboBox();
    bool refreshingModels;
    void RefreshModels() {
        refreshingModels=true;
        try {
            var options=new Dictionary<string,string>();
            foreach(string code in new[]{"auto","quickmt","m2m100","hy"}) {
                if(Ui.Allows(code,Ui.Target)) options.Add(code,code=="auto"?Ui.Get(Msg.Automatic):
                    code=="hy"?"Hy-MT2 1.8B Q6_K (GPU)":code=="quickmt"?"QuickMT EN → UK (CPU)":"M2M100 (CPU)");
            }
            foreach(var entry in Ui.CustomModels(Ui.Target)) options.Add(entry.Key,entry.Value);
            models.DataSource=new BindingSource(options,null);
            models.SelectedValue=Ui.Model;
            models.AccessibleName=Ui.Get(Msg.Model);
        } finally {refreshingModels=false;}
    }
    Process running;
    EngineJob engineJob;
    bool stopping;
    bool busy, closing;
    string roi;
    static readonly string CropFile = Path.Combine(Data,"area.txt");
    public Launcher() {
        Text=Ui.Get(Msg.Window); Font=new Font("Segoe UI",10); ClientSize=new Size(710,596);
        AutoScaleMode=AutoScaleMode.Dpi; FormBorderStyle=FormBorderStyle.FixedDialog; MaximizeBox=false; StartPosition=FormStartPosition.CenterScreen;
        BackColor=Color.White;
        Label title=LabelAt(Msg.Title,24,18,452,46);title.Font=new Font(Font.FontFamily,22,FontStyle.Bold);
        LabelAt(Msg.Subtitle,26,68,445,41);
        ButtonAt(connect,Msg.Connect,26,116,208);connect.Click+=delegate { HelpOBS(); };
        LabelAt(Msg.ConnectHint,250,114,436,55);
        ButtonAt(demo,Msg.Test,26,173,208);demo.Click+=async delegate {await Start(false);};
        LabelAt(Msg.TestHint,250,174,436,52);
        ButtonAt(select,Msg.SelectArea,26,230,208);select.Click+=async delegate {await Choose();};
        LabelAt(Msg.SelectHint,250,231,436,52);
        ButtonAt(play,Msg.Start,26,289,251);play.BackColor=Color.FromArgb(23,78,103);play.ForeColor=Color.White;
        play.Click+=async delegate {if(running!=null)StopOwned();else await Start(true);};
        Button guide=new Button(); ButtonAt(guide,Msg.Guide,352,289,270);
        guide.Click+=delegate {
            if (Ui.Language != "en" && Ui.Language != "uk") { MessageBox.Show(this,Ui.Get(Msg.GuideBody),Ui.Get(Msg.Guide)); return; }
            string page=Path.Combine(Root,"docs",Ui.Language=="uk"?"PLAYER-UA.html":"PLAYER-EN.html");
            if(File.Exists(page)) Process.Start(new ProcessStartInfo(page){UseShellExecute=true});
            else Process.Start("notepad.exe",Quote(Path.Combine(Root,Ui.Language == "uk" ? "README.txt" : "README.en.txt")));
        };
        status.SetBounds(26,350,652,55);status.ForeColor=Color.FromArgb(35,69,83);Controls.Add(status);
        line.SetBounds(26,409,652,78);line.Font=new Font(Font.FontFamily,12,FontStyle.Bold);Controls.Add(line);
        LabelAt(Msg.Model,26,499,126,29);
        models.DropDownStyle=ComboBoxStyle.DropDownList;
        models.SetBounds(154,496,270,36); models.DisplayMember="Value"; models.ValueMember="Key";
        Controls.Add(models);
        models.SelectionChangeCommitted+=delegate {
            if(refreshingModels)return;
            try {Ui.Save(Ui.Language,Ui.Target,(string)models.SelectedValue);}
            catch(Exception ex) {RefreshModels();MessageBox.Show(this,ex.Message,Ui.Get(Msg.Model));}
        };
        var appearance=new Button();
        ButtonAt(appearance,Msg.Appearance,444,490,238);
        appearance.Click+=delegate {using(var dialog=new StyleDialog())dialog.ShowDialog(this);};
        language.SetBounds(482,21,200,38);
        language.Click += delegate {
            try { using(var dialog = new LanguageDialog(busy && Ui.Model!="auto")) { if(dialog.ShowDialog(this) == DialogResult.OK) { Ui.Save(dialog.InterfaceCode, dialog.TargetCode, Ui.Allows(Ui.Model,dialog.TargetCode)?Ui.Model:"auto"); RefreshLanguage(); } } }
            catch (IOException e) { MessageBox.Show(this, Ui.Get(Msg.SettingsError) + "\n" + e.Message, "SubZir"); }
            catch (UnauthorizedAccessException e) { MessageBox.Show(this, Ui.Get(Msg.SettingsError) + "\n" + e.Message, "SubZir"); }
        };
        Controls.Add(language);
        update.SetBounds(482,65,200,34);Controls.Add(update);update.BringToFront();
        update.Click+=async delegate {await CheckUpdate();};
        Directory.CreateDirectory(Data);
        ZoneButtons();
        roi=File.Exists(CropFile)?File.ReadAllText(CropFile).Trim():null;
        if(roi!=null && !System.Text.RegularExpressions.Regex.IsMatch(roi,@"^\d+:\d+:\d+:\d+$"))roi=null;
        SetStatus(roi==null && layout==null?Msg.ConnectFirst:Msg.AreaReady);
        RefreshLanguage();
        FormClosing+=delegate {closing=true;StopOwned();};
    }
    Label LabelAt(Msg text,int x,int y,int w,int h){var l=new Label{AutoEllipsis=true};Caption(l,text);l.SetBounds(x,y,w,h);Controls.Add(l);return l;}
    void ButtonAt(Button b,Msg text,int x,int y,int w){Caption(b,text);b.SetBounds(x,y,w,42);b.AutoEllipsis=true;b.FlatStyle=FlatStyle.Flat;Controls.Add(b);}
    void HelpOBS(){
        try {Clipboard.SetText(Lua);}
        catch {Process.Start("explorer.exe","/select,"+Quote(Lua));}
        MessageBox.Show(this,Ui.Get(Msg.Help),Ui.Get(Msg.ConnectTitle),MessageBoxButtons.OK,MessageBoxIcon.Information);
    }
    static string Quote(string path){return "\""+path+"\"";}
    ProcessStartInfo Info(string exe,string args){
        var s=new ProcessStartInfo(exe,args){WorkingDirectory=App,UseShellExecute=false,CreateNoWindow=true,RedirectStandardOutput=true,RedirectStandardError=true,StandardOutputEncoding=Utf8,StandardErrorEncoding=Utf8};
        s.EnvironmentVariables["PATH"]=Path.Combine(App,"bin")+";"+s.EnvironmentVariables["PATH"];
        s.EnvironmentVariables["SUB_RUNTIME"]=Data;s.EnvironmentVariables["SUB_WORKER"]=Path.Combine(App,"worker.py");
        s.EnvironmentVariables["SUB_PYTHON"]=Path.Combine(App,"python","python.exe");
        s.EnvironmentVariables["SUB_DEVICE"]="OBS Virtual Camera";s.EnvironmentVariables["SUB_DEMO"]=Path.Combine(App,"demo");
        s.EnvironmentVariables["SUB_QUICK_DEVICE"]="cpu";s.EnvironmentVariables["SUB_BEAM"]="3";
        s.EnvironmentVariables["SUB_MODEL"]=Ui.Model;
        if(Ui.Model!="auto")s.EnvironmentVariables["SUB_TARGET"]=Ui.Target;
        if(roi!=null)s.EnvironmentVariables["SUB_CROP"]=roi;
        return s;
    }
    void Lock(bool val){busy=val;editZones.Enabled=editRecording.Enabled=!val;models.Enabled=!val;update.Enabled=!val;connect.Enabled=demo.Enabled=select.Enabled=!val;play.Enabled=!val;}
    void StopOwned(){
        Process p=running;
        stopping=true;
        if(engineJob!=null){engineJob.Dispose();engineJob=null;}
        else if(p!=null && !p.HasExited){
            using(var killer=Process.Start(new ProcessStartInfo(Path.Combine(Environment.SystemDirectory,"taskkill.exe"),"/PID "+p.Id+" /T /F"){UseShellExecute=false,CreateNoWindow=true})){killer.WaitForExit(4000);}
        }
        if(p!=null){File.WriteAllText(Path.Combine(Data,"caption.txt"),"stop-"+DateTime.UtcNow.Ticks+"\n",Utf8);line.Text="";SetStatus(Msg.Stopped);}
    }
    async Task Start(bool live){
        if(busy)return;
        if(live && layout==null && roi==null){await Choose();if(roi==null || busy || closing)return;}
        stopping=false;Lock(true);line.Text="";SetStatus(live?Msg.Loading:Msg.Checking);
        var err=new StringBuilder();bool got=false;
        try{
            using(var p=new Process{StartInfo=Info(Path.Combine(App,"subtitle-rs.exe"),live?(layout==null?"live":"zones"+(recording==null?"":" "+Quote(recording))):"demo")}){
                p.OutputDataReceived+=(o,e)=>{if(e.Data==null || closing)return;var message=e.Data;
                    BeginInvoke((Action)(()=>{if(closing || stopping || running!=p)return;try{
                        var row=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(message);
                        line.Text=row.ContainsKey("captions")?PublishZones(row["captions"]):Convert.ToString(row["uk"]);got=true;
                        if(live)SetStatus(Msg.Active);
                        else SetStatus(Convert.ToBoolean(row["obs"])?Msg.TestSuccess:Msg.TestNoAck);
                    }catch(Exception ex){SetStatus(Msg.ResponseError,ex.Message);}}));};
                p.ErrorDataReceived+=(o,e)=>{if(e.Data==null)return;lock(err){err.AppendLine(e.Data);if(err.Length>32000)err.Remove(0,err.Length-16000);}
                    if(live && e.Data.StartsWith("Worker ") && !closing)BeginInvoke((Action)(()=>{if(running==p)SetStatus(Msg.Waiting);}));};
                p.Start();running=p;engineJob=EngineJob.Attach(p);p.BeginOutputReadLine();p.BeginErrorReadLine();if(live){Caption(play,Msg.Stop);play.Enabled=true;}
                await Task.Run(()=>p.WaitForExit());
                if(closing)return;
                if(p.ExitCode!=0 && !(live && statusMessage==Msg.Stopped)){
                    File.WriteAllText(Path.Combine(Data,"diagnostics.txt"),err.ToString(),Utf8);
                    string msg=err.ToString();
                    bool modelFailure=Ui.IsGpuModel(Ui.Model) && (msg.Contains("hy_worker.py") || msg.Contains("Hy-MT2 server:") || msg.Contains("Hy-MT2 runtime:") || msg.Contains("Translation worker stopped"));
                    SetStatus(modelFailure?Msg.ModelError:msg.Contains("Windows OCR")||msg.Contains("DEMO_OCR")?Msg.OcrMissing:live?Msg.CheckCamera:Msg.TestFailed);
                }else if(!got && !live){SetStatus(Msg.TestEmpty);}
                else if(live && recording!=null && !stopping){SetStatus(Msg.Stopped," · "+Path.GetFileName(recording));}
                running=null;
            }
        }catch(Exception ex){if(!closing)SetStatus(Msg.StartError,ex.Message);}
        finally{running=null;if(engineJob!=null){engineJob.Dispose();engineJob=null;}if(live && layout!=null)File.WriteAllText(Path.Combine(Data,"caption.txt"),"stop-"+DateTime.UtcNow.Ticks+"\n",Utf8);if(!closing){Lock(false);Caption(play,Msg.Start);}}
    }
    async Task Choose(){
        if(busy)return;Lock(true);SetStatus(Msg.GettingFrame);
        string path=Path.Combine(Data,"preview-"+Guid.NewGuid().ToString("N")+".bmp");
        try{
            using(var p=new Process{StartInfo=Info(Path.Combine(App,"bin","ffmpeg.exe"),"-nostdin -hide_banner -loglevel error -f dshow -i \"video=OBS Virtual Camera\" -frames:v 1 -threads:v 1 "+Quote(path))}){
                p.Start();running=p;p.BeginOutputReadLine();p.BeginErrorReadLine();
                if(!await Task.Run(()=>p.WaitForExit(12000))){p.Kill();p.WaitForExit();}
                running=null;if(closing)return;
                if(!File.Exists(path)||p.ExitCode!=0){SetStatus(Msg.NoFrame);return;}
            }
            using(var img=new Bitmap(path))using(var picker=new Picker(img)){
                if(picker.ShowDialog(this)!=DialogResult.OK){SetStatus(Msg.Cancelled);return;}
                Rectangle a=picker.Area;roi=String.Format("{0}:{1}:{2}:{3}",a.Width,a.Height,a.X,a.Y);
                File.WriteAllText(CropFile,roi,Utf8);if(File.Exists(ZoneFile))File.Delete(ZoneFile);layout=null;recording=null;SetStatus(Msg.Saved);
            }
        }catch(Exception ex){if(!closing)SetStatus(Msg.FrameError,ex.Message);}
        finally{running=null;try{File.Delete(path);}catch(IOException){} if(!closing)Lock(false);}
    }
    [STAThread]static void Main(){Application.EnableVisualStyles();Application.SetCompatibleTextRenderingDefault(false);
        bool first;using(var gate=new System.Threading.Mutex(true,"Local\\PS5QuickMTLauncher",out first)){
        if(!first){MessageBox.Show(Ui.Get(Msg.AlreadyOpen));return;}
        try{Application.Run(new Launcher());}catch(Exception e){MessageBox.Show(Ui.Get(Msg.LaunchError)+e.Message,Ui.Get(Msg.Window));}}}
}

class Picker:Form {
    public Rectangle Area; readonly PictureBox picture; Point anchor;Rectangle box; bool drawing;
    public Picker(Bitmap image){
        Text=Ui.Get(Msg.Picker);
        StartPosition=FormStartPosition.CenterParent; FormBorderStyle=FormBorderStyle.FixedDialog;MaximizeBox=false;MinimizeBox=false;
        var screen=Screen.FromPoint(Cursor.Position).WorkingArea;
        double scale=Math.Min(1,Math.Min((screen.Width-80.0)/image.Width,(screen.Height-170.0)/image.Height));
        int w=(int)(image.Width*scale), h=(int)(image.Height*scale);
        ClientSize=new Size(w,h+55);picture=new PictureBox{Image=image,SizeMode=PictureBoxSizeMode.StretchImage,Cursor=Cursors.Cross,Size=new Size(w,h)};Controls.Add(picture);
        var save=new Button{Text=Ui.Get(Msg.Save),Left=w-162,Top=h+9,Width=150,Height=35,Enabled=false,DialogResult=DialogResult.OK};Controls.Add(save);
        var cancel=new Button{Text=Ui.Get(Msg.Cancel),Left=12,Top=h+9,Width=110,Height=35,DialogResult=DialogResult.Cancel};Controls.Add(cancel);CancelButton=cancel;AcceptButton=save;
        picture.MouseDown+=(s,e)=>{if(e.Button!=MouseButtons.Left)return;anchor=e.Location;drawing=true;picture.Capture=true;save.Enabled=false;box=Rectangle.Empty;picture.Invalidate();};
        picture.MouseMove+=(s,e)=>{if(!drawing)return;int x=Math.Max(0,Math.Min(w,e.X)),y=Math.Max(0,Math.Min(h,e.Y));
            box=Rectangle.FromLTRB(Math.Min(anchor.X,x),Math.Min(anchor.Y,y),Math.Max(anchor.X,x),Math.Max(anchor.Y,y));picture.Invalidate();};
        picture.MouseUp+=(s,e)=>{if(!drawing)return;drawing=false;picture.Capture=false;
            int x=box.X*image.Width/w/2*2,y=box.Y*image.Height/h/2*2;
            Area=new Rectangle(x,y,(box.Right*image.Width/w-x)/2*2,(box.Bottom*image.Height/h-y)/2*2);save.Enabled=Area.Width>=40 && Area.Height>=12;};
        picture.Paint+=(s,e)=>{if(box.Width>0 && box.Height>0){using(var pen=new Pen(Color.Lime,3))e.Graphics.DrawRectangle(pen,box);}};
    }
}
