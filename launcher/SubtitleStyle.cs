using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.Text;
using System.Web.Script.Serialization;
using System.Windows.Forms;

class SubtitleStyle {
    public string face { get; set; }
    public int size { get; set; }
    public int flags { get; set; }
    public int color { get; set; }
    public int background { get; set; }
    public int opacity { get; set; }
    public bool outline { get; set; }
    public int outline_color { get; set; }
    public const int MaxOutlineSize=8;
    int outlineSize;
    // Zero preserves the original per-renderer automatic width in existing profiles.
    public int outline_size {
        get{return outlineSize;}
        set{outlineSize=Math.Max(0,Math.Min(MaxOutlineSize,value));}
    }
    public int OutlineRadius(int automatic){return outlineSize==0?automatic:outlineSize;}

    public void DrawText(Graphics g,string text,Font font,Brush ink,RectangleF body,StringFormat format,int stroke){
        if(outline && outlineSize>0){
            using(var path=new GraphicsPath())
            using(var family=font.FontFamily)
            using(var layout=(StringFormat)format.Clone())
            using(var pen=new Pen(Decode(outline_color),outlineSize*2){LineJoin=LineJoin.Round}){
                layout.FormatFlags &= ~StringFormatFlags.LineLimit;
                path.AddString(text,family,(int)font.Style,font.Size,body,layout);
                var state=g.Save();
                try{
                    g.SmoothingMode=SmoothingMode.AntiAlias;
                    g.DrawPath(pen,path);
                    g.FillPath(ink,path);
                }finally{g.Restore(state);}
            }
            return;
        }
        if(outline)using(var edge=new SolidBrush(Decode(outline_color))){
            for(int x=-1;x<=1;x++)for(int y=-1;y<=1;y++){
                if(x==0 && y==0)continue;
                var offset=body;offset.Offset(x*stroke,y*stroke);g.DrawString(text,font,edge,offset,format);
            }
        }
        g.DrawString(text,font,ink,body,format);
    }

    static readonly string PathName = Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"runtime","subtitle-style.json");
    public SubtitleStyle() {
        face="Segoe UI"; size=36; color=0xffffff; outline=true;
    }
    public SubtitleStyle Copy(){return (SubtitleStyle)MemberwiseClone();}
    public bool Valid(){return !String.IsNullOrWhiteSpace(face) && size>=8 && size<=250 &&
        opacity>=0 && opacity<=100 && flags>=0 && flags<=15;}
    public float ZoneFontSize(Zone zone){
        return zone.Style==null?DefaultFontSize(zone):size*2f/3;
    }
    float DefaultFontSize(Zone zone){
        if(!zone.Hud && !zone.UsesEntries)return size*2f/3;
        return zone.UsesEntries?Math.Max(11,Math.Min(20,size*.5f)):Math.Max(10,Math.Min(24,size*.4f));
    }
    public static SubtitleStyle DefaultForZone(Zone zone){
        var style=Load();
        if(zone.Hud||zone.UsesEntries){
            style.size=(int)Math.Round(style.DefaultFontSize(zone)*1.5f);
            style.flags=(int)FontStyle.Bold;
            style.color=Encode(zone.UsesEntries?Color.Gold:Color.FromArgb(255,212,70));
            style.background=Encode(Color.FromArgb(17,21,23));style.opacity=94;
        }
        return style;
    }
    public static SubtitleStyle Load() {
        if(File.Exists(PathName)) {
            try {
                var style = new JavaScriptSerializer().Deserialize<SubtitleStyle>(File.ReadAllText(PathName));
                if(style!=null && style.Valid())return style;
            } catch(ArgumentException) {} catch(InvalidOperationException) {} catch(IOException) {}
        }
        return new SubtitleStyle();
    }
    public void Save() {
        Ui.WriteJson(PathName,this);
    }
    public static Color Decode(int value) { return ColorTranslator.FromWin32(value & 0xffffff); }
    public static int Encode(Color value) { return value.R | value.G << 8 | value.B << 16; }
}

