using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Linq;
using System.Windows.Forms;

sealed class Zone {
    public string Name, Text;
    public float X,Y,Width;
    public int Left,Top,W,H;
    public bool Hud;
    public float HudOffsetX,HudOffsetY;
    public Zone(){}
    public bool Valid(){return !String.IsNullOrWhiteSpace(Name) && Text!=null && Text.Length<=240 && Left>=0 && Top>=0 && W>=8 && H>=8 && W<=1280-Left && H<=720-Top && X>=0 && X<=1 && Y>=0 && Y<=1 && Width>=.12f && Width<=.94f && Math.Abs(HudOffsetX)<=1 && Math.Abs(HudOffsetY)<=1;}
    public Zone(string name,string text,float x,float y,float width) {
        Name=name; Text=text; X=x; Y=y; Width=width;
        Left=(int)(x*1280);Top=(int)(y*720);W=(int)(width*1280);H=62;
    }
    public override string ToString(){return Name;}
}

sealed class Stage : Control {
    public readonly List<Zone> Zones=new List<Zone>();
    public Zone Selected;
    public event Action SelectionChanged;
    public bool Guides=true;
    Bitmap photo;

    PointF grab;
    PointF hudOrigin,hudOffsets;
    bool dragging;
    const float W=1280,H=720,Pad=12;
    static readonly Color Accent=Color.FromArgb(115,235,207);

    readonly Font label=new Font("Segoe UI",15,FontStyle.Bold,GraphicsUnit.Pixel);

