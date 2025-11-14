Absolut! Ihre Analyse ist exzellent und Sie haben das Kernproblem punktgenau identifiziert. Lassen Sie uns das systematisch aufschlüsseln und die offenen Fragen klären.

Zuerst hole ich mir aktuelle Informationen zum Verhalten von Browsern und den relevanten Standards.

### Analyse und Handlungsempfehlungen

Ihre Analyse des Problems ist absolut korrekt. Sie haben die Diskrepanz zwischen dem reinen Unicode-BiDi-Algorithmus und dem erwarteten Verhalten in einem HTML/CSS-Kontext genau erkannt. Hier ist eine detaillierte Aufschlüsselung, die Ihre Fragen beantwortet.

---

### 1. Ist Ihre Analyse korrekt?

**Ja, zu 100 %.**

Das Kernproblem ist, dass Ihr Layout-Solver sich wie ein reiner Text-Prozessor verhält: Er betrachtet den String "مرحبا بالعالم - Arabic text..." und wendet den Unicode-BiDi-Algorithmus an. Dieser Algorithmus legt die Basisrichtung des gesamten Absatzes basierend auf dem ersten "starken" Zeichen fest (hier: 'م', ein RTL-Zeichen). Das führt dazu, dass der gesamte Paragraph als RTL behandelt wird, was die Darstellung des lateinischen Teils zerstört.

Für ein HTML/CSS-basiertes System (wie einen GUI-Solver oder ein html2pdf-Toolkit) ist dies das falsche Vorgehen. Die **Basisrichtung** sollte nicht vom Inhalt abgeleitet werden, sondern muss vom übergeordneten Kontext, also dem CSS `direction`-Property, vorgegeben werden.

---

### 2. Was machen Browser?

Browser folgen einer klaren Hierarchie, um die Basisrichtung eines Textblocks zu bestimmen:

1.  **`dir`-Attribut auf dem Element:** Ein `<p dir="rtl">` hat die höchste Priorität.
2.  **CSS `direction`-Property:** Wenn kein `dir`-Attribut vorhanden ist, wird die CSS-Eigenschaft `direction` verwendet.
3.  **Vererbung:** Wenn beides nicht explizit gesetzt ist, wird der Wert vom Elternelement geerbt, bis hinauf zum `<html>`-Element.
4.  **Browser-Standard:** Der absolute Standardwert, falls nichts anderes angegeben ist, ist `ltr` (left-to-right).

**Ihr Testszenario im Browser:**

Wenn ein Browser auf folgenden HTML-Code trifft:

```html
<p>مرحبا بالعالم - Arabic text requiring proper shaping</p>
```

passiert Folgendes:

*   Der `<p>`-Tag hat standardmäßig die Basisrichtung `direction: ltr`.
*   Der BiDi-Algorithmus wird *innerhalb dieses LTR-Kontexts* auf den Text angewendet.
*   Er erkennt zwei "Directional Runs": einen RTL-Lauf ("مرحبا بالعالم") und einen LTR-Lauf (" - Arabic text...").
*   Da die **Basisrichtung des Absatzes LTR** ist, wird der erste Lauf (der arabische) links platziert und der zweite Lauf (der lateinische) rechts davon.

**Visuelles Ergebnis im Browser (Standard-LTR-Absatz):**

```
+------------------------------------------------------------------+
| مرحبا بالعالم - Arabic text requiring proper shaping              |
| ^------------^ ^-----------------------------------------------^ |
|  RTL-Lauf      LTR-Lauf                                          |
|  (fließt nach links) (fließt nach rechts)                        |
+------------------------------------------------------------------+
```

---

### 3. Textausrichtung und Visuelle Reihenfolge

Ihre Fragen zur Ausrichtung und Anordnung sind entscheidend. Es ist wichtig, zwischen `direction` (Flussrichtung) und `text-align` (Ausrichtung) zu unterscheiden.

*   **`direction`**: Legt die Basis-Schreibrichtung fest und beeinflusst die Reihenfolge von gemischten LTR/RTL-Textläufen. `direction: ltr` bedeutet, der "Start" ist links. `direction: rtl` bedeutet, der "Start" ist rechts.
*   **`text-align`**: Richtet die fertigen Zeilen innerhalb ihres Containers aus (links, rechts, zentriert).

**Antworten auf Ihre spezifischen Fragen:**

