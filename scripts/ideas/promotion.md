# Promotion: non-western and per-language outreach for the "call for testing"

Researched 2026-09-19 with headless Chrome over CDP (DuckDuckGo HTML search, 6 tabs in
parallel), then **checked every forum, Discord, Telegram and mailing list for activity**:

| Mark | Meaning | How it was measured |
|---|---|---|
| ✅ | active: posts in the last 7 days | Discourse `/latest.json`, dated posts on the page, pipermail message counts |
| 🟡 | slow: last post 1–4 weeks ago, or only a trickle | same |
| ❌ | dead: nothing for months, or moved away | same |
| 👥 N/M | Discord / Telegram group: N members, M online right now | Discord invite API `with_counts`, `t.me/<name>` page |
| — | not measured: huge site that is obviously alive (Qiita, Zenn, V2EX, Habr, Reddit…) | |

Only the **shipped tier** languages are covered (`SHIPPED_LANGUAGES`,
`doc/src/docgen/mod.rs`): C, C++, Rust, C#, Java, Kotlin, Lua, Ruby, JS (Node), OCaml,
Zig, Go, Pascal, Scala, Fortran, Haskell, Python.

General rules:

- **Write natively.** Machine-translate a draft, then have a native speaker in the local
  Rust or language Discord check it. A post in the local language gets far more goodwill.
- **Show, don't advertise.** Every place below punishes plain ads. Post a build log with a
  screenshot, a 30-line hello-world *in that community's language*, and what's still broken.
- **The per-language angle is the hook.** "A new native GUI framework for Fortran / OCaml /
  Free Pascal / Lua" is news to that community in a way "another Rust GUI" is not. Niche
  communities have fewer announcements, so yours gets read.
- Stagger posts (one community per day or two) so you can answer every comment.

---

## Part A: per language (shipped tier)

Pattern for each: the **global** announcement channel, then **regional** channels where
that pairing is unusually strong.

