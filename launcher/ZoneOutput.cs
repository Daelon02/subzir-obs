using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

[Flags]
enum BoxEdge { None=0, Left=1, Right=2, Top=4, Bottom=8 }

// Optional in zones.json. Older profiles keep their content-sized, anchored layout.
sealed class ZoneOutput {
    public const int CanvasWidth=1280,CanvasHeight=720,MinWidth=24,MinHeight=14;
    public int X,Y,W,H;
    public Rectangle Bounds(){return new Rectangle(X,Y,W,H);}
    public bool Valid(){return X>=0 && Y>=0 && W>=MinWidth && H>=MinHeight &&
        W<=CanvasWidth-X && H<=CanvasHeight-Y;}

    static int Limit(int value,int min,int max){return Math.Max(min,Math.Min(value,max));}
    public static ZoneOutput Drag(RectangleF original,BoxEdge edge,PointF from,PointF to){
        var r=Rectangle.Round(original);
        int w=Limit(r.Width,MinWidth,CanvasWidth),h=Limit(r.Height,MinHeight,CanvasHeight);
        int x=Limit(r.X,0,CanvasWidth-w),y=Limit(r.Y,0,CanvasHeight-h);
        int dx=(int)Math.Round(to.X-from.X),dy=(int)Math.Round(to.Y-from.Y);
        int right=x+w,bottom=y+h;
        if(edge==BoxEdge.None){
            x=Limit(x+dx,0,CanvasWidth-w);y=Limit(y+dy,0,CanvasHeight-h);
            return new ZoneOutput{X=x,Y=y,W=w,H=h};
        }
        if((edge&BoxEdge.Left)!=0)x=Limit(x+dx,0,right-MinWidth);
        if((edge&BoxEdge.Right)!=0)right=Limit(right+dx,x+MinWidth,CanvasWidth);
        if((edge&BoxEdge.Top)!=0)y=Limit(y+dy,0,bottom-MinHeight);
        if((edge&BoxEdge.Bottom)!=0)bottom=Limit(bottom+dy,y+MinHeight,CanvasHeight);
        return new ZoneOutput{X=x,Y=y,W=right-x,H=bottom-y};
    }
    public static BoxEdge EdgeAt(RectangleF r,PointF p,float margin){
        var outer=r;outer.Inflate(margin,margin);
        if(!outer.Contains(p))return BoxEdge.None;
        float mx=Math.Min(margin,r.Width/3),my=Math.Min(margin,r.Height/3);
        var edge=BoxEdge.None;
        if(p.X<=r.Left+mx)edge|=BoxEdge.Left;
        else if(p.X>=r.Right-mx)edge|=BoxEdge.Right;
        if(p.Y<=r.Top+my)edge|=BoxEdge.Top;
        else if(p.Y>=r.Bottom-my)edge|=BoxEdge.Bottom;
        return edge;
    }
    public static Cursor CursorFor(BoxEdge edge){
        switch(edge){
            case BoxEdge.Left:case BoxEdge.Right:return Cursors.SizeWE;
            case BoxEdge.Top:case BoxEdge.Bottom:return Cursors.SizeNS;
            case BoxEdge.Left|BoxEdge.Top:case BoxEdge.Right|BoxEdge.Bottom:return Cursors.SizeNWSE;
            case BoxEdge.Right|BoxEdge.Top:case BoxEdge.Left|BoxEdge.Bottom:return Cursors.SizeNESW;
            default:return Cursors.SizeAll;
        }
    }
    public RectangleF Paint(Graphics g,Zone zone,string text,SubtitleStyle style){
        var box=Bounds();
        if(String.IsNullOrEmpty(text))return box;
        bool hud=zone.Hud||zone.Legend;
        if(zone.Hud && !zone.Legend)text=text.ToUpperInvariant();
        int padX=Math.Min(hud?5:12,box.Width/8),padY=Math.Min(hud?1:12,box.Height/8);
        var body=Rectangle.Inflate(box,-padX,-padY);
        float size=style.ZoneFontSize(zone);
        var flags=hud && zone.Style==null?FontStyle.Bold:(FontStyle)(style.flags&15);
        var state=g.Save();
        try{
            g.SetClip(box,CombineMode.Intersect);
            g.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
            using(var fmt=new StringFormat{Alignment=StringAlignment.Center,LineAlignment=StringAlignment.Center})
            using(var ink=new SolidBrush(hud && zone.Style==null?(zone.Legend?Color.Gold:Color.FromArgb(255,212,70)):SubtitleStyle.Decode(style.color)))
            using(var back=new SolidBrush(hud && zone.Style==null?Color.FromArgb(240,17,21,23):
                Color.FromArgb(style.opacity*255/100,SubtitleStyle.Decode(style.background)))){
                while(true){
                    using(var font=new Font(style.face,size,flags,GraphicsUnit.Pixel)){
                        var measured=g.MeasureString(text,font,body.Width,fmt);
                        if((measured.Height<=body.Height && measured.Width<=body.Width) || size<=1){
                            var bg=box;
                            if(hud){
                                bg.Width=Math.Min(box.Width,(int)Math.Ceiling(measured.Width)+2*padX);
                                bg.Height=Math.Min(box.Height,(int)Math.Ceiling(measured.Height)+2*padY);
                                bg.Offset((box.Width-bg.Width)/2,(box.Height-bg.Height)/2);
                            }
                            g.FillRectangle(back,bg);
                            style.DrawText(g,text,font,ink,body,fmt,hud?1:2);
                            return box;
                        }
                        size=Math.Max(1,Math.Min(size-1,size*Math.Min(body.Height/measured.Height,body.Width/measured.Width)));
                    }
                }
            }
        }finally{g.Restore(state);}
    }
}
