using System.Drawing;
using System.Windows.Forms;

sealed class ZoneGuide : Form {
    readonly PictureBox example = new PictureBox();

    public ZoneGuide() {
        Text=Editor.T("Як налаштувати зони","Zone setup guide");
        Font=new Font("Segoe UI",10);
        StartPosition=FormStartPosition.CenterParent;
        MinimizeBox=false;
        ClientSize=new Size(720,570);
        AutoScaleMode=AutoScaleMode.Dpi;
        var layout=new TableLayoutPanel{Dock=DockStyle.Fill,RowCount=2,ColumnCount=1,Padding=new Padding(12)};
        layout.RowStyles.Add(new RowStyle(SizeType.Percent,52));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent,48));
        var steps=new RichTextBox{ReadOnly=true,Dock=DockStyle.Fill,BorderStyle=BorderStyle.None,
            BackColor=SystemColors.Window,Text=Channel.Resource(Ui.Language=="uk"?"zones-uk":"zones-en"),
            AccessibleName=Text,DetectUrls=false};
        using(var stream=typeof(ZoneGuide).Assembly.GetManifestResourceStream("zones-example"))
        using(var picture=Image.FromStream(stream))example.Image=new Bitmap(picture);
        example.Dock=DockStyle.Fill;
        example.SizeMode=PictureBoxSizeMode.Zoom;
        example.AccessibleName=Editor.T("Переклад над оригінальним текстом у God of War","Translation above the original text in God of War");
        layout.Controls.Add(steps,0,0);layout.Controls.Add(example,0,1);Controls.Add(layout);
    }
    protected override void Dispose(bool disposing) {
        if(disposing && example.Image!=null){example.Image.Dispose();example.Image=null;}
        base.Dispose(disposing);
    }
}
