using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Threading.Tasks;
using System.Windows.Forms;

partial class Launcher {
    readonly Button update=new Button();
    static readonly Dictionary<string,string> UpdateLabels=Channel.Json.Deserialize<Dictionary<string,string>>(Channel.Resource("update-labels"));
    string UpdateText(string en,string uk,string other) {return Ui.Language=="uk"?uk:Ui.Language=="en"?en:other;}
    void UpdateCaption() {
        string caption;
        update.Text=UpdateLabels.TryGetValue(Ui.Language,out caption)?caption:UpdateLabels["en"];
        update.AccessibleName=update.Text;
    }
    async Task CheckUpdate() {
        if(busy || closing)return;
        Enabled=false;
        try {
            ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls12;
            var release=await Task.Run(()=>{
                using(var res=Channel.Request(Channel.Settings.latest).GetResponse())
                using(var input=new StreamReader(res.GetResponseStream()))return Channel.Json.Deserialize<Channel.Release>(input.ReadToEnd());
            });
            if(closing)return;
            var asset=Channel.Newer(release);
            if(asset==null) {
                MessageBox.Show(this,UpdateText("You have the latest version.","У вас актуальна версія.","✓ SubZir")+" ("+Channel.Version.ToString(3)+")","SubZir");return;
            }
            if(MessageBox.Show(this,UpdateText("Download and install ","Завантажити й встановити ",update.Text+" → ")+release.tag_name+"?","SubZir",MessageBoxButtons.YesNo)!=DialogResult.Yes)return;
            update.Text=UpdateText("Downloading…","Завантаження…","↓ "+release.tag_name+"…");
            var setup=await Task.Run(()=>FetchSetup(asset));
            if(closing)return;
            Process.Start(new ProcessStartInfo(setup,"--update "+Quote(Root.TrimEnd(Path.DirectorySeparatorChar))){UseShellExecute=false});
            Close();
        } catch(Exception ex) {
            if(!closing)MessageBox.Show(this,UpdateText("Update failed. Try again.\n","Не вдалося оновити. Спробуйте ще раз.\n",update.AccessibleName+" ×\n")+ex.Message,"SubZir");
        } finally {if(!closing){Enabled=true;UpdateCaption();}}
    }
    static string FetchSetup(Channel.Asset asset) {
        string dir=Path.Combine(Path.GetTempPath(),"SubZir-update-"+Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(dir);string file=Path.Combine(dir,Channel.SetupName);
        try {
            using(var res=Channel.Request(asset.browser_download_url).GetResponse())
            using(var input=res.GetResponseStream())
            using(var outFile=new FileStream(file,FileMode.CreateNew,FileAccess.Write,FileShare.None)){
                var buffer=new byte[64*1024];int count;
                while((count=input.Read(buffer,0,buffer.Length))!=0){
                    if(count>asset.size-outFile.Position)throw new InvalidDataException("Unexpected installer size.");
                    outFile.Write(buffer,0,count);
                }
            }
            if(new FileInfo(file).Length!=asset.size || Channel.HashFile(file)!=asset.digest.Substring(7))throw new InvalidDataException("SHA-256");
            return file;
        } catch {if(File.Exists(file))File.Delete(file);throw;}
    }
}
