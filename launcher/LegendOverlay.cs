using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Text;
using System.Linq;

sealed class LegendEntry {
    public int[] bounds {get;set;}
    public string text {get;set;}
}
sealed class LegendSnapshot {
    public LegendEntry[] rows {get;set;}
    public int[][] bands {get;set;}
}
static class LegendOverlay {
    const int CanvasW=1280,CanvasH=720;
    static readonly RectangleF Canvas=new RectangleF(0,0,CanvasW,CanvasH);

    public static void Protect(Zone zone,LegendSnapshot snapshot,List<RectangleF> occupied){
        foreach(var b in snapshot.bands){
            // A controller glyph may be taller than its label or invisible to OCR.
            occupied.Add(new RectangleF(zone.Left-3,b[1]-1,zone.W+6,b[3]+2));
        }
        foreach(var row in snapshot.rows){
            var b=row.bounds;
            float extra=Math.Max(4,b[3]*.55f);
            float glyph=b[3]*2+6;
            occupied.Add(new RectangleF(b[0]+b[2],b[1]-extra,glyph,b[3]+2*extra));
            occupied.Add(new RectangleF(b[0]-glyph,b[1]-extra,glyph,b[3]+2*extra));
        }
    }

    public static void Draw(Graphics g,Zone zone,LegendSnapshot snapshot,SubtitleStyle style,
                            List<RectangleF> occupied,List<string> display){
        foreach(var row in snapshot.rows){
            var b=row.bounds;
            if(!String.IsNullOrWhiteSpace(row.text) && !PaintLabel(g,zone,
                new RectangleF(b[0],b[1],b[2],b[3]),row.text,style,occupied).IsEmpty)display.Add(row.text);
        }
    }
    static IEnumerable<PointF> Positions(Zone z,RectangleF source,SizeF size,bool side){
        float left=Math.Max(0,Math.Min(CanvasW-size.Width,source.Right-size.Width));
        float pad=Math.Max(4,source.Height*.55f)+2;
        if(!side){
            yield return new PointF(left,source.Top-pad-size.Height);
        }else{
            yield return new PointF(z.Left-size.Width-6,source.Top+(source.Height-size.Height)/2);
            yield return new PointF(z.Left+z.W+6,source.Top+(source.Height-size.Height)/2);
            yield return new PointF(left,source.Bottom+pad);
        }
    }
    static RectangleF PaintLabel(Graphics g,Zone zone,RectangleF source,string text,SubtitleStyle style,
                                 List<RectangleF> occupied){
        int initial=(int)Math.Max(11,Math.Min(Math.Min(20,style.size*.5),source.Height));
        using(var format=(StringFormat)StringFormat.GenericTypographic.Clone()){
            format.Alignment=StringAlignment.Center;
            for(int pass=0;pass<2;pass++){
                for(int px=initial;px>=9;px--){
                    using(var font=new Font(style.face,px,FontStyle.Bold,GraphicsUnit.Pixel)){
                        int limit=Math.Min(CanvasW-10,Math.Max(160,zone.W));
                        var m=g.MeasureString(text,font,limit,format);
                        var size=new SizeF((float)Math.Ceiling(m.Width)+8,(float)Math.Ceiling(m.Height)+2);
                        foreach(var point in Positions(zone,source,size,pass==1)){
                            var r=new RectangleF(point,size);
                            if(!Canvas.Contains(r) || occupied.Any(o=>o.IntersectsWith(r)))continue;
                            using(var bg=new SolidBrush(Color.FromArgb(242,17,21,23)))
                            using(var fg=new SolidBrush(Color.Gold)){
                                g.FillRectangle(bg,r);
                                g.TextRenderingHint=TextRenderingHint.AntiAliasGridFit;
                                g.DrawString(text,font,fg,new RectangleF(r.X+4,r.Y+1,r.Width-8,r.Height-2),format);
                            }
                            occupied.Add(r);
                            return r;
                        }
                    }
                }
            }
        }
        return RectangleF.Empty;
    }
    public static RectangleF Preview(Graphics g,Zone z,string text,SubtitleStyle style){
        var occupied=new List<RectangleF>();
        var area=new RectangleF(z.Left,z.Top,z.W,z.H);
        foreach(var item in new[]{text,Editor.T("Вийти","Exit")}){
            var row=new RectangleF(area.Left,area.Top+area.Height*(occupied.Count==0?.35f:.85f),area.Width*.78f,14);
            PaintLabel(g,z,row,item,style,occupied);
        }
        return area;
    }
}