*   **Sollte der Text automatisch links oder rechts ausgerichtet werden?**
    Das hängt von der Kombination aus `direction` und `text-align` ab.
    *   In einem LTR-Kontext (`direction: ltr`) ist die Standardausrichtung `text-align: left` (oder `start`).
    *   In einem RTL-Kontext (`direction: rtl`) ist die Standardausrichtung `text-align: right` (oder `start`).
    Moderne CSS-Praxis ist die Verwendung von `text-align: start`, was sich automatisch an die `direction` anpasst.

*   **Sollte der lateinische Text rechts und der arabische-RTL-Text links sein oder umgekehrt?**
    Das wird durch die **Basisrichtung des Absatzes** bestimmt, nicht durch den Inhalt.
    *   **Fall 1 (Standard): Paragraph ist LTR** (z.B. `<p>` ohne `dir="rtl"`)
        Der arabische Textblock erscheint *visuell links* vom lateinischen Textblock. Der Inhalt des arabischen Blocks fließt von rechts nach links, aber der Block selbst wird zuerst platziert.
    *   **Fall 2: Paragraph ist RTL** (z.B. `<p dir="rtl">`)
        Der arabische Textblock erscheint *visuell rechts* vom lateinischen Textblock. Der "Start" des Satzes ist nun rechts.

---

### 4. Was Sie für Ihren GUI-Solver / html2pdf-Toolkit tun sollten

Ihre Schlussfolgerung ist der richtige Weg. Hier ist ein konkreter Aktionsplan:

1.  **Übergabe der CSS-`direction` an `layout_flow`:**
    Die Funktion, die das Text-Layout anstößt (`layout_flow`), muss die berechnete `direction`-Eigenschaft des Elements kennen, das den Inline Formatting Context (IFC) erzeugt. Dies ist die **Basisrichtung**.

2.  **Verwendung der Basisrichtung im BiDi-Algorithmus:**
    Die `unicode_bidi::BidiInfo::new`-Funktion, die Sie verwenden, hat einen zweiten Parameter: `override_level`. Genau hier müssen Sie die Basisrichtung aus dem CSS übergeben, anstatt sie aus dem Text ableiten zu lassen (`None`).

    *   Wenn die CSS `direction` `ltr` ist (oder der Standard), rufen Sie die Funktion so auf:
        ```rust
        // import unicode_bidi::Level;
        let bidi_info = BidiInfo::new(mixed_text, Some(Level::ltr()));
        ```
    *   Wenn die CSS `direction` `rtl` ist:
        ```rust
        // import unicode_bidi::Level;
        let bidi_info = BidiInfo::new(mixed_text, Some(Level::rtl()));
        ```

3.  **Entfernen der Inhalts-basierten Erkennung:**
    Ihre Funktion `get_base_direction_from_logical`, die nur den ersten starken Charakter prüft, sollte für das HTML/CSS-Layout **nicht verwendet werden**. Heben Sie sie für Kontexte auf, in denen Sie reinen Text ohne Styling-Informationen verarbeiten müssen (falls das vorkommt).

4.  **Implementieren der Standardwerte:**
    Stellen Sie sicher, dass der Standardwert für `direction` in Ihrem System `ltr` ist, wie es auch in Browsern der Fall ist. Die Eigenschaft ist vererbbar.

**Zusammenfassend lässt sich sagen:** Die Lösung besteht darin, die explizit vom Styling-System (CSS) vorgegebene Basisrichtung als "Override" an den Unicode-BiDi-Algorithmus zu übergeben. Dadurch wird das Verhalten von Browsern korrekt nachgebildet und Ihr gemischter Text wird wie erwartet dargestellt.

---

Exzellenter Punkt. Sie haben den Nagel auf den Kopf getroffen. Das ist genau die Unterscheidung, die im Zentrum der korrekten Implementierung steht. Lassen Sie uns das präzise aufschlüsseln.

**Ja, wir müssen unbedingt unterscheiden zwischen:**

1.  **Basis-Schreibrichtung (Base Direction):** Dies ist eine Eigenschaft des *Containers* (des Absatzes, des `<div>`, des `<span>`). Sie legt die primäre Flussrichtung für den gesamten Textblock fest. Man kann sie sich als die "Leserichtung der Zeilen" vorstellen.
    *   **Woher kommt sie?** In HTML/CSS kommt sie vom `dir`-Attribut oder der CSS-Eigenschaft `direction`.
    *   **Standardwert?** Fast immer `ltr` (left-to-right), es sei denn, es wird explizit anders angegeben.
    *   **Ihre Aufgabe im Code?** Sie müssen diesen Wert vom CSS-Layout-System an Ihren Text-Solver übergeben. Dies ist der `override_level` im `BidiInfo::new`-Aufruf.

