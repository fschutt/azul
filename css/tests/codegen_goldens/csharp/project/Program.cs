// Copy Azul.cs (target/codegen) and the azul library next to this file.
using System;

class Program
{
    static void Main()
    {
        var styleBtn = AzulStyles.Styles.StyleBtn();
        Console.WriteLine("StyleBtn: " + styleBtn.len + " properties");
    }
}
