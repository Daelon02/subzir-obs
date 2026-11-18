using System;
using System.Drawing;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using System.Collections.Generic;
using System.Web.Script.Serialization;

class Launcher : Form {
    static readonly string Root = AppDomain.CurrentDomain.BaseDirectory;
    static readonly string App = Path.Combine(Root,"app"), Data = Path.Combine(Root,"runtime");
    static readonly string Lua = Path.Combine(Root,"OBS","PS5-Subtitles.lua");
    static readonly UTF8Encoding Utf8 = new UTF8Encoding(false);
    readonly Button connect = new Button(), demo = new Button(), select = new Button(), play = new Button();
    readonly Label status = new Label(), line = new Label();
    Process running;
    bool busy, closing;
    string roi;
    static readonly string CropFile = Path.Combine(Data,"area.txt");
    public Launcher() {
        Text="Субтитри PS5"; Font=new Font("Segoe UI",10); ClientSize=new Size(650,476);
        AutoScaleMode=AutoScaleMode.Dpi; FormBorderStyle=FormBorderStyle.FixedDialog; MaximizeBox=false; StartPosition=FormStartPosition.CenterScreen;
        BackColor=Color.White;
        Label title=LabelAt("Українські субтитри",24,18,590,46);title.Font=new Font(Font.FontFamily,22,FontStyle.Bold);
        LabelAt("Англійський текст із гри на PS5 → український текст у сцені OBS",26,69,605,30);
        ButtonAt(connect,"1. Під’єднати OBS",26,116,208);connect.Click+=delegate { HelpOBS(); };
        LabelAt("Один раз додайте скрипт до сцени з грою.",250,124,374,31);
        ButtonAt(demo,"2. Перевірити переклад",26,173,208);demo.Click+=async delegate {await Start(false);};
        LabelAt("Пробна фраза працює навіть без консолі.",250,181,376,32);
        ButtonAt(select,"3. Виділити субтитри",26,230,208);select.Click+=async delegate {await Choose();};
        LabelAt("Обведіть англійський текст на кадрі з гри.",250,238,376,30);
        ButtonAt(play,"Почати переклад",26,289,251);play.BackColor=Color.FromArgb(23,78,103);play.ForeColor=Color.White;
        play.Click+=async delegate {if(running!=null)StopOwned();else await Start(true);};
        Button guide=new Button(); ButtonAt(guide,"Відкрити інструкцію",352,289,270);
        guide.Click+=delegate {Process.Start("notepad.exe",Quote(Path.Combine(Root,"README.txt")));};
        status.SetBounds(26,351,596,48);status.ForeColor=Color.FromArgb(35,69,83);Controls.Add(status);
        line.SetBounds(26,404,596,60);line.Font=new Font(Font.FontFamily,12,FontStyle.Bold);Controls.Add(line);
        Directory.CreateDirectory(Data);
        roi=File.Exists(CropFile)?File.ReadAllText(CropFile).Trim():null;
        if(roi!=null && !System.Text.RegularExpressions.Regex.IsMatch(roi,@"^\d+:\d+:\d+:\d+$"))roi=null;
        status.Text=roi==null?"Почніть із під’єднання OBS.":"Область субтитрів збережено. Увімкніть віртуальну камеру OBS.";
        FormClosing+=delegate {closing=true;StopOwned();};
    }
    Label LabelAt(string text,int x,int y,int w,int h){var l=new Label{Text=text};l.SetBounds(x,y,w,h);Controls.Add(l);return l;}
    void ButtonAt(Button b,string text,int x,int y,int w){b.Text=text;b.SetBounds(x,y,w,42);b.FlatStyle=FlatStyle.Flat;Controls.Add(b);}
    void HelpOBS(){
        try {Clipboard.SetText(Lua);}
        catch {Process.Start("explorer.exe","/select,"+Quote(Lua));}
        MessageBox.Show(this,"В OBS виберіть сцену, де граєте. Відкрийте «Інструменти → Скрипти», натисніть «+» і вставте шлях до файлу (Ctrl+V). Натисніть «Відкрити».\n\nПраворуч натисніть «Додати субтитри до поточної сцени». Поверніться сюди й перевірте переклад.\n\nФайл також є в папці OBS поруч із програмою.","Під’єднання до OBS",MessageBoxButtons.OK,MessageBoxIcon.Information);
    }
    static string Quote(string path){return "\""+path+"\"";}
    ProcessStartInfo Info(string exe,string args){
        var s=new ProcessStartInfo(exe,args){WorkingDirectory=App,UseShellExecute=false,CreateNoWindow=true,RedirectStandardOutput=true,RedirectStandardError=true,StandardOutputEncoding=Utf8,StandardErrorEncoding=Utf8};
        s.EnvironmentVariables["PATH"]=Path.Combine(App,"bin")+";"+s.EnvironmentVariables["PATH"];
        s.EnvironmentVariables["SUB_RUNTIME"]=Data;s.EnvironmentVariables["SUB_WORKER"]=Path.Combine(App,"worker.py");
        s.EnvironmentVariables["SUB_PYTHON"]=Path.Combine(App,"python","python.exe");
        s.EnvironmentVariables["SUB_DEVICE"]="OBS Virtual Camera";s.EnvironmentVariables["SUB_DEMO"]=Path.Combine(App,"demo");
        s.EnvironmentVariables["SUB_QUICK_DEVICE"]="cpu";s.EnvironmentVariables["SUB_BEAM"]="3";
        if(roi!=null)s.EnvironmentVariables["SUB_CROP"]=roi;
        return s;
    }
    void Lock(bool val){busy=val;connect.Enabled=demo.Enabled=select.Enabled=!val;play.Enabled=!val;}
    void StopOwned(){
        Process p=running;
        if(p!=null && !p.HasExited){
            using(var killer=Process.Start(new ProcessStartInfo(Path.Combine(Environment.SystemDirectory,"taskkill.exe"),"/PID "+p.Id+" /T /F"){UseShellExecute=false,CreateNoWindow=true})){killer.WaitForExit(4000);}
        }
        if(p!=null){File.WriteAllText(Path.Combine(Data,"caption.txt"),"stop-"+DateTime.UtcNow.Ticks+"\n",Utf8);line.Text="";status.Text="Переклад зупинено.";}
    }
    async Task Start(bool live){
        if(busy)return;
        if(live && roi==null){await Choose();if(roi==null || busy || closing)return;}
        Lock(true);line.Text="";status.Text=live?"Завантаження перекладача…":"Розпізнаю тестову фразу й перевіряю зв’язок з OBS…";
        var err=new StringBuilder();bool got=false;
        try{
            using(var p=new Process{StartInfo=Info(Path.Combine(App,"subtitle-rs.exe"),live?"live":"demo")}){
                p.OutputDataReceived+=(o,e)=>{if(e.Data==null || closing)return;var message=e.Data;
                    BeginInvoke((Action)(()=>{if(closing || running!=p)return;try{
                        var row=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(message);
                        line.Text=Convert.ToString(row["uk"]);got=true;
                        if(live)status.Text="Переклад увімкнено.";
                        else status.Text=Convert.ToBoolean(row["obs"])?"OBS підтвердив отримання фрази. Перевірте її розташування на екрані.":"Переклад готовий, але OBS його не підтвердив. Виберіть сцену гри й повторіть крок 1.";
                    }catch(Exception ex){status.Text="Не вдалося прочитати відповідь: "+ex.Message;}}));};
                p.ErrorDataReceived+=(o,e)=>{if(e.Data==null)return;lock(err){if(err.Length<16000)err.AppendLine(e.Data);}
                    if(live && e.Data.StartsWith("Worker ") && !closing)BeginInvoke((Action)(()=>{if(running==p)status.Text="Очікую англійські субтитри з віртуальної камери OBS…";}));};
                p.Start();running=p;p.BeginOutputReadLine();p.BeginErrorReadLine();if(live){play.Text="Зупинити переклад";play.Enabled=true;}
                await Task.Run(()=>p.WaitForExit());
                if(closing)return;
                if(p.ExitCode!=0 && !(live && status.Text=="Переклад зупинено.")){
                    File.WriteAllText(Path.Combine(Data,"diagnostics.txt"),err.ToString(),Utf8);
                    string msg=err.ToString();
                    status.Text=msg.Contains("Windows OCR")||msg.Contains("DEMO_OCR")?"Додайте розпізнавання англійського тексту Windows. Порядок дій є в інструкції.":live?"Перевірте віртуальну камеру й повторно виділіть область субтитрів.":"Тест не завершився. Відкрийте розділ «Якщо щось не працює» в інструкції.";
                }else if(!got && !live){status.Text="Не отримано тестової фрази. Повторіть перевірку.";}
                running=null;
            }
        }catch(Exception ex){if(!closing)status.Text="Не вдалося запустити переклад. "+ex.Message;}
        finally{running=null;if(!closing){Lock(false);play.Text="Почати переклад";}}
    }
    async Task Choose(){
        if(busy)return;Lock(true);status.Text="Отримую кадр з віртуальної камери OBS…";
        string path=Path.Combine(Data,"preview-"+Guid.NewGuid().ToString("N")+".bmp");
        try{
            using(var p=new Process{StartInfo=Info(Path.Combine(App,"bin","ffmpeg.exe"),"-nostdin -hide_banner -loglevel error -f dshow -i \"video=OBS Virtual Camera\" -frames:v 1 -threads:v 1 "+Quote(path))}){
                p.Start();running=p;p.BeginOutputReadLine();p.BeginErrorReadLine();
                if(!await Task.Run(()=>p.WaitForExit(12000))){p.Kill();p.WaitForExit();}
                running=null;if(closing)return;
                if(!File.Exists(path)||p.ExitCode!=0){status.Text="Немає кадру. В OBS натисніть «Запустити віртуальну камеру», потім повторіть крок 3.";return;}
            }
            using(var img=new Bitmap(path))using(var picker=new Picker(img)){
                if(picker.ShowDialog(this)!=DialogResult.OK){status.Text="Вибір скасовано.";return;}
                Rectangle a=picker.Area;roi=String.Format("{0}:{1}:{2}:{3}",a.Width,a.Height,a.X,a.Y);
                File.WriteAllText(CropFile,roi,Utf8);status.Text="Область збережено. Натисніть «Почати переклад».";
            }
        }catch(Exception ex){if(!closing)status.Text="Не вдалося отримати кадр. "+ex.Message;}
        finally{running=null;try{File.Delete(path);}catch(IOException){} if(!closing)Lock(false);}
    }
    [STAThread]static void Main(){Application.EnableVisualStyles();Application.SetCompatibleTextRenderingDefault(false);
        bool first;using(var gate=new System.Threading.Mutex(true,"Local\\PS5QuickMTLauncher",out first)){
        if(!first){MessageBox.Show("Програму вже відкрито.");return;}
        try{Application.Run(new Launcher());}catch(Exception e){MessageBox.Show("Розпакуйте ZIP у папку Документи та повторіть запуск.\n"+e.Message,"Субтитри PS5");}}}
}