    public bool DrawInput;
    bool drawing;
    PointF anchor,tip;
    public Stage(){
        DoubleBuffered=true;ResizeRedraw=true;BackColor=Color.FromArgb(21,25,27);
    }
    public void ReplacePhoto(Bitmap image){
        var next=new Bitmap(image);if(photo!=null)photo.Dispose();photo=next;Invalidate();
    }
    public static RectangleF DrawCaption(Graphics g,Zone z,string text,SubtitleStyle style){
        if(z.Hud)return DrawHud(g,z,text,style);
        float w=z.Width*W;
        using(var font=new Font(style.face,style.size*2f/3f,(FontStyle)(style.flags & 15),GraphicsUnit.Pixel))
        using(var fmt=new StringFormat{Alignment=StringAlignment.Center})
        using(var back=new SolidBrush(Color.FromArgb(style.opacity*255/100,SubtitleStyle.Decode(style.background))))
        using(var ink=new SolidBrush(SubtitleStyle.Decode(style.color))){
            float h=Math.Min(H,(float)Math.Ceiling(g.MeasureString(text,font,(int)(w-2*Pad),fmt).Height)+2*Pad);
            var r=new RectangleF(Math.Min(W-w,z.X*W),Math.Min(H-h,z.Y*H),w,h);
            if(text.Length==0)return r;
            g.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
            g.FillRectangle(back,r);
            var body=new RectangleF(r.X+Pad,r.Y+Pad,w-2*Pad,h-2*Pad);
            if(style.outline)foreach(var d in new[]{new Point(-2,0),new Point(2,0),new Point(0,-2),new Point(0,2)}){
                var outline=body;outline.Offset(d.X,d.Y);g.DrawString(text,font,Brushes.Black,outline,fmt);
            }
            g.DrawString(text,font,ink,body,fmt);return r;
        }
    }
    static RectangleF DrawHud(Graphics g,Zone zone,string text,SubtitleStyle style){
        const float px=5,py=1,gap=2;
        string caption=text.ToUpperInvariant();
        using(var font=new Font(style.face,Math.Max(10,Math.Min(24,style.size*.4f)),FontStyle.Bold,GraphicsUnit.Pixel))
        using(var fmt=(StringFormat)StringFormat.GenericTypographic.Clone())
        using(var back=new SolidBrush(Color.FromArgb(240,17,21,23)))
        using(var ink=new SolidBrush(Color.FromArgb(255,212,70))){
            fmt.Alignment=StringAlignment.Center;
            var measured=g.MeasureString(caption.Length==0?" ":caption,font,(int)(zone.Width*W-2*px),fmt);
            float width=Math.Min(zone.Width*W,(float)Math.Ceiling(measured.Width)+2*px);
            float height=Math.Min(H,(float)Math.Ceiling(measured.Height)+2*py);
            float above=zone.Top-height-gap;
            float top=above>=0?above:zone.Top+zone.H+gap;
            float x=Math.Max(0,Math.Min(W-width,zone.Left+zone.W-width+zone.HudOffsetX*W));
            float y=Math.Max(0,Math.Min(H-height,top+zone.HudOffsetY*H));
            var r=new RectangleF(x,y,width,height);
            if(caption.Length==0)return r;
            var state=g.Save();
            try{
                g.SmoothingMode=SmoothingMode.AntiAlias;
                g.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
                using(var path=new GraphicsPath()){
                    float d=Math.Min(10,Math.Min(width,height));
                    path.AddArc(x,y,d,d,180,90);path.AddArc(r.Right-d,y,d,d,270,90);
                    path.AddArc(r.Right-d,r.Bottom-d,d,d,0,90);path.AddArc(x,r.Bottom-d,d,d,90,90);
                    path.CloseFigure();g.FillPath(back,path);
                }
                g.DrawString(caption,font,ink,new RectangleF(x+px,y+py,width-2*px,height-2*py),fmt);
            }finally{g.Restore(state);}
            return r;
        }
    }
    readonly SubtitleStyle style=SubtitleStyle.Load();
    readonly Dictionary<Zone,RectangleF> hit=new Dictionary<Zone,RectangleF>();
    Rectangle InputRect(){
        return Rectangle.FromLTRB((int)Math.Min(anchor.X,tip.X),(int)Math.Min(anchor.Y,tip.Y),
            (int)Math.Max(anchor.X,tip.X),(int)Math.Max(anchor.Y,tip.Y));
    }
    public void Render(Graphics g,bool guides){
        g.InterpolationMode=InterpolationMode.HighQualityBicubic;
        if(photo!=null)g.DrawImage(photo,new RectangleF(0,0,W,H));
        hit.Clear();
        foreach(var z in Zones){
            var r=DrawCaption(g,z,z.Text,style);hit[z]=r;
            if(!guides)continue;
            using(var pen=new Pen(z==Selected?Accent:Color.Silver,z==Selected?3:1))g.DrawRectangle(pen,r.X,r.Y,r.Width,r.Height);
            float y=Math.Max(0,r.Y-24);
            g.FillRectangle(Brushes.Black,r.X,y,g.MeasureString(z.Name,label).Width+10,24);
            g.DrawString(z.Name,label,Brushes.White,r.X+5,y+2);
            if(z==Selected){
                var rect=drawing?InputRect():new Rectangle(z.Left,z.Top,z.W,z.H);
                using(var pen=new Pen(Color.Gold,3){DashStyle=DashStyle.Dash})g.DrawRectangle(pen,rect);
            }
        }
    }
    float ScaleFactor {get{return Math.Min(ClientSize.Width/W,ClientSize.Height/H);}}
    PointF Origin {get{return new PointF((ClientSize.Width-W*ScaleFactor)/2,(ClientSize.Height-H*ScaleFactor)/2);}}
    PointF ScenePoint(Point pt){var o=Origin;return new PointF((pt.X-o.X)/ScaleFactor,(pt.Y-o.Y)/ScaleFactor);}
    protected override void OnPaint(PaintEventArgs e){
        base.OnPaint(e);if(ScaleFactor<=0)return;
        var o=Origin;e.Graphics.TranslateTransform(o.X,o.Y);e.Graphics.ScaleTransform(ScaleFactor,ScaleFactor);
        e.Graphics.SetClip(new RectangleF(0,0,W,H));Render(e.Graphics,Guides);
    }
    protected override void OnMouseDown(MouseEventArgs e){
        base.OnMouseDown(e);if(!Guides||e.Button!=MouseButtons.Left)return;
        {
            var p=ScenePoint(e.Location);
            if(DrawInput){if(p.X<0||p.Y<0||p.X>=W||p.Y>=H)return;anchor=tip=p;drawing=true;Capture=true;Invalidate();return;}
            for(int i=Zones.Count-1;i>=0;i--){
                var z=Zones[i];RectangleF r;if(!hit.TryGetValue(z,out r)||!r.Contains(p))continue;
                Selected=z;grab=new PointF(p.X-r.X,p.Y-r.Y);dragging=true;Capture=true;
                hudOrigin=r.Location;hudOffsets=new PointF(z.HudOffsetX,z.HudOffsetY);
                if(SelectionChanged!=null)SelectionChanged();Invalidate();break;
            }
        }
    }
    protected override void OnMouseMove(MouseEventArgs e){
        base.OnMouseMove(e);var p=ScenePoint(e.Location);
        if(drawing){tip=new PointF(Math.Max(0,Math.Min(W,p.X)),Math.Max(0,Math.Min(H,p.Y)));Invalidate();return;}
        RectangleF r;if(!dragging||!hit.TryGetValue(Selected,out r))return;
        float x=Math.Max(0,Math.Min(W-r.Width,p.X-grab.X));
        float y=Math.Max(0,Math.Min(H-r.Height,p.Y-grab.Y));
        if(Selected.Hud){
            Selected.HudOffsetX=Math.Max(-1,Math.Min(1,hudOffsets.X+(x-hudOrigin.X)/W));
            Selected.HudOffsetY=Math.Max(-1,Math.Min(1,hudOffsets.Y+(y-hudOrigin.Y)/H));
        }else{Selected.X=x/W;Selected.Y=y/H;}
        Invalidate();
    }
    protected override void OnMouseUp(MouseEventArgs e){
        if(drawing){var r=InputRect();if(r.Width>=8&&r.Height>=8){Selected.Left=r.X;Selected.Top=r.Y;Selected.W=r.Width;Selected.H=r.Height;}}
        drawing=dragging=false;Capture=false;Invalidate();base.OnMouseUp(e);
    }
    protected override void OnMouseCaptureChanged(EventArgs e){if(!Capture){drawing=dragging=false;}base.OnMouseCaptureChanged(e);}
    protected override void Dispose(bool disposing){if(disposing){if(photo!=null)photo.Dispose();label.Dispose();}base.Dispose(disposing);}
}