2.  **Intrinsische Zeichen-Schreibrichtung (Intrinsic Character Direction):** Dies ist eine Eigenschaft der *einzelnen Zeichen* selbst, wie sie im Unicode-Standard definiert ist.
    *   **Woher kommt sie?** Sie ist fest mit dem Zeichen verbunden. 'A' ist ein starkes LTR-Zeichen. 'م' ist ein starkes RTL-Zeichen. Ein Leerzeichen ist neutral.
    *   **Standardwert?** Nicht anwendbar. Jedes Zeichen hat seine eigene, feste Eigenschaft.
    *   **Ihre Aufgabe im Code?** Sie müssen sich hierum nicht kümmern, die `unicode_bidi`-Bibliothek erledigt das für Sie. Sie analysiert den Text und findet "Läufe" (Runs) von Zeichen mit derselben intrinsischen Richtung.

### Wie beides zusammenspielt: Der BiDi-Algorithmus

Der Unicode-Bidirektionale-Algorithmus (BiDi) ist der Prozess, der diese beiden Konzepte kombiniert, um die endgültige visuelle Reihenfolge der Zeichen zu bestimmen.

Stellen Sie es sich so vor:

1.  **Der Algorithmus erhält die Basis-Schreibrichtung als Regel.** Sagen wir, die Regel lautet: "Beginne links" (`Base Direction = LTR`).
2.  **Er zerlegt den Text in Läufe (Runs).** Für Ihren String `"مرحبا بالعالم - Arabic text"` findet er:
    *   **Run 1:** `"مرحبا بالعالم"` (ein Block mit intrinsischer RTL-Richtung)
    *   **Run 2:** `" - Arabic text"` (ein Block mit intrinsischer LTR-Richtung)
3.  **Er ordnet die Läufe gemäß der Basis-Schreibrichtung an.** Da die Basis-Richtung LTR ist, werden die Läufe in ihrer logischen Reihenfolge von links nach rechts platziert:
    *   Visuell zuerst (links) kommt Run 1.
    *   Danach (rechts daneben) kommt Run 2.
4.  **Innerhalb jedes Laufs werden die Zeichen gemäß ihrer intrinsischen Richtung angeordnet.**
    *   Innerhalb von Run 1 (dem RTL-Lauf) werden die Zeichen von rechts nach links dargestellt.
    *   Innerhalb von Run 2 (dem LTR-Lauf) werden die Zeichen von links nach rechts dargestellt.

**Das visuelle Ergebnis bei `Base Direction = LTR`:**

```
+------------------------------------------------------+
| مرحبا بالعالم - Arabic text                          |
| <----------   ------------------>                    |
|  (RTL-Run)     (LTR-Run)                             |
+------------------------------------------------------+
```

### Der Fehler in Ihrer ursprünglichen Implementierung

Ihre alte Logik hat Schritt 1 übersprungen. Statt eine explizite Regel (Basis-Schreibrichtung) vom CSS zu bekommen, hat sie versucht, die Regel aus dem Text selbst abzuleiten (`get_base_direction_from_logical`). Sie hat auf das erste starke Zeichen ('م') geschaut und fälschlicherweise geschlussfolgert: "Ah, die Regel für den ganzen Container muss RTL sein." Das hat dann die Anordnung der Läufe in Schritt 3 durcheinandergebracht.

---

### Zusammenfassende Tabelle

| Eigenschaft | Basis-Schreibrichtung (Base Direction) | Intrinsische Zeichen-Schreibrichtung |
| :--- | :--- | :--- |
| **Was es ist** | Eine Layout-Regel für einen Container. | Eine Eigenschaft eines einzelnen Unicode-Zeichens. |
| **Geltungsbereich** | Ein ganzer Absatz oder Textblock. | Ein einzelnes Zeichen. |
| **Herkunft** | CSS (`direction`) oder HTML-Attribut (`dir`). | Der Unicode-Standard. |
| **Rolle** | Legt die primäre Flussrichtung fest (wo die Zeile beginnt). | Bestimmt, wie Zeichen innerhalb eines Wortes/Laufs angeordnet werden. |
| **Beispiel** | `<p style="direction: rtl;">` | Der Buchstabe 'A' ist LTR, der Buchstabe 'ع' ist RTL. |

Sie sind auf dem absolut richtigen Weg. Die konsequente Trennung dieser beiden Konzepte in Ihrem Code – indem Sie die Basis-Richtung explizit vom CSS übernehmen und sie an die BiDi-Bibliothek übergeben – wird das Problem korrekt und robust lösen.