class Picker:Form {
    public Rectangle Area; readonly PictureBox picture; Point anchor;Rectangle box; bool drawing;
    public Picker(Bitmap image){
        Text="Обведіть мишею місце для одного або двох рядків субтитрів";
        StartPosition=FormStartPosition.CenterParent; FormBorderStyle=FormBorderStyle.FixedDialog;MaximizeBox=false;MinimizeBox=false;
        var screen=Screen.FromPoint(Cursor.Position).WorkingArea;
        double scale=Math.Min(1,Math.Min((screen.Width-80.0)/image.Width,(screen.Height-170.0)/image.Height));
        int w=(int)(image.Width*scale), h=(int)(image.Height*scale);
        ClientSize=new Size(w,h+55);picture=new PictureBox{Image=image,SizeMode=PictureBoxSizeMode.StretchImage,Cursor=Cursors.Cross,Size=new Size(w,h)};Controls.Add(picture);
        var save=new Button{Text="Зберегти область",Left=w-162,Top=h+9,Width=150,Height=35,Enabled=false,DialogResult=DialogResult.OK};Controls.Add(save);
        var cancel=new Button{Text="Скасувати",Left=12,Top=h+9,Width=110,Height=35,DialogResult=DialogResult.Cancel};Controls.Add(cancel);CancelButton=cancel;AcceptButton=save;
        picture.MouseDown+=(s,e)=>{if(e.Button!=MouseButtons.Left)return;anchor=e.Location;drawing=true;picture.Capture=true;save.Enabled=false;box=Rectangle.Empty;picture.Invalidate();};
        picture.MouseMove+=(s,e)=>{if(!drawing)return;int x=Math.Max(0,Math.Min(w,e.X)),y=Math.Max(0,Math.Min(h,e.Y));
            box=Rectangle.FromLTRB(Math.Min(anchor.X,x),Math.Min(anchor.Y,y),Math.Max(anchor.X,x),Math.Max(anchor.Y,y));picture.Invalidate();};
        picture.MouseUp+=(s,e)=>{if(!drawing)return;drawing=false;picture.Capture=false;
            int x=box.X*image.Width/w/2*2,y=box.Y*image.Height/h/2*2;
            Area=new Rectangle(x,y,(box.Right*image.Width/w-x)/2*2,(box.Bottom*image.Height/h-y)/2*2);save.Enabled=Area.Width>=40 && Area.Height>=12;};
        picture.Paint+=(s,e)=>{if(box.Width>0 && box.Height>0){using(var pen=new Pen(Color.Lime,3))e.Graphics.DrawRectangle(pen,box);}};
    }
}