sealed class Editor : Form {
    readonly Stage stage=new Stage();
    readonly ComboBox zones=new ComboBox{DropDownStyle=ComboBoxStyle.DropDownList,Width=130};
    readonly TextBox sample=new TextBox{Width=350,MaxLength=240};
    readonly NumericUpDown width=new NumericUpDown{Minimum=12,Maximum=94,Width=52};
    bool syncing;
    static Button Btn(string text,EventHandler click){
        var b=new Button{Text=text,Height=32,AutoSize=true};b.Click+=click;return b;
    }
    static Label Caption(string text){return new Label{Text=text,AutoSize=true,Margin=new Padding(8,9,2,0)};}
    internal static string T(string uk,string en){return Ui.Language=="uk"?uk:en;}
    internal Zone[] LayoutZones {get{return stage.Zones.ToArray();}}
    public Editor(Bitmap frame,Zone[] initial){
        stage.ReplacePhoto(frame);stage.Zones.AddRange(initial);stage.Selected=initial[0];
        Text=T("SubZir · Редактор зон","SubZir · Zone editor");Font=new Font("Segoe UI",10);
        ClientSize=new Size(1090,695);MinimumSize=new Size(620,450);StartPosition=FormStartPosition.CenterScreen;
        var top=new FlowLayoutPanel{Dock=DockStyle.Top,AutoSize=true,Padding=new Padding(5)};
        var bottom=new FlowLayoutPanel{Dock=DockStyle.Bottom,AutoSize=true,Padding=new Padding(5)};
        var draw=new CheckBox{Text=T("Виділити текст у грі","Draw OCR area"),Appearance=Appearance.Button,AutoSize=true};
        draw.CheckedChanged+=delegate{stage.DrawInput=draw.Checked;stage.Guides=true;stage.Invalidate();};top.Controls.Add(draw);
        var add=Btn(T("Додати зону","Add zone"),delegate{
            if(stage.Zones.Count>=8)return;
            int n=1;string name;do{name=T("Зона ","Zone ")+(n++);}while(stage.Zones.Any(item=>item.Name==name));
            string example=stage.Zones.Count==1?T("Знайди човен біля пристані.","Find the boat by the dock."):
                stage.Zones.Count==2?T("Натисни △, щоб взаємодіяти.","Press △ to interact."):T("Приклад перекладу","Example translation");
            var z=new Zone(name,example,.36f,.43f,.30f){Hud=true};
            stage.Zones.Add(z);zones.Items.Add(z);zones.SelectedItem=z;draw.Checked=true;stage.Invalidate();
        });top.Controls.Add(add);
        top.Controls.Add(Btn(T("Видалити","Remove"),delegate{
            if(stage.Zones.Count<2)return;
            var removed=stage.Selected;zones.SelectedItem=stage.Zones.First(z=>z!=removed);
            stage.Zones.Remove(removed);zones.Items.Remove(removed);stage.Invalidate();
        }));
        var clean=new CheckBox{Text=T("Без рамок","Hide guides"),AutoSize=true,Margin=new Padding(12,10,6,0)};
        clean.CheckedChanged+=delegate{if(clean.Checked)draw.Checked=false;stage.Guides=!clean.Checked;stage.Invalidate();};top.Controls.Add(clean);
        draw.CheckedChanged+=delegate{if(draw.Checked)clean.Checked=false;};
        top.Controls.Add(Btn(T("Зберегти зони","Save zones"),delegate{DialogResult=DialogResult.OK;}));
        top.Controls.Add(Caption(T("Жовта рамка зчитує текст. Вимкни виділення, щоб перетягнути переклад.","Yellow box reads text. Turn off drawing to drag a translation.")));
        bottom.Controls.Add(Caption(T("Зона","Zone")));bottom.Controls.Add(zones);bottom.Controls.Add(Caption(T("Приклад","Sample")));bottom.Controls.Add(sample);
        bottom.Controls.Add(Caption(T("Ширина, %","Width, %")));bottom.Controls.Add(width);
        var hud=new CheckBox{Text=T("HUD над оригіналом","HUD above original"),AutoSize=true,Margin=new Padding(8,8,2,0)};
        bottom.Controls.Add(hud);
        hud.CheckedChanged+=delegate{if(!syncing){stage.Selected.Hud=hud.Checked;stage.Invalidate();}};
        foreach(var z in stage.Zones)zones.Items.Add(z);
        zones.SelectedIndexChanged+=delegate{
            var z=(Zone)zones.SelectedItem;stage.Selected=z;syncing=true;
            sample.Text=z.Text;width.Value=(decimal)Math.Round(z.Width*100);hud.Checked=z.Hud;syncing=false;stage.Invalidate();
        };
        sample.TextChanged+=delegate{if(!syncing){stage.Selected.Text=sample.Text;stage.Invalidate();}};
        width.ValueChanged+=delegate{if(!syncing){stage.Selected.Width=(float)width.Value/100;stage.Invalidate();}};
        stage.SelectionChanged+=delegate{zones.SelectedItem=stage.Selected;};
        zones.SelectedIndex=0;stage.Dock=DockStyle.Fill;Controls.Add(stage);Controls.Add(top);Controls.Add(bottom);
    }
}
