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
    readonly ComboBox gpu=new ComboBox();
    readonly TextBox folder=new TextBox();
    readonly Button browse=new Button(),go=new Button();
    readonly Label status=new Label();
    readonly ProgressBar bar=new ProgressBar();
    CancellationTokenSource active;
    bool finished;
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
    Setup(string existing) {
        Text="SubZir "+Channel.Version.ToString(3);Font=new Font("Segoe UI",10);
        ClientSize=new Size(542,292);FormBorderStyle=FormBorderStyle.FixedDialog;
        MaximizeBox=false;StartPosition=FormStartPosition.CenterScreen;AutoScaleMode=AutoScaleMode.Dpi;
        var gpuTitle=new Label();
        Place(gpuTitle,T("Graphics card","Відеокарта"),18,16,490,24);
        Place(gpu,"",18,41,506,30);gpu.DropDownStyle=ComboBoxStyle.DropDownList;
        gpu.DisplayMember="Key";
        gpu.Items.Add(new KeyValuePair<string,string>("RTX 3080  ·  Hy-MT2 7B  ·  6.28 GB","3080"));
        gpu.Items.Add(new KeyValuePair<string,string>("RTX 5070  ·  Hy-MT2 7B  ·  6.10 GB","5070-Ti"));
        gpu.Items.Add(new KeyValuePair<string,string>("RTX 5070 Ti  ·  Hy-MT2 7B  ·  6.10 GB","5070-Ti"));
        gpu.Items.Add(new KeyValuePair<string,string>("CPU  ·  QuickMT / M2M100  ·  0.85 GB","cpu"));
        gpu.SelectedIndex=0;
        Place(new Label(),T("Installation folder","Папка встановлення"),18,85,488,25);
        Place(folder,existing??Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),@"Programs\SubZir"),18,112,395,31);
        Place(browse,T("Browse…","Вибрати…"),421,110,103,31);
        browse.Click+=delegate {using(var pick=new FolderBrowserDialog()){pick.SelectedPath=folder.Text;if(pick.ShowDialog(this)==DialogResult.OK)folder.Text=pick.SelectedPath;}};
        Place(status,"",18,153,506,45);
        Place(bar,"",18,201,506,16);
        Place(go,"",329,235,195,38);
        Action refresh=delegate {
            bool update=existing!=null || File.Exists(folder.Text+Path.DirectorySeparatorChar+"SubZir.exe");
            gpu.Visible=gpuTitle.Visible=!update;
            gpu.Enabled=!update;
            status.Text=update?T("Update this installation and keep its models and settings.","Оновіть програму зі збереженням моделей і налаштувань."):
                T("Setup downloads the selected model and installs SubZir.","Інсталятор завантажить вибрану модель та встановить SubZir.");
            go.Text=update?T("Update","Оновити"):T("Install","Встановити");
        };
        refresh();folder.TextChanged+=delegate {refresh();};
        if(existing!=null){folder.ReadOnly=true;browse.Enabled=false;}
        go.Click+=async delegate {
            if(finished){try{Process.Start(Path.Combine(folder.Text,"SubZir.exe"));Close();}catch(Exception ex){MessageBox.Show(this,ex.Message,Text);}return;}
            if(active!=null){active.Cancel();return;}
            string dir;
            try{if(!Path.IsPathRooted(folder.Text))throw new IOException(T("Select a folder.","Виберіть папку."));dir=Path.GetFullPath(folder.Text);}catch(Exception ex){MessageBox.Show(this,ex.Message,Text);return;}
            string edition=((KeyValuePair<string,string>)gpu.SelectedItem).Value;
            bool update=existing!=null || File.Exists(Path.Combine(dir,"SubZir.exe"));
            var token=new CancellationTokenSource();active=token;
            gpu.Enabled=false;folder.Enabled=false;browse.Enabled=false;
            go.Text=T("Cancel","Скасувати");go.Enabled=!update;
            int last=-1;
            IProgress<int> changed=new Progress<int>(n=>{
                bar.Value=n;status.Text=n<80?T("Downloading and checking files","Завантаження й перевірка файлів"):T("Installing files","Встановлення файлів");
                status.Text+="  ·  "+n+"%";
            });
            try {
                if(update)await Task.Run(()=>Install.Upgrade(dir));
                else await Task.Run(()=>Install.Fresh(dir,edition,Path.Combine(Path.GetTempPath(),"SubZir-downloads",Install.Base.baseline),n=>{
                    if(n!=last){last=n;changed.Report(n);}
                },token.Token));
                finished=true;bar.Value=100;go.Text=T("Open SubZir","Відкрити SubZir");
                status.Text=T("Ready to play. SubZir is installed in the folder above.","Готово. SubZir встановлено у вибрану папку.");
                if(!update)try{AddShortcut(dir);}catch(Exception){status.Text=T("Installed. Open SubZir.exe in the folder above.","Встановлено. Відкрийте SubZir.exe у вибраній папці.");}
            } catch(Exception ex) {
                refresh();folder.Enabled=existing==null;browse.Enabled=existing==null;
                status.Text=token.IsCancellationRequested?T("Paused. Click Install to resume.","Призупинено. Натисніть «Встановити», щоб продовжити."):ex.Message;
            } finally {go.Enabled=true;active=null;token.Dispose();}
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
