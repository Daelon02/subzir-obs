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
    internal sealed class Placement {
        public RectangleF Box;
        public int Size;
        public string Text;
        public string Face;
        public int StyleSize, Flags;
    }
    const int CanvasW=1280,CanvasH=720;
    static readonly RectangleF Canvas=new RectangleF(0,0,CanvasW,CanvasH);

    public static void Protect(Zone zone,LegendSnapshot snapshot,List<RectangleF> occupied){
        foreach(var b in snapshot.bands){
            // A controller glyph may be taller than its label or invisible to OCR.
            occupied.Add(zone.AttachedPanel?new RectangleF(b[0]-2,b[1]-1,b[2]+4,b[3]+2):
                new RectangleF(zone.Left-3,b[1]-1,zone.W+6,b[3]+2));
        }
        if(zone.AttachedPanel)return;
        foreach(var row in snapshot.rows){
            var b=row.bounds;
            float extra=Math.Max(4,b[3]*.55f);
            float glyph=b[3]*2+6;
            occupied.Add(new RectangleF(b[0]+b[2],b[1]-extra,glyph,b[3]+2*extra));
            occupied.Add(new RectangleF(b[0]-glyph,b[1]-extra,glyph,b[3]+2*extra));
        }
    }

    public static void Draw(Graphics g,Zone zone,LegendSnapshot snapshot,SubtitleStyle style,
                            List<RectangleF> occupied,List<string> display,NativeFrame frame=null){
        style=zone.Style??style;
        if(zone.Output!=null){
            var rows=snapshot.rows.OrderBy(row=>row.bounds[1]).ThenBy(row=>row.bounds[0]).ToArray();
            var box=zone.Output;
            for(int i=0;i<rows.Length;i++){
                if(String.IsNullOrWhiteSpace(rows[i].text))continue;
                int y=box.H*i/rows.Length, end=box.H*(i+1)/rows.Length;
                if(end<=y)continue;
                new ZoneOutput{X=box.X,Y=box.Y+y,W=box.W,H=end-y}.Paint(g,zone,rows[i].text,style,frame);
                display.Add(rows[i].text);
            }
            return;
        }
        var previous=new Dictionary<Rectangle,Placement>(zone.Placements);
        zone.Placements.Clear();
        var reserved=new Dictionary<LegendEntry,Placement>();
        foreach(var row in snapshot.rows){
            var b=row.bounds;
            var key=new Rectangle(b[0],b[1],b[2],b[3]);
            Placement place;
            if(previous.TryGetValue(key,out place) && place.Text==row.text && place.Face==style.face &&
                place.StyleSize==style.size && place.Flags==style.flags && !occupied.Any(o=>o.IntersectsWith(place.Box))){
                reserved.Add(row,place);occupied.Add(place.Box);
            }
        }
        foreach(var row in snapshot.rows){
            if(String.IsNullOrWhiteSpace(row.text))continue;
            var b=row.bounds;
            var key=new Rectangle(b[0],b[1],b[2],b[3]);
            Placement place;
            reserved.TryGetValue(row,out place);
            if(place!=null)occupied.Remove(place.Box);
            if(!PaintLabel(g,zone,key,row.text,style,occupied,frame,place,zone.Placements).IsEmpty)display.Add(row.text);
        }
    }
    static IEnumerable<PointF> Positions(Zone z,RectangleF source,SizeF size,bool side,List<RectangleF> occupied){
        if(z.AttachedPanel){
            float centered=Math.Max(0,Math.Min(CanvasW-size.Width,source.Left+(source.Width-size.Width)/2));
            float gap=source.Height>32?4:Math.Max(4,Math.Min(14,source.Height*.55f+2));
            bool above=z.Anchor==CaptionAnchor.AboveEach;
            float edge=above?source.Top-size.Height:source.Bottom;
            int sign=above?-1:1;
            if(!side){
                yield return new PointF(centered,edge+sign*gap);
            }
            // Walk the actual obstacle edges. A fixed stride can skip the only
            // free pixels between two controller glyphs in a tight menu.
            var position=new RectangleF(centered,edge+sign,size.Width,size.Height);
            var blocks=occupied.Where(r=>r.Right>centered && r.Left<position.Right);
            foreach(var block in above?blocks.OrderByDescending(r=>r.Bottom):blocks.OrderBy(r=>r.Top)){
                if(position.IntersectsWith(block))position.Y=above?block.Top-position.Height:block.Bottom;
            }
            if(side || Math.Abs(position.Y-edge)<=gap+4)yield return position.Location;
            yield break;
        }
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
                                 List<RectangleF> occupied,NativeFrame frame=null,Placement previous=null,
                                 Dictionary<Rectangle,Placement> placements=null){
        int initial=(int)(zone.Style==null?Math.Max(11,Math.Min(style.ZoneFontSize(zone),source.Height)):style.ZoneFontSize(zone));
        using(var format=(StringFormat)StringFormat.GenericTypographic.Clone()){
            format.Alignment=StringAlignment.Center;
            for(int pass=previous==null?0:-1;pass<2;pass++){
                for(int px=pass<0?previous.Size:initial;px>=(pass<0?previous.Size:Math.Min(9,initial));px--){
                    using(var font=new Font(style.face,px,zone.Style==null?FontStyle.Bold:(FontStyle)(style.flags&15),GraphicsUnit.Pixel)){
                        int limit=Math.Min(CanvasW-10,Math.Max(160,zone.W));
                        var m=g.MeasureString(text,font,limit,format);
                        var size=new SizeF((float)Math.Ceiling(m.Width)+8,(float)Math.Ceiling(m.Height)+2);
                        foreach(var point in pass<0?new[]{previous.Box.Location}:Positions(zone,source,size,pass==1,occupied)){
                            var r=new RectangleF(point,size);
                            if(!Canvas.Contains(r) || occupied.Any(o=>o.IntersectsWith(r)))continue;
                            using(var bg=new SolidBrush(zone.Style==null?Color.FromArgb(242,17,21,23):Color.FromArgb(style.opacity*255/100,SubtitleStyle.Decode(style.background))))
                            using(var fg=new SolidBrush(zone.Style==null?Color.Gold:SubtitleStyle.Decode(style.color))){
                                var body=new RectangleF(r.X+4,r.Y+1,r.Width-8,r.Height-2);
                                if(frame!=null)frame.Add(g,text,font,style,fg.Color,bg.Color,body,r,format,1,false);
                                else {g.FillRectangle(bg,r);
                                g.TextRenderingHint=TextRenderingHint.AntiAliasGridFit;
                                style.DrawText(g,text,font,fg,body,format,1);}
                            }
                            occupied.Add(r);
                            if(placements!=null)placements[Rectangle.Round(source)]=new Placement{
                                Box=r,Size=px,Text=text,Face=style.face,StyleSize=style.size,Flags=style.flags};
                            return r;
                        }
                    }
                }
            }
        }
        return RectangleF.Empty;
    }
    public static RectangleF Preview(Graphics g,Zone z,string text,SubtitleStyle style){
        if(z.Output!=null)return z.Output.Paint(g,z,text+"\n"+Editor.T("Вийти","Exit"),style);
        var occupied=new List<RectangleF>();
        var area=new RectangleF(z.Left,z.Top,z.W,z.H);
        foreach(var item in new[]{text,Editor.T("Вийти","Exit")}){
            var row=new RectangleF(area.Left,area.Top+area.Height*(occupied.Count==0?.35f:.85f),area.Width*.78f,14);
            PaintLabel(g,z,row,item,style,occupied);
        }
        return area;
    }
}
