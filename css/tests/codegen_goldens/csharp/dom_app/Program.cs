// Copy Azul.cs (target/codegen) and the azul library next to this file.
using System;

class Program
{
    static void Main()
    {
        AzulStyles.Styles.RenderUi();
        Console.WriteLine("RenderUi: built");
    }
}