### Pascal (Free Pascal / Lazarus / Delphi): biggest niche win
Pascal is still huge in Russia, Brazil, China, Germany and the Spanish-speaking world,
and those communities rarely see new GUI toolkits.
- ✅ **Lazarus forum, "Third party" board** (forum.lazarus.freepascal.org, board 19): the right board for a new-library announcement.
- ✅ **fpc-pascal mailing list** (lists.freepascal.org): 115 messages so far in Sept. ✅ **lazarus mailing list** (lists.lazarus-ide.org): 13 in Sept.
- ✅ **Delphi-PRAXiS** (en.delphipraxis.net EN, www.delphipraxis.net DE): very active, posts from hours ago.
- Russia: ✅ **freepascal.ru/forum**, ✅ **Киберфорум Lazarus** (cyberforum.ru/lazarus), ✅ programmersforum.ru Lazarus board.
- China: ✅ **Lazarus中文社区** (fpccn.com), Baidu Tieba `lazarus吧`.
- Brazil: ✅ **Delphi Brasil** (delphibrasil.forumeiros.com), 🟡 DevMedia forum (37 days since last post).
- Spanish: ✅ **ClubDelphi** (clubdelphi.com/foros, has a Lazarus/FreePascal subforum), delphiaccess.com (couldn't check).
- ❌ Skip: delphimaster.net, the 2ccc.com Delphi forum (2 years), ShowDelphi (years), the Lazarus forum's *Spanish* board (318 days).

### Fortran: niche, scientific, underserved for GUIs
- ✅ **Fortran Discourse** (fortran-lang.discourse.group): the global hub, very active.
- Japan: **Fortran-jp** (fortran-jp.org).
- China: **Fortran Coder** (bbs.fcode.cn, couldn't check, requires JS), CSDN Fortran board, 计算化学公社 (bbs.keinsci.com) Fortran board.
- Russia: ✅ **Киберфорум Fortran**, ✅ forum.sources.ru Fortran board, linux.org.ru `fortran` tag.
- ❌ `t.me/fortran_ru` (1 member). comp.lang.fortran on Google Groups returned a rate-limit error; Usenet is effectively dead anyway.

### OCaml
- ✅ **discuss.ocaml.org**: use the `[ANN]` title prefix, which is the convention there.
- Japan: ✅ **OCaml.jp** (restarted; Slack + Discord "OCaml 日本語" 👥 117/31 online, ocamljp.connpass.com).
- China: ocaml.lang.ac.cn (Chinese mirror of ocaml.org), Douban OCaml group.
- Russia: ❌ `t.me/ocaml_ru` (33 members).
- caml-list (sympa.inria.fr): couldn't read. Most traffic moved to Discourse.

### Haskell
- ✅ **discourse.haskell.org**, Announcements category. Discord "WE LIKE HASKELL" 👥 15.6k/1.6k.
- Japan: **Haskell-jp** (haskell.jp, public Slack).
- Russia: 👥 **t.me/haskellru** 1.7k/427, an active chat. (The ruhaskell.org *site* is ❌ dead since 2022.)
- ❌ haskell-cafe mailing list: archive stops at Aug 2025. ❌ haskell.kr (2016).

### Zig
- ✅ **ziggit.dev**: Showcase category, busiest of all the forums checked. Discord 👥 21k/3.9k.
- China: ✅ **Zig 语言中文社区** (ziglang.cc + Google Group `zigcc`, which reposts articles), a very tight-knit community.
- Russia: 👥 **t.me/ziglang_ru** 636/152.

### Lua
- ✅ **lua-l**, now at **groups.google.com/g/lua-l** (the old lua-users.org archive is ❌ stale). `[ANN]` posts are the norm (several in Aug/Sept).
- Discord "/lua" 👥 1.3k/99.
- **Brazil is Lua's home** (PUC-Rio / Tecgraf, lua.inf.puc-rio.br). A Portuguese post in Lua BR and a note to LabLua is a nice gesture.
- Japan: "Programming Lua Japan" Google Group (couldn't read).
- China: OpenResty community (openresty.org/cn/community.html), which is a Lua-server crowd but big.
- ❌ `t.me/lua_ru` (64 members).

### Ruby
- Japan is Ruby's home: **ruby-jp Slack** (public invite on ruby-jp.github.io, very large), **ruby-list** ML (hyperkitty on ml.ruby-lang.org, couldn't read), RubyKaigi.
- China: ✅ **Ruby China** (ruby-china.org), replies from hours to days ago.
- Brazil: r/RubyBrasil, Ruby on Rails Brasil list of communities.
- Korea: ❌ rubykr Google Group (172 days).

### Kotlin
- 🟡 discuss.kotlinlang.org (5 threads/30 days); the real hub is **kotlinlang Slack** (#announcements / #compose-desktop).
- Russia: 👥 **t.me/kotlin_lang** 6.9k/1.6k, very active.
- Brazil: 👥 **Kotlin Devs Brasil** Discord 1k/40; kotlin.dev.br lists Telegram/Discord.
- Korea: Kotlin Korea (kotlin.kr, Facebook group). Japan: kotlinlang-jp Slack, Kotlin Fest (kotlin.connpass.com).
- China: kotlincn.net is ❌ a docs mirror, not a community.

### Scala
- ✅ **users.scala-lang.org**, Discord "Scala" 👥 9k/791.
- Russia: 👥 **t.me/scala_ru** 2.1k/627.
- Japan: ScalaJP (jp.scala-users.org), ScalaMatsuri.
- ❌ scalacn Google Group (3.7 years).

### Go
- ✅ **golang-nuts** (Google Group, active), ✅ forum.golangbridge.org, Discord Gophers 👥 44k/5.1k.
- China: ✅ **studygolang.com** (Go语言中文网, very active), ✅ learnku.com/go.
- Russia: 👥 **t.me/golang_ru** 1.6k/383.
- Japan: Gophers Japan (gophers.jp). Korea: GopherCon Korea; 🟡 golang-korea group (66 days).
- Brazil: golang.com.br/comunidade (❔ t.me/golangbr didn't respond). LatAm: Gophers LATAM.

### C / C++
- Russia: 👥 **t.me/ProCxx** 11k/1.75k, the big RU C++ chat. C++ Russia conference (cppconf.ru, CFP).
- Brazil: ✅ **ccppbrasil** Google Group (C & C++ Brasil), still active.
- Japan: cpprefjp community, "C++ の歩き方" community list (cppmap).
- Global: cpplang Slack (cppalliance.org/slack), r/cpp (Show & Tell threads).

### C# / .NET
- Discord "C#" 👥 50k/11k.
- Korea: ✅ **닷넷데브** (forum.dotnetdev.kr, Discourse, active).
- Russia: ✅ t.me/dotnetru (channel, posts weekly).
- Japan: .NET Lab (dotnetlab.connpass.com), .NET Conf JP.
- Brazil: DIO .NET Discord group, .NET Brasil.

### Java
- Brazil: **SouJava** (soujava.org.br, one of the world's biggest JUGs), ❌ GUJ forum (last post March 2026).
- Japan: **JJUG** (java-users.jp, JJUG CCC talks).
- Russia: 👥 t.me/javastart 4.7k/1.3k.
- Spanish: javaHispano, comunidad-hispana-jugs.

### JS / Node
- Spanish: **midudev** Discord 👥 109k/4.5k (the biggest Spanish-language dev Discord, JS-focused), **MoureDev** Discord 👥 123k/4k.
- Brazil: **Rocketseat** Discord 👥 255k/9.5k, NodeBR.
- Arabic: Hsoub Academy JS section, Elzero Web School (Arabic YouTube + academy, JS-heavy).
- Japan: JavaScript communities on Doorkeeper.

### Python
- Global: ✅ discuss.python.org (Community → "Show and tell"? verify category), Discord 👥 432k/38k.
- Japan: **python.jp Discord** 👥 5.7k/957 online, PyCon JP 2026 (Hiroshima).
- Spanish: **Python en Español** Discord 👥 13.5k/328; **Python Chile** Discord 👥 3k/56 + t.me/pythonchile 1k; PyAr (Argentina); Python Colombia.
- Brazil: ✅ **python-brasil** Google Group (still gets posts); python.org.br lists ~40 regional Telegram groups; t.me/pythonbrasil channel (🟡 last post 30 days ago).
- Arabic: "Python Arabic Community" (Pythonation, X `@python_ar`, Facebook), Python Egypt.
- Korea: Python Korea / PyCon Korea 2026.

### Rust (for the "native Rust GUI" framing)
- Japan: **Rust Developers JP** Discord 👥 353/99, **rust-jp Zulip** (rust-lang-jp.zulipchat.com), connpass meetups.
- Korea: **rust-kr Discord** 👥 3.4k/514 (the rust-kr.org site itself is a static page).
- China: ✅ **rustcc.cn** (+ daily 【Rust日报】), ✅ learnku.com/rust, 🟡 rustfan.net, 🟡 mzfoss.com (last post July).
- Russia: 👥 **t.me/rustlang_ru** 5.6k/1.5k, 👥 t.me/rust_beginners_ru 4.2k/1.1k. **❌ forum.rust-lang.ru / rustycrate: last post 156 days ago, skip.**
- Spanish: **RustLang en Español** Discord 👥 1.6k/111 + t.me/rust_lang_es 👥 612 + t.me/rust_es 👥 524; ✅ **rust-lang.ar**; BcnRust Discord 👥 215; Rust Colombia 👥 168; La Web del Programador Rust forum (✅ small).
- Brazil: 👥 **t.me/rustlangbr** 1.7k/79; **Rust Brasil** Discord (156 online per rustbrasil.com.br); ✅ rustlang.com.br blog (takes posts).
- Arabic: **Rust Arabia** Discord 👥 11.7k/341 (check it's not the *game*; the name is ambiguous), **Rust Arabic Community** Discord 👥 3.3k/766 (same caveat).
- Vietnam: 👥 t.me/rust_vn 327/28. Turkey: Rust Türkiye Topluluğu Discord.

---

## Part B: regions

### Japan: top priority (strongest response last time)
Japan rewards long, careful write-ups. The traffic comes from **Hatena Bookmark** picking
up a Zenn/Qiita article.

| Where | How to use it |
|---|---|
| — **zenn.dev** | Flagship article: "Rust製・多言語対応ネイティブGUI azul" with Python + C + Pascal examples. |
| — **qiita.com** | Cross-post a shorter version, or one per binding ("FortranでネイティブGUI"). |
| — **b.hatena.ne.jp** (hotentry/it) | Don't self-post. It picks up good Zenn/Qiita pieces. |
| ✅ **GeekNews JP** (ja.news.hada.io) | Submit the link. They covered the 2026 Rust GUI survey. |
| — catnose.me/lab/hackernews-ja | A Show HN that does well gets mirrored here automatically. |
| **Publickey** (publickey1.jp) | Email a short JP release note. |
| **窓の杜** (forest.watch.impress.co.jp, author form) / **Vector** | Submit the demo *apps* (AzWriter, AzPaint). |
| **CodeZine** (codezine.jp/offering) | Openly asks for article/interview pitches. |
| **Software Design** (gihyo), **Nikkei xTECH**, **GIGAZINE** tip form | Later, once there's a story. |
| Rust.Tokyo CFP, connpass meetups, 技術書典 | Talks / lightning talks / a small book. |
| Per-language JP channels | python.jp Discord, ruby-jp Slack, Haskell-jp Slack, OCaml.jp, Fortran-jp, JJUG, Gophers Japan, ScalaJP, Kotlin Fest, cpprefjp (see Part A). |

### Korea
- ✅ **GeekNews Show** (news.hada.io/show): self-posts welcome, very active. Best single channel.
- — **OKKY** (okky.kr), which has a dedicated promo board `/events/promote`; biggest KR dev community.
- ✅ **Disquiet** (disquiet.io), a maker/side-project community. Good for the demo apps.
- velog.io (write-up), 요즘IT (yozm.wishket.com, contributed articles), 커리어리.
- Per-language: rust-kr Discord, 닷넷데브, Python Korea, Kotlin Korea, GopherCon Korea.
- Full list: jihundev.github.io/awesome-developer-community-in-korea.

### China / Taiwan
- ✅ **rustcc.cn** + 【Rust日报】 (ask the editors), Rust语言周刊 (PR on github.com/rustcn-org/rust-weekly).
- — **V2EX 分享创造** (v2ex.com/go/create), behind Cloudflare, known very active.
- **HelloGitHub** (hellogithub.com), submit via their GitHub repo.
- OSCHINA 开源中国 (project index + news), InfoQ 中文 (infoq.cn/contribute, in-depth articles), 掘金 / SegmentFault / 知乎 / CSDN for cross-posts.
- Per-language: ✅ studygolang.com, ✅ Ruby China, ✅ Zig 中文社区, ✅ Lazarus中文社区, Fortran Coder.
- ❌ Linux 中国 (shut down), ❌ scalacn.
- Taiwan: iThome 鐵人賽 (Rust track 2026), Rust Taiwan (Facebook), PTT Soft_Job.

### Russian-speaking / CIS
The outlets you pasted (nag.ru, RUБЕЖ, D-Russia, Digital-Report, MSKIT) are
telecom/security/gov-IT trade press. They're alive but won't cover a GUI toolkit.
Where developers actually are:
- ✅ **linux.org.ru** (news submissions), ✅ **OpenNET** (opennet.ru, "add news" form). Both post daily.
- — **Habr**. The first article goes through the moderated Sandbox (Песочница).
- **Telegram is the RU forum now**: rustlang_ru 5.6k, ProCxx 11k, kotlin_lang 6.9k, javastart 4.7k, scala_ru 2.1k, haskellru 1.7k, golang_ru 1.6k, ziglang_ru 636 (members; see Part A).
- ✅ Киберфорум (rust / lazarus / fortran boards), ✅ freepascal.ru, Tproger (pitch), Хакер (recruiting paid authors).
- ❌ forum.rust-lang.ru / rustycrate (156 days), ❌ delphimaster.net.
- Ukraine: ✅ **DOU** (dou.ua/forums), posts from today. Write in Ukrainian.

### Arabic
Searching "Rust" in Arabic mostly finds the *video game*. Say "لغة Rust" / "لغة البرمجة راست".
Lead with the Python and JS bindings: that's where the Arabic audience is.
- ✅ **حسوب I/O** (io.hsoub.com/programming): the biggest Arabic Reddit-like community, still posting in 2026.
- **أكاديمية حسوب** (academy.hsoub.com/questions): Q&A. Answer existing GUI questions there.
- ✅ **مجتمع أسس / Aosus** (discourse.aosus.org, active), the largest Arabic FOSS community. Discord 👥 237/20, Telegram 👥 2.2k/143, Matrix. **Best fit for an open-source call for testing.**
- ✅ **مجتمع لينكس العربي** (linuxac.org forums): slow but alive (last post 3 days ago).
- **JOSA** (josa.ngo / josa.community), the Jordan Open Source Association.
- Discord: **Rust Arabia** 👥 11.7k/341 and **Rust Arabic Community** 👥 3.3k/766 (verify they're about the language, not the game); "أكبر مجتمع عربي" 👥 10.4k/144 (general dev).
- Media: **أراجيك تك** (arageek.com/tech, takes contributions via a form), **البوابة التقنية** (aitnews.com, news tips).
- Reddit: r/EgyptianDevelopers, r/Egypt_Developers. Saudi: SDC (Saudi Developers Community, X `@SDC_Saudi`). Maghreb: Moroccan Developers / Tunisian IT Discords.
- Facebook groups: Rust بالعربي, Python Egypt, Egyptian Developers, جروب المبرمجين العرب.
- Newsletter platform: Miswadda (miswadda.com) hosts Arabic tech newsletters; check for a dev one.
- ❌ arab-dev.forumfa.net (111 days). majara.dev and Elzero academy couldn't be checked.

### Spanish (Spain + LatAm)
- **midudev** Discord 👥 109k/4.5k and **MoureDev** Discord 👥 123k/4k: the two biggest Spanish-language dev communities. Both have "show your project" channels.
- **RustLang en Español** (Discord 1.6k + 2 Telegram groups + blog), ✅ rust-lang.ar, BcnRust, Rust Colombia, RustMX / Rust México.
- **Python en Español** 👥 13.5k, Python Chile, PyAr, Python Colombia.
- Reddit: r/programacion, r/devsarg (Argentina, large), r/devSpain, r/PERUDEVS.
- Media: **Genbeta** (genbeta.com/desarrollo), Código Facilito (has a partner-community program).
- Pascal: ✅ ClubDelphi. Java: javaHispano. Go: Gophers LATAM. Rust Latam conference (rustlatam.org).
- 🟡 ForoDev (most posts a year old), ✅ La Web del Programador (small).

### Brazil (Portuguese): a large, very online dev scene
- ✅ **TabNews** (tabnews.com.br): BR's HN, very active. Best single channel.
- **Rocketseat** Discord 👥 255k/9.5k, the biggest BR dev Discord.
- **r/brdev**: large and active.
- ✅ **Fórum iMasters** (posts from hours ago), ✅ dev.to `#braziliandevs`.
- Rust: t.me/rustlangbr 👥 1.7k, Rust Brasil Discord, rustlang.com.br blog (✅, takes articles).
- Per-language: python-brasil list ✅ + ~40 regional Python Telegrams, Kotlin Devs Brasil 👥 1k, ccppbrasil ✅, SouJava, NodeBR, golang.com.br, Delphi Brasil ✅, **Lua (born at PUC-Rio)**.
- Newsletter: Codecon "code (weekly)" (codecon.dev/newsletter).
- ❌ GUJ (March 2026), 🟡 DevMedia forum.

### Second wave
- India: r/developersIndia, ✅ **FOSS United forum** (forum.fossunited.org, very active), IndiaFOSS.
- Turkey: **Turkish Developers** Discord 👥 5.9k/245 (+ tdevelopers.tr forum), Rust Türkiye Discord, yazilimtoplulugu.com (couldn't check).
- Iran: Virgool (virgool.io), barnamenevisan.org, Quera.
- Vietnam: Viblo (🟡 can't confirm), t.me/rust_vn 👥 327.
- Indonesia: ✅ KASKUS Programmer Forum (slow), ✅ Skilvul forum (slow).
- Central Europe: 4programmers.net (PL, behind Cloudflare), Root.cz (CZ), prog.hu (HU).

---

## Part C: mailing lists that are still alive

| List | Status |
|---|---|
| golang-nuts (Google Groups) | ✅ active, [ANN] posts accepted |
| lua-l (groups.google.com/g/lua-l) | ✅ active, [ANN] convention |
| fpc-pascal (lists.freepascal.org) | ✅ 115 msgs in Sept |
| lazarus (lists.lazarus-ide.org) | ✅ 13 msgs in Sept |
| ccppbrasil (Google Groups) | ✅ recent posts |
| python-brasil (Google Groups) | ✅ recent posts |
| zigcc (Google Groups, CN) | ✅ article reposts |
| golang-korea | 🟡 66 days |
| rubykr | ❌ 172 days |
| scalacn | ❌ ~3.7 years |
| haskell-cafe | ❌ archive ends Aug 2025 (moved to Discourse) |
| lua-users.org lua-l archive | ❌ stale mirror (list moved to Google Groups) |
| caml-list, ruby-list / ruby-talk, programming-lua-japan | ❔ archives not machine-readable. Check by hand. |

## Part D: dead, skip these
forum.rust-lang.ru / rustycrate.ru · delphimaster.net · bbs.2ccc.com · ShowDelphi forum ·
Lazarus forum Spanish board · ruhaskell.org · haskell.kr · Linux 中国 · kotlincn.net (docs only) ·
GUJ · arab-dev.forumfa.net · haskell-cafe · scalacn · rubykr · t.me/fortran_ru, ocaml_ru, lua_ru (tiny).

## Suggested order
1. **Japan** (Zenn article → GeekNews JP → Rust Developers JP / python.jp Discord → Publickey → 窓の杜 → CodeZine).
2. **Per-language niche posts** in parallel, one per day: Lazarus third-party + fpc-pascal, Fortran Discourse, discuss.ocaml.org, Haskell Discourse, ziggit, lua-l, golang-nuts.
3. **China** (rustcc + 日报 → V2EX → HelloGitHub → Zig/Lazarus CN).
4. **Korea** (GeekNews Show → OKKY → rust-kr Discord).
5. **Brazil** (TabNews → r/brdev → Rust/Python/Lua BR) and **Spanish** (RustLangES → midudev/MoureDev → r/devsarg).
6. **Russian** (Telegram chats per language → LOR/OpenNET news on release → Habr).
7. **Arabic** (Aosus Discourse → Hsoub I/O → linuxac → Arabic Rust/Python Discords).

---

Tooling (session scratchpad; copy to `scripts/` to rerun):
`cdpp.mjs "?query" …` parallel search, `alive.mjs targets.txt` liveness check
(`d:` Discourse, `i:` Discord invite, `t:` Telegram, plain URL = page date scan).
Discord's invite API rate-limits bursts. Space the checks about 3 s apart.