class StyleDialog : Form {
    SubtitleStyle style;
    public SubtitleStyle ResultStyle {get;private set;}
    readonly Panel preview = new Panel();
    readonly NumericUpDown opacity = new NumericUpDown();
    readonly ComboBox thickness = new ComboBox{DropDownStyle=ComboBoxStyle.DropDownList};
    readonly CheckBox outline = new CheckBox();
    Font previewFont;
    public StyleDialog():this(SubtitleStyle.Load(),null,null,false){}
    public StyleDialog(SubtitleStyle settings,string sample,SubtitleStyle inherited,bool inheritOnMatch,int automaticRadius=2) {
        bool forZone=inherited!=null;
        var defaults=inherited??new SubtitleStyle();
        style=settings;
        Font=new Font("Segoe UI",10); Text=Ui.Get(Msg.Appearance);
        RightToLeft=Ui.IsRtl?RightToLeft.Yes:RightToLeft.No;RightToLeftLayout=Ui.IsRtl;
        StartPosition=FormStartPosition.CenterParent; FormBorderStyle=FormBorderStyle.FixedDialog;
        if(forZone)Text=Editor.T("Вигляд тексту зони","Zone text appearance");
        MinimizeBox=MaximizeBox=false;ClientSize=new Size(500,379);
        preview.SetBounds(16,16,468,90);
        preview.Paint+=delegate(object sender,PaintEventArgs e){
            using(var fmt=new StringFormat{Alignment=StringAlignment.Center,LineAlignment=StringAlignment.Center})
            using(var ink=new SolidBrush(SubtitleStyle.Decode(style.color))){
                e.Graphics.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
                style.DrawText(e.Graphics,String.IsNullOrWhiteSpace(sample)?Ui.Get(Msg.Title):sample,previewFont,ink,preview.ClientRectangle,fmt,automaticRadius);
            }
        };
        Controls.Add(preview);
        var font=ButtonAt(Msg.Font,16,117,224);
        font.Click+=delegate {
            using(var initial=new Font(style.face,style.size*72f/96f,(FontStyle)style.flags))
            using(var dialog=new FontDialog {Font=initial,Color=SubtitleStyle.Decode(style.color),ShowColor=true,MinSize=6,MaxSize=187}) {
                if(dialog.ShowDialog(this)!=DialogResult.OK)return;
                style.face=dialog.Font.Name;style.flags=(int)dialog.Font.Style;
                style.size=(int)Math.Round(dialog.Font.SizeInPoints*96/72);
                style.color=SubtitleStyle.Encode(dialog.Color);RefreshPreview();
            }
        };
        var background=ButtonAt(Msg.Background,254,117,230);
        background.Click+=delegate {
            using(var dialog=new ColorDialog {Color=SubtitleStyle.Decode(style.background),FullOpen=true}) {
                if(dialog.ShowDialog(this)!=DialogResult.OK)return;
                style.background=SubtitleStyle.Encode(dialog.Color);
                if(opacity.Value==0)opacity.Value=65;
                RefreshPreview();
            }
        };
        outline.Text=Ui.Get(Msg.Outline);outline.Checked=style.outline;
        outline.SetBounds(16,163,224,30);Controls.Add(outline);
        var edgeColor=new Button{Text=Editor.T("Колір контуру","Outline color"),UseVisualStyleBackColor=false};
        edgeColor.SetBounds(16,195,224,34);Controls.Add(edgeColor);
        var edgeWidth=new Label{Text=Editor.T("Товщина контуру","Outline width"),TextAlign=ContentAlignment.MiddleLeft};
        edgeWidth.SetBounds(16,240,222,30);Controls.Add(edgeWidth);
        thickness.Items.Add(Ui.Get(Msg.Automatic));
        for(int px=1;px<=SubtitleStyle.MaxOutlineSize;px++)thickness.Items.Add(px+Editor.T(" пікс."," px"));
        thickness.SetBounds(254,240,150,30);thickness.SelectedIndex=style.outline_size;Controls.Add(thickness);
        thickness.SelectedIndexChanged+=delegate{style.outline_size=thickness.SelectedIndex;preview.Invalidate();};
        Action updateSwatch=delegate{
            var value=SubtitleStyle.Decode(style.outline_color);
            edgeColor.BackColor=value;edgeColor.ForeColor=value.GetBrightness()<.5f?Color.White:Color.Black;
            edgeColor.Enabled=edgeWidth.Enabled=thickness.Enabled=style.outline;preview.Invalidate();
        };
        outline.CheckedChanged+=delegate{style.outline=outline.Checked;updateSwatch();};
        edgeColor.Click+=delegate{
            using(var dialog=new ColorDialog{Color=SubtitleStyle.Decode(style.outline_color),FullOpen=true}){
                if(dialog.ShowDialog(this)==DialogResult.OK){style.outline_color=SubtitleStyle.Encode(dialog.Color);updateSwatch();}
            }
        };
        updateSwatch();
        var amount=new Label{Text=Ui.Get(Msg.Opacity),AutoEllipsis=true};
        amount.SetBounds(254,163,225,24);Controls.Add(amount);
        opacity.SetBounds(254,190,100,26);opacity.Maximum=100;opacity.Value=style.opacity;
        opacity.ValueChanged+=delegate{style.opacity=(int)opacity.Value;RefreshPreview();};
        Controls.Add(opacity);
        var cancel=ButtonAt(Msg.Cancel,16,279,224); cancel.DialogResult=DialogResult.Cancel;
        var apply=ButtonAt(Msg.Apply,254,279,230);
        apply.Click+=delegate {
            style.outline=outline.Checked;
            if(forZone){
                ResultStyle=inheritOnMatch && Channel.Json.Serialize(style)==Channel.Json.Serialize(defaults)?null:style;
                DialogResult=DialogResult.OK;return;
            }
            try {style.Save();DialogResult=DialogResult.OK;}
            catch(IOException ex){MessageBox.Show(this,ex.Message,Ui.Get(Msg.Appearance));}
            catch(UnauthorizedAccessException ex){MessageBox.Show(this,ex.Message,Ui.Get(Msg.Appearance));}
        };
        var reset=new Button{Text=Editor.T("Відновити стандартний вигляд","Restore default appearance")};
        reset.SetBounds(16,326,468,36);Controls.Add(reset);
        reset.Click+=delegate{
            style=defaults.Copy();inheritOnMatch=true;
            opacity.Value=style.opacity;outline.Checked=style.outline;thickness.SelectedIndex=style.outline_size;
            updateSwatch();RefreshPreview();
        };
        AcceptButton=apply;CancelButton=cancel;RefreshPreview();
    }
    void RefreshPreview() {
        // Paint owns this font. WinForms can retain an equal Font assigned to Control.Font.
        if(previewFont==null || previewFont.OriginalFontName!=style.face ||
           previewFont.Size!=style.size || (int)previewFont.Style!=style.flags){
            var old=previewFont;
            previewFont=new Font(style.face,style.size,(FontStyle)style.flags,GraphicsUnit.Pixel);
            if(old!=null)old.Dispose();
        }
        preview.ForeColor=SubtitleStyle.Decode(style.color);
        var bg=SubtitleStyle.Decode(style.background);
        int alpha=style.opacity;
        preview.BackColor=Color.FromArgb((bg.R*alpha+96*(100-alpha))/100,
            (bg.G*alpha+96*(100-alpha))/100,(bg.B*alpha+96*(100-alpha))/100);
        preview.Invalidate();
    }
    protected override void Dispose(bool disposing) {
        base.Dispose(disposing); if(disposing && previewFont!=null)previewFont.Dispose();
    }
    Button ButtonAt(Msg key,int x,int y,int w) {
        var b=new Button{Text=Ui.Get(key),AutoEllipsis=true};b.SetBounds(x,y,w,40);Controls.Add(b);return b;
    }
}
