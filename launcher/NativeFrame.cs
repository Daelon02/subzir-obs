using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// Coordinates use the same 1280 x 720 space as the zone editor.
sealed class NativeFrame {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool MoveFileEx(string from,string to,uint flags);
    const uint ReplaceExisting=1;
    static readonly string ScratchSuffix="."+Guid.NewGuid().ToString("N")+".tmp";
    static readonly Encoding Utf8=new UTF8Encoding(false);
    public static void Publish(string path,string content){
        // Fixed staging paths avoid creating a new filename for every streamed button.
        string tmp=path+ScratchSuffix;
        try{
            File.WriteAllText(tmp,content,Utf8);
            for(int attempt=0;!MoveFileEx(tmp,path,ReplaceExisting);attempt++){
                int error=Marshal.GetLastWin32Error();
                if((error!=5 && error!=32) || attempt>=15)
                    throw new IOException("Cannot publish zone layer",new System.ComponentModel.Win32Exception(error));
                Thread.Sleep(2);
            }
        }finally{if(File.Exists(tmp))File.Delete(tmp);}
    }
    public readonly List<object> Layers=new List<object>();
    public bool Fits=true;
    public int ZoneIndex;
    readonly string directory;
    readonly Dictionary<int,Backdrop> backdrops;
    internal sealed class Backdrop { public int Width,Height,Argb,Slot; public string Revision; }
    public NativeFrame(string directory,Dictionary<int,Backdrop> backdrops){
        this.directory=directory;this.backdrops=backdrops;
    }
    static int Px(float n){return (int)Math.Round(n);}
    string Rounded(int w,int h,Color color,out string revision){
        Backdrop old;
        backdrops.TryGetValue(ZoneIndex,out old);
        if(old==null || old.Width!=w || old.Height!=h || old.Argb!=color.ToArgb()){
            var next=new Backdrop{Width=w,Height=h,Argb=color.ToArgb(),Slot=old==null?0:1-old.Slot,
                                  Revision=Guid.NewGuid().ToString("N")};
            var file=Path.Combine(directory,FileName(next));
            var tmp=file+"."+next.Revision;
            try{
                using(var bmp=new Bitmap(w,h,PixelFormat.Format32bppArgb))
                using(var g=Graphics.FromImage(bmp))
                using(var path=new GraphicsPath())
                using(var brush=new SolidBrush(color)){
                    g.SmoothingMode=SmoothingMode.AntiAlias;
                    int d=Math.Min(10,Math.Min(w,h));
                    path.AddArc(0,0,d,d,180,90);path.AddArc(w-d,0,d,d,270,90);
                    path.AddArc(w-d,h-d,d,d,0,90);path.AddArc(0,h-d,d,d,90,90);
                    path.CloseFigure();g.FillPath(brush,path);bmp.Save(tmp,ImageFormat.Png);
                }
                if(File.Exists(file))File.Replace(tmp,file,null);else File.Move(tmp,file);
            }finally{if(File.Exists(tmp))File.Delete(tmp);}
            old=next;backdrops[ZoneIndex]=next;
        }
        revision=old.Revision;
        return FileName(old);
    }
    string FileName(Backdrop b){return "zone-back-"+ZoneIndex+"-"+b.Slot+".png";}
    public void Add(Graphics g,string text,Font font,SubtitleStyle style,Color ink,Color bg,
                    RectangleF body,RectangleF box,StringFormat fmt,int stroke,bool rounded){
        // Let the bitmap renderer preserve GDI+'s line breaks and clipping in tight boxes.
        var measured=g.MeasureString(text,font,Int32.MaxValue,fmt);
        if(text.IndexOfAny(new[]{'\r','\n'})>=0 || measured.Width>body.Width+1 || measured.Height>body.Height+1){
            Fits=false;return;
        }
        int w=Math.Max(1,Px(box.Width)),h=Math.Max(1,Px(box.Height));
        string revision="",image=rounded && bg.A>0?Rounded(w,h,bg,out revision):"";
        Layers.Add(new {
            x=Px(body.X),y=Px(body.Y),w=Math.Max(1,Px(body.Width)),h=Math.Max(1,Px(body.Height)),
            bx=Px(box.X),by=Px(box.Y),bw=w,bh=h,image,revision,
            background=(long)(uint)(SubtitleStyle.Encode(bg) | (bg.A<<24)), opacity=bg.A,
            cfg=new {text,font=new{face=font.Name,size=Math.Max(1,Px(font.GetHeight(g))),flags=(int)font.Style},
                     color=SubtitleStyle.Encode(ink),outline=style.outline,outline_color=style.outline_color,
                     outline_size=stroke*2,align="center",bk_opacity=0}
        });
    }
}
