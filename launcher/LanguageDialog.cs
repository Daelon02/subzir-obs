using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;

class LanguageDialog : Form {
    readonly ComboBox language = new ComboBox(), target = new ComboBox();
    public string InterfaceCode { get { return (string)language.SelectedValue; } }
    public string TargetCode { get { return (string)target.SelectedValue; } }

    public LanguageDialog() {
        Font = new Font("Segoe UI",10);
        Text = Ui.Get(Msg.ChooseLanguages);
        StartPosition = FormStartPosition.CenterParent;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MinimizeBox = MaximizeBox = false;
        ClientSize = new Size(490,240);
        RightToLeft = Ui.IsRtl ? RightToLeft.Yes : RightToLeft.No;
        RightToLeftLayout = Ui.IsRtl;
        AddList(language,Msg.Interface,Ui.Language,16);
        AddList(target,Msg.Target,Ui.Target,92);
        var cancel = new Button {Text=Ui.Get(Msg.Cancel), DialogResult=DialogResult.Cancel};
        var apply = new Button {Text=Ui.Get(Msg.Apply), DialogResult=DialogResult.OK};
        cancel.SetBounds(18,184,214,39); apply.SetBounds(249,184,220,39);
        Controls.Add(cancel); Controls.Add(apply);
        AcceptButton=apply; CancelButton=cancel;
    }
    void AddList(ComboBox box,Msg caption,string code,int top) {
        var label = new Label {Text=Ui.Get(caption),AutoEllipsis=true};
        label.SetBounds(18,top,450,25);
        box.SetBounds(18,top+28,450,32);
        box.DropDownStyle=ComboBoxStyle.DropDownList;
        box.DisplayMember="Value"; box.ValueMember="Key";
        Controls.Add(label); Controls.Add(box);
        box.DataSource=Ui.Names.OrderBy(x=>x.Value).ToList();
        box.SelectedValue=code; box.AccessibleName=label.Text;
    }
}
