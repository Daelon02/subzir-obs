using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Net;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

sealed class Setup : Form {
    readonly ComboBox models=new ComboBox();
    readonly TextBox folder=new TextBox();
    readonly TextBox destination=new TextBox{ReadOnly=true,TabStop=false};
    readonly Button browse=new Button(),go=new Button();
    readonly Label status=new Label();
    readonly ProgressBar bar=new ProgressBar();
    CancellationTokenSource active;
    string installedDirectory;
    static string T(string en,string uk){return CultureInfo.CurrentUICulture.TwoLetterISOLanguageName=="uk"?uk:en;}
    static void AddShortcut(string dir) {
        var shell=Activator.CreateInstance(Type.GetTypeFromProgID("WScript.Shell",true));
        try {
            dynamic link=((dynamic)shell).CreateShortcut(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs),"SubZir.lnk"));
            try {link.TargetPath=Path.Combine(dir,"SubZir.exe");link.WorkingDirectory=dir;link.Save();}
            finally {Marshal.FinalReleaseComObject(link);}
        } finally {Marshal.FinalReleaseComObject(shell);}
    }
    void Place(Control c,string text,int x,int y,int w,int h){c.Text=text;c.SetBounds(x,y,w,h);Controls.Add(c);}
    static string CudaWarning(CudaDevice[] devices) {
        string reason=T("The NVIDIA driver reported no available CUDA device.","Драйвер NVIDIA не надав жодного доступного пристрою CUDA.");
        if(Array.Exists(devices,d=>d.Unavailable==CudaDevice.Limitation.Driver))
            reason=T("Update the NVIDIA driver to use this GPU.","Для цієї відеокарти потрібно оновити драйвер NVIDIA.");
        else if(Array.Exists(devices,d=>d.Unavailable==CudaDevice.Limitation.Memory))
            reason=T("The detected GPU has less than 5 GiB of video memory.","У виявленої відеокарти менше ніж 5 ГіБ відеопам’яті.");
        else if(devices.Length>0)
            reason=T("The available CUDA packages do not support this GPU architecture.","Наявні пакети CUDA не підтримують архітектуру цієї відеокарти.");
        return T("⚠ CUDA unavailable for Hy-MT2 7B","⚠ CUDA недоступна для Hy-MT2 7B")+Environment.NewLine+
            reason+Environment.NewLine+T("QuickMT and M2M100 can run on the CPU.","QuickMT і M2M100 можуть працювати на процесорі.");
    }
    Setup(string existing) {
        Text="SubZir "+Channel.Version.ToString(3);Font=new Font("Segoe UI",10);
        ClientSize=new Size(542,345);FormBorderStyle=FormBorderStyle.FixedDialog;
        MaximizeBox=false;StartPosition=FormStartPosition.CenterScreen;AutoScaleMode=AutoScaleMode.Dpi;
        var modelTitle=new Label();
        Place(modelTitle,T("Local translation model","Локальна модель перекладу"),18,16,490,24);
        Place(models,"",18,41,506,30);models.DropDownStyle=ComboBoxStyle.DropDownList;
        models.DisplayMember="Key";
        var devices=CudaDevice.Detect();
        var card=CudaDevice.Best(devices,null);
        if(card!=null)models.Items.Add(new KeyValuePair<string,string>("Hy-MT2 7B",Install.HyModel));
        models.Items.Add(new KeyValuePair<string,string>("QuickMT · EN → UK","quickmt"));
        models.Items.Add(new KeyValuePair<string,string>("M2M100 · "+T("multilingual","багатомовна"),"m2m100"));
        models.SelectedIndex=0;
        Place(new Label(),T("Choose a folder for SubZir","Виберіть папку для SubZir"),18,85,488,25);
        Place(folder,existing??Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),"Programs"),18,112,395,31);
        Place(browse,T("Browse…","Вибрати…"),421,110,103,31);
        browse.Click+=delegate {using(var pick=new FolderBrowserDialog()){pick.SelectedPath=folder.Text;if(pick.ShowDialog(this)==DialogResult.OK)folder.Text=pick.SelectedPath;}};
        Place(new Label(),T("SubZir will be installed in","Шлях встановлення SubZir"),18,148,506,25);
        Place(destination,"",18,174,506,28);
        Place(status,"",18,210,506,45);
        Place(bar,"",18,258,506,16);
        Place(go,"",329,292,195,38);
        if(card==null) {
            const int space=94;
            ClientSize=new Size(ClientSize.Width,ClientSize.Height+space);
            foreach(Control control in Controls)if(control.Top>=85)control.Top+=space;
            var warning=new Label{Name="cudaWarning",BackColor=Color.LemonChiffon,ForeColor=Color.FromArgb(80,49,0),
                BorderStyle=BorderStyle.FixedSingle,Padding=new Padding(8,4,8,4),AccessibleRole=AccessibleRole.Alert};
            Place(warning,CudaWarning(devices),18,79,506,88);
        }
        Action refresh=delegate {
            destination.Text=Install.Destination(folder.Text,existing!=null);
            bool update=existing!=null || (destination.TextLength!=0 && File.Exists(Path.Combine(destination.Text,"SubZir.exe")));
            models.Visible=modelTitle.Visible=!update;
            models.Enabled=!update;
            bool hy=((KeyValuePair<string,string>)models.SelectedItem).Value==Install.HyModel;
            status.Text=update?T("Update this installation and keep its models and settings.","Оновіть програму зі збереженням моделей і налаштувань."):
                hy?card.Name+" · CUDA "+(card.Edition==CudaDevice.Cuda13?"13 · 6.10 GB":"12 · 6.28 GB"):
                T("CPU · QuickMT and M2M100 share a 0.85 GB download.","CPU · Спільне завантаження QuickMT і M2M100 — 0,85 ГБ.");
            go.Text=update?T("Update","Оновити"):T("Install","Встановити");
            go.Enabled=destination.TextLength!=0;
            if(!go.Enabled)status.Text=T("Choose a folder or enter its full path.","Виберіть папку або введіть повний шлях.");
        };
        refresh();folder.TextChanged+=delegate {refresh();};models.SelectedIndexChanged+=delegate {refresh();};
        if(existing!=null){folder.ReadOnly=true;browse.Enabled=false;}
        go.Click+=async delegate {
            if(installedDirectory!=null){try{Process.Start(Path.Combine(installedDirectory,"SubZir.exe"));Close();}catch(Exception ex){MessageBox.Show(this,ex.Message,Text);}return;}
            if(active!=null){active.Cancel();return;}
            string dir=destination.Text;
            string model=((KeyValuePair<string,string>)models.SelectedItem).Value;
            string edition=model==Install.HyModel?card.Edition:"cpu";
            bool update=existing!=null || File.Exists(Path.Combine(dir,"SubZir.exe"));
            var token=new CancellationTokenSource();active=token;
            models.Enabled=false;folder.Enabled=false;browse.Enabled=false;
            go.Text=T("Cancel","Скасувати");go.Enabled=!update;
            int last=-1;
            IProgress<int> changed=new Progress<int>(n=>{
                bar.Value=n;status.Text=n<80?T("Downloading and checking files","Завантаження й перевірка файлів"):T("Installing files","Встановлення файлів");
                status.Text+="  ·  "+n+"%";
            });
            try {
                if(update)await Task.Run(()=>Install.Upgrade(dir));
                else await Task.Run(()=>Install.Fresh(dir,edition,model,Path.Combine(Path.GetTempPath(),"SubZir-downloads",Install.Base.baseline),n=>{
                    if(n!=last){last=n;changed.Report(n);}
                },token.Token));
                installedDirectory=dir;bar.Value=100;go.Text=T("Open SubZir","Відкрити SubZir");
                status.Text=T("Ready to play. SubZir is installed in the folder above.","Готово. SubZir встановлено у вибрану папку.");
                if(!update)try{AddShortcut(dir);}catch(Exception){status.Text=T("Installed. Open SubZir.exe in the folder above.","Встановлено. Відкрийте SubZir.exe у вибраній папці.");}
            } catch(Exception ex) {
                refresh();folder.Enabled=existing==null;browse.Enabled=existing==null;
                status.Text=token.IsCancellationRequested?T("Paused. Click Install to resume.","Призупинено. Натисніть «Встановити», щоб продовжити."):ex.Message;
            } finally {go.Enabled=destination.TextLength!=0;active=null;token.Dispose();}
        };
        FormClosing+=(s,e)=>{if(active!=null){active.Cancel();e.Cancel=true;}};
        if(existing!=null)Shown+=delegate {go.PerformClick();};
    }
    [STAThread] static void Main(string[] args){
        Application.EnableVisualStyles();Application.SetCompatibleTextRenderingDefault(false);
        bool first;
        using(var single=new Mutex(true,@"Local\SubZirSetup",out first)){
            if(!first){MessageBox.Show(T("Setup is already open.","Інсталятор уже відкритий."),"SubZir");return;}
            ServicePointManager.SecurityProtocol|=SecurityProtocolType.Tls12;
            Application.Run(new Setup(args.Length==2 && args[0]=="--update"?args[1]:null));
        }
    }
}
