//! Spoken code: how what a recogniser wrote for "C S S error" becomes the words of `CssError`. A
//! person says `CssError` as "see ess ess error" and a recogniser writes "see esses error" or "sees
//! error"; it says `enum` and gets "enemy", `await` and gets "awake", `HTTP request` and gets "HDP
//! request" or "age TTP request". Nothing here looks at the code: it turns a run of heard words into
//! the READINGS the words may be (lists of lower-case words, the form [`crate::symbols_tokens`] gives
//! a name), and the resolver asks the index which readings are names.
//!
//! - **Letters.** Letter names (`see`, `ess`, `esses` for a doubled `s`, `ay`, `aitch`, `jay`, `you`,
//!   `eye`, `double you` ...) and spelled letters (`c s s`, `C-S-S`) read as an acronym. A run
//!   of letters is an acronym only when the result is a listed one ([`KNOWN_ACRONYMS`], at least two
//!   letters) or a word of the workspace (at least three): "we can see errors" stays words, "are you"
//!   is not "ru".
//! - **Numbers.** `eight` is `8`, `thirty two` is `32`, `two fifty six` is `256`, as a person says
//!   them; a letter and a number (`u eight`, `i thirty two`) is the name of a type.
//! - **Mishearings.** A short table of what recognisers write for code words ([`HOMOPHONES`]),
//!   measured ones first: `enemy` for `enum`, `awake` for `await`, `jason` for `json`, `hdp` for `http`.
//! - **Identifiers.** A word the recogniser wrote as `CssError` or `css_error` is split into its words.
//!
//! Each of these is a reading BESIDE the words as heard, never instead of them; a [`Reading`] says
//! how it came about (`via`), so the resolver can weigh a reading that took a spelled-out acronym
//! more than the plain words. Everything is bounded: [`MAX_READINGS`] readings, a fixed budget of
//! steps, at most [`MAX_RUN`] words in a run.
//!
//! The stop words, the common words (a name made of these alone is a name a person says without
//! meaning the code) and the kind words (`struct`, `function`, `crate`) live here too.

use crate::{symbols::Kind, symbols_tokens::split_identifier};

/// A reading used spelled-out letters.
pub const VIA_LETTERS: u8 = 1;
/// A reading used the table of mishearings.
pub const VIA_HOMOPHONE: u8 = 2;
/// A reading used number words as digits.
pub const VIA_DIGITS: u8 = 4;
/// A reading used a word the recogniser wrote as an identifier (`CssError`).
pub const VIA_IDENTIFIER: u8 = 8;

/// A reading is a name written as one word (`scrollmanager`) that is the name of two or more words.
pub const VIA_JOINED: u8 = 16;

/// The most readings of one window.
pub const MAX_READINGS: usize = 12;
/// The most words a run of letters, of numbers or a mishearing covers.
pub const MAX_RUN: usize = 8;
/// The most steps one `readings` call takes.
const BUDGET: usize = 3_000;

/// One word as the recogniser gave it.
#[derive(Clone, Debug, PartialEq)]
pub struct HeardWord {
    /// As written, with its punctuation.
    pub text: String,
    /// How sure the recogniser is, 0 to 1; 0 or NaN where it does not say.
    pub probability: f32,
    /// The word is one of the vocabulary the recogniser was told to favour.
    pub biased: bool,
}

/// A heard word, cleaned for matching. A word spelled with hyphens or dots (`C-S-S`) is several of
/// these, all of the same heard word (`orig`).
#[derive(Clone, Debug, PartialEq)]
pub struct SpokenWord {
    /// The index of the heard word.
    pub orig: usize,
    /// The heard word as written.
    pub text: String,
    /// Lower case, without the punctuation around it.
    pub clean: String,
    /// `None` where the recogniser did not say.
    pub probability: Option<f32>,
    pub biased: bool,
    /// Written as an identifier: `CssError`, `css_error`, `utf8`.
    pub identifier_like: bool,
    /// Written as an acronym: `CSS`, `HTTP2`.
    pub allcaps: bool,
}

/// What a window of words may say: lower-case words, and how they came about (the `VIA_` bits; 0 is the
/// words as heard).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    pub tokens: Vec<String>,
    pub via: u8,
}

fn trim_edges(text: &str) -> &str {
    text.trim_matches(|c: char| !c.is_alphanumeric())
}

fn is_letter_part(part: &str) -> bool {
    let mut chars = part.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_alphabetic())
}

/// The shape of a written word: identifier-like (`CssError`, `css_error`, `utf8`) and acronym (`CSS`).
fn shape(text: &str) -> (bool, bool) {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let digits = text.chars().filter(char::is_ascii_digit).count();
    let upper = letters > 0 && text.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase);
    let mut lower_to_upper = false;
    let mut prev_lower = false;
    for c in text.chars() {
        if c.is_uppercase() && prev_lower {
            lower_to_upper = true;
        }
        prev_lower = c.is_lowercase();
    }
    let identifier = text.contains('_') || lower_to_upper || (letters > 0 && digits > 0 && !upper);
    (identifier, letters >= 2 && upper)
}

/// Cleans the heard words: lower case, the punctuation around a word gone, a word that is nothing but
/// punctuation dropped, a spelled-out word (`C-S-S`, `C.S.S.`) one word per letter.
#[must_use]
pub fn clean_words(words: &[HeardWord]) -> Vec<SpokenWord> {
    let mut out = Vec::with_capacity(words.len());
    for (orig, heard) in words.iter().enumerate() {
        let trimmed = trim_edges(&heard.text);
        if trimmed.is_empty() {
            continue;
        }
        let probability = (heard.probability.is_finite() && heard.probability > 0.0).then(|| heard.probability.min(1.0));
        let lower = trimmed.to_lowercase();
        let parts: Vec<&str> = lower.split(['-', '.']).collect();
        let (identifier_like, allcaps) = shape(trimmed);
        if parts.len() >= 2 && parts.len() <= MAX_RUN && parts.iter().all(|p| is_letter_part(p)) {
            for part in parts {
                out.push(SpokenWord {
                    orig,
                    text: heard.text.clone(),
                    clean: part.to_string(),
                    probability,
                    biased: heard.biased,
                    identifier_like: false,
                    allcaps: false,
                });
            }
            continue;
        }
        out.push(SpokenWord {
            orig,
            text: heard.text.clone(),
            clean: lower,
            probability,
            biased: heard.biased,
            identifier_like,
            allcaps,
        });
    }
    out
}


/// What a spoken word may be as a letter (a plural is the doubled letter: `esses` is `ss`). Sorted by the word: searched by
/// bisection. `double you` is two words and is read in code.
static LETTER_NAMES: [(&str, &str); 86] = [
    ("a", "a"),
    ("age", "h"),
    ("aitch", "h"),
    ("ar", "r"),
    ("are", "r"),
    ("ay", "a"),
    ("b", "b"),
    ("be", "b"),
    ("bee", "b"),
    ("bees", "bb"),
    ("c", "c"),
    ("cee", "c"),
    ("cue", "q"),
    ("d", "d"),
    ("dee", "d"),
    ("dees", "dd"),
    ("e", "e"),
    ("ee", "e"),
    ("eff", "f"),
    ("effs", "ff"),
    ("el", "l"),
    ("ell", "l"),
    ("els", "ll"),
    ("em", "m"),
    ("ems", "mm"),
    ("en", "n"),
    ("ens", "nn"),
    ("es", "s"),
    ("ess", "s"),
    ("esses", "ss"),
    ("ex", "x"),
    ("exes", "xx"),
    ("eye", "i"),
    ("eyes", "ii"),
    ("f", "f"),
    ("g", "g"),
    ("gee", "g"),
    ("gees", "gg"),
    ("h", "h"),
    ("haitch", "h"),
    ("i", "i"),
    ("j", "j"),
    ("jay", "j"),
    ("jays", "jj"),
    ("k", "k"),
    ("kay", "k"),
    ("kays", "kk"),
    ("l", "l"),
    ("m", "m"),
    ("n", "n"),
    ("o", "o"),
    ("oh", "o"),
    ("ohs", "oo"),
    ("p", "p"),
    ("pea", "p"),
    ("pee", "p"),
    ("pees", "pp"),
    ("q", "q"),
    ("queue", "q"),
    ("queues", "qq"),
    ("r", "r"),
    ("s", "s"),
    ("sea", "c"),
    ("seas", "cc"),
    ("see", "c"),
    ("sees", "cc"),
    ("t", "t"),
    ("tea", "t"),
    ("tee", "t"),
    ("tees", "tt"),
    ("u", "u"),
    ("v", "v"),
    ("vee", "v"),
    ("vees", "vv"),
    ("w", "w"),
    ("why", "y"),
    ("whys", "yy"),
    ("wye", "y"),
    ("x", "x"),
    ("y", "y"),
    ("yoo", "u"),
    ("you", "u"),
    ("z", "z"),
    ("zed", "z"),
    ("zeds", "zz"),
    ("zee", "z"),
];

/// Words that only join others (English and German); a window of words does not begin or end with one
/// unless a spelled-out reading says otherwise. Sorted: searched by bisection.
pub static STOP_WORDS: [&str; 245] = [
    "a", "aber", "about", "actually", "after", "again", "ah", "all", "alle", "allem", "allen", "aller", "alles",
    "als", "also", "am", "an", "and", "any", "anyway", "are", "as", "at", "auf", "aus", "basically", "be", "because",
    "been", "before", "bei", "being", "bin", "bis", "bist", "but", "by", "can", "could", "da", "dann", "das", "dass",
    "dein", "deine", "dem", "den", "der", "des", "did", "die", "dies", "diese", "dieser", "dieses", "do", "doch",
    "does", "doing", "done", "dort", "down", "du", "durch", "each", "ein", "eine", "einem", "einen", "einer",
    "eines", "else", "er", "es", "euch", "euer", "for", "from", "für", "gegen", "get", "gibt", "got", "hab", "habe",
    "haben", "had", "has", "hat", "hatte", "have", "having", "he", "her", "here", "hers", "hier", "him", "his",
    "hmm", "how", "i", "ich", "if", "ihr", "ihre", "im", "in", "into", "is", "ist", "it", "its", "ja", "jetzt",
    "just", "kann", "kein", "keine", "like", "man", "may", "maybe", "me", "mein", "meine", "mich", "might", "mir",
    "mit", "more", "most", "much", "muss", "must", "my", "nach", "nein", "nicht", "no", "noch", "nor", "not", "now",
    "nun", "nur", "ob", "oder", "of", "off", "oh", "ohne", "ok", "okay", "on", "once", "only", "or", "other", "our",
    "ours", "out", "over", "own", "really", "same", "sehr", "sein", "seine", "shall", "she", "should", "sich", "sie",
    "sind", "so", "soll", "some", "still", "such", "than", "that", "the", "their", "theirs", "them", "then", "there",
    "these", "they", "this", "those", "through", "to", "too", "uh", "um", "und", "under", "uns", "unser", "unsere",
    "until", "up", "upon", "us", "very", "von", "vor", "war", "waren", "was", "we", "weil", "well", "wenn", "wer",
    "werden", "were", "what", "when", "where", "which", "while", "who", "whom", "whose", "why", "wie", "will", "wir",
    "wird", "with", "wo", "wollen", "would", "yeah", "yes", "you", "your", "yours", "yourself", "zu", "zum", "zur",
];

/// Everyday words (the stop words and the frequent ones): a name made only of these is a name a person says
/// without meaning the code, so it needs more than its words to be taken for a definition. No word of the
/// code's own (`css`, `scroll`, `speech`, `enum`) is here. Sorted: searched by bisection.
pub static COMMON_WORDS: [&str; 915] = [
    "a", "aber", "about", "above", "act", "actually", "add", "after", "again", "age", "ago", "agree", "ah", "air",
    "all", "alle", "allem", "allen", "aller", "alles", "allow", "almost", "alone", "along", "already", "als", "also",
    "always", "am", "among", "amount", "an", "and", "angle", "animal", "answer", "any", "anyway", "appear", "apple",
    "are", "area", "arm", "army", "around", "arrive", "art", "as", "ask", "at", "auf", "aus", "away", "baby", "back",
    "bad", "bag", "ball", "bank", "base", "basically", "be", "bear", "beat", "beautiful", "because", "become", "bed",
    "been", "before", "begin", "behind", "bei", "being", "believe", "below", "best", "better", "between", "big",
    "bin", "bird", "bis", "bist", "bit", "black", "blood", "blue", "board", "boat", "body", "book", "born", "both",
    "bottom", "box", "boy", "break", "bring", "brother", "brown", "build", "burn", "business", "but", "buy", "by",
    "call", "came", "camp", "can", "car", "care", "carry", "case", "cat", "catch", "cause", "center", "century",
    "certain", "chair", "chance", "change", "check", "child", "choose", "city", "class", "clean", "clear", "close",
    "cloud", "code", "cold", "color", "come", "common", "company", "complete", "condition", "consider", "contain",
    "continue", "control", "cook", "cool", "copy", "corner", "correct", "cost", "could", "count", "country",
    "course", "cover", "create", "cross", "crowd", "cry", "cut", "da", "dann", "dark", "das", "dass", "data", "day",
    "dead", "deal", "dear", "death", "decide", "deep", "dein", "deine", "dem", "den", "der", "des", "develop", "did",
    "die", "dies", "diese", "dieser", "dieses", "difference", "different", "difficult", "direct", "do", "doch",
    "doctor", "does", "dog", "doing", "done", "door", "dort", "double", "down", "draw", "dream", "dress", "drink",
    "drive", "drop", "dry", "du", "durch", "during", "each", "early", "earth", "east", "easy", "eat", "edge",
    "effect", "egg", "ein", "eine", "einem", "einen", "einer", "eines", "else", "end", "enemy", "enough", "er",
    "error", "es", "euch", "euer", "even", "evening", "event", "every", "example", "except", "exist", "expect",
    "eye", "face", "fact", "fall", "family", "far", "farm", "fast", "father", "fear", "feel", "feet", "few", "field",
    "fight", "figure", "file", "fill", "final", "find", "fine", "finger", "finish", "fire", "first", "fish", "five",
    "fix", "floor", "flow", "fly", "follow", "food", "foot", "for", "force", "forest", "forget", "form", "found",
    "four", "free", "friend", "from", "front", "full", "fun", "für", "game", "garden", "gave", "gegen", "general",
    "get", "gibt", "girl", "give", "glass", "go", "gold", "gone", "good", "got", "great", "green", "ground", "group",
    "grow", "guess", "gun", "hab", "habe", "haben", "had", "hair", "half", "hand", "happen", "happy", "hard", "has",
    "hat", "hatte", "have", "having", "he", "head", "hear", "heart", "heat", "heavy", "held", "hello", "help", "her",
    "here", "hers", "hier", "high", "hill", "him", "his", "hmm", "hold", "hole", "home", "hope", "horse", "hot",
    "hour", "house", "how", "huge", "hundred", "hurt", "i", "ich", "idea", "if", "ihr", "ihre", "im", "important",
    "in", "inch", "include", "input", "inside", "instead", "into", "iron", "is", "island", "ist", "it", "item",
    "its", "ja", "jetzt", "job", "join", "joy", "jump", "just", "kann", "keep", "kein", "keine", "kept", "key",
    "kill", "kind", "king", "knew", "know", "lady", "land", "language", "large", "last", "late", "later", "laugh",
    "law", "lay", "lead", "learn", "least", "leave", "left", "leg", "length", "less", "let", "letter", "level",
    "lie", "life", "lift", "light", "like", "line", "list", "listen", "little", "live", "log", "long", "look",
    "lose", "lost", "lot", "loud", "love", "low", "machine", "made", "main", "make", "man", "many", "map", "mark",
    "matter", "may", "maybe", "me", "mean", "measure", "meat", "meet", "mein", "meine", "member", "men", "message",
    "mich", "middle", "might", "mile", "milk", "million", "mind", "minute", "mir", "miss", "mit", "moment", "money",
    "month", "moon", "more", "morning", "most", "mother", "mountain", "mouth", "move", "much", "music", "muss",
    "must", "my", "nach", "name", "near", "necessary", "neck", "need", "nein", "never", "new", "news", "next",
    "nice", "nicht", "night", "nine", "no", "noch", "noise", "nor", "normal", "north", "nose", "not", "note",
    "nothing", "notice", "now", "number", "nun", "nur", "ob", "object", "ocean", "oder", "of", "off", "offer",
    "office", "often", "oh", "ohne", "oil", "ok", "okay", "old", "on", "once", "only", "open", "or", "order",
    "other", "our", "ours", "out", "output", "outside", "over", "own", "page", "paint", "pair", "paper", "part",
    "party", "pass", "past", "path", "pay", "people", "perhaps", "person", "phone", "pick", "picture", "piece",
    "place", "plan", "plant", "play", "please", "point", "poor", "position", "possible", "pound", "power", "pretty",
    "price", "problem", "produce", "public", "pull", "push", "put", "question", "quick", "quiet", "quite", "race",
    "rain", "raise", "ran", "reach", "read", "ready", "real", "really", "reason", "receive", "record", "red",
    "remember", "rest", "result", "return", "rich", "ride", "right", "ring", "rise", "river", "road", "rock", "room",
    "round", "rule", "run", "safe", "said", "sail", "same", "sand", "save", "saw", "say", "school", "science", "sea",
    "season", "seat", "second", "see", "seem", "sehr", "sein", "seine", "sell", "send", "sense", "sent", "serve",
    "set", "seven", "several", "shall", "shape", "share", "sharp", "she", "ship", "shoe", "shop", "short", "shot",
    "should", "shoulder", "show", "shut", "sich", "side", "sie", "sign", "silver", "simple", "since", "sind", "sing",
    "sir", "sister", "sit", "six", "size", "skin", "sky", "sleep", "slow", "small", "smell", "smile", "snow", "so",
    "soft", "soil", "soldier", "soll", "solve", "some", "son", "song", "soon", "sorry", "sort", "sound", "south",
    "space", "speak", "special", "speed", "spell", "spend", "spot", "spread", "spring", "square", "stand", "star",
    "start", "state", "station", "stay", "steel", "step", "stick", "still", "stone", "stop", "store", "story",
    "straight", "strange", "street", "stretch", "string", "strong", "student", "study", "stuff", "subject",
    "success", "such", "sudden", "sugar", "suggest", "summer", "sun", "supply", "suppose", "sure", "surface",
    "surprise", "swim", "symbol", "system", "table", "tail", "take", "talk", "tall", "teach", "team", "tell", "ten",
    "term", "test", "text", "than", "thank", "thanks", "that", "the", "their", "theirs", "them", "then", "there",
    "these", "they", "thick", "thin", "thing", "think", "third", "this", "those", "thought", "thousand", "three",
    "through", "throw", "thus", "tie", "time", "tiny", "tire", "to", "today", "together", "told", "tone", "too",
    "took", "tool", "top", "total", "touch", "toward", "town", "track", "trade", "train", "travel", "tree", "trip",
    "trouble", "true", "try", "turn", "twenty", "two", "type", "uh", "um", "uncle", "und", "under", "understand",
    "unit", "unless", "uns", "unser", "unsere", "until", "up", "upon", "us", "use", "user", "usual", "value", "vary",
    "verb", "very", "video", "view", "village", "visit", "voice", "von", "vor", "vowel", "wait", "walk", "wall",
    "want", "war", "waren", "warm", "was", "wash", "watch", "water", "wave", "way", "we", "wear", "weather", "week",
    "weight", "weil", "welcome", "well", "wenn", "wer", "werden", "were", "west", "what", "wheel", "when", "where",
    "which", "while", "white", "who", "whole", "whom", "whose", "why", "wide", "wie", "wife", "wild", "will", "win",
    "wind", "window", "winter", "wir", "wird", "wire", "wish", "with", "wo", "wollen", "woman", "wonder", "wood",
    "word", "work", "world", "worry", "would", "write", "wrong", "yard", "yeah", "year", "yes", "you", "young",
    "your", "yours", "yourself", "zero", "zu", "zum", "zur",
];

/// Acronyms a letter run may name, whatever the workspace holds. Sorted.
pub static KNOWN_ACRONYMS: [&str; 88] = [
    "abi", "ai", "api", "ascii", "aws", "bmp", "cd", "cdn", "ci", "cli", "cors", "cpu", "csrf", "css", "csv", "db",
    "dns", "dom", "dpi", "eof", "ffi", "fps", "fs", "ftp", "gb", "gif", "gpu", "gui", "hsl", "html", "http", "https",
    "hz", "id", "ide", "io", "ip", "jpeg", "jpg", "json", "jvm", "jwt", "kb", "llm", "mb", "md", "mime", "ml", "ms",
    "ocr", "os", "pdf", "png", "ppi", "px", "ram", "regex", "rgb", "rgba", "rpc", "sdk", "sha", "sql", "ssh", "ssl",
    "svg", "tb", "tcp", "tiff", "tls", "toml", "tsv", "ttl", "tui", "udp", "ui", "uri", "url", "usb", "utf", "uuid",
    "ux", "vm", "wasm", "webp", "xml", "xss", "yaml",
];

/// What recognisers write for the words of the code; each is the code word it may be. Heard words
/// first, in lower case; the ones the speech agent measured at the top.
pub static HOMOPHONES: [(&[&str], &[&str]); 47] = [
    (&["enemy"], &["enum"]),
    (&["awake"], &["await"]),
    (&["hdp"], &["http"]),
    (&["age", "ttp"], &["http"]),
    (&["jason"], &["json"]),
    (&["htdp"], &["http"]),
    (&["htp"], &["http"]),
    (&["h", "ttp"], &["http"]),
    (&["in", "um"], &["enum"]),
    (&["e", "num"], &["enum"]),
    (&["a", "num"], &["enum"]),
    (&["e", "numb"], &["enum"]),
    (&["inum"], &["enum"]),
    (&["a", "wait"], &["await"]),
    (&["a", "sink"], &["async"]),
    (&["a", "sync"], &["async"]),
    (&["asink"], &["async"]),
    (&["asynch"], &["async"]),
    (&["mew", "tex"], &["mutex"]),
    (&["mute", "ex"], &["mutex"]),
    (&["mutt", "ex"], &["mutex"]),
    (&["mew", "techs"], &["mutex"]),
    (&["veck"], &["vec"]),
    (&["vek"], &["vec"]),
    (&["imple"], &["impl"]),
    (&["in", "pull"], &["impl"]),
    (&["strukt"], &["struct"]),
    (&["un", "wrap"], &["unwrap"]),
    (&["on", "wrap"], &["unwrap"]),
    (&["un", "rap"], &["unwrap"]),
    (&["unrap"], &["unwrap"]),
    (&["jay", "son"], &["json"]),
    (&["sequel"], &["sql"]),
    (&["gooey"], &["gui"]),
    (&["klee"], &["cli"]),
    (&["yammel"], &["yaml"]),
    (&["yam", "l"], &["yaml"]),
    (&["tom", "l"], &["toml"]),
    (&["reg", "ex"], &["regex"]),
    (&["wazum"], &["wasm"]),
    (&["was", "em"], &["wasm"]),
    (&["askee"], &["ascii"]),
    (&["as", "key"], &["ascii"]),
    (&["engine", "x"], &["nginx"]),
    (&["standard", "out"], &["stdout"]),
    (&["standard", "in"], &["stdin"]),
    (&["oh", "auth"], &["oauth"]),
];

/// What a kind word may mean: the kinds of definition, the likeliest first.
static KIND_WORDS: [(&str, &[Kind]); 44] = [
    ("alias", &[Kind::Type]),
    ("class", &[Kind::Class, Kind::Struct]),
    ("classes", &[Kind::Class, Kind::Struct]),
    ("const", &[Kind::Const, Kind::Static]),
    ("constant", &[Kind::Const, Kind::Static]),
    ("crate", &[Kind::Package, Kind::Dependency]),
    ("dependency", &[Kind::Dependency, Kind::Package]),
    ("enemy", &[Kind::Enum]),
    ("enum", &[Kind::Enum]),
    ("enums", &[Kind::Enum]),
    ("file", &[Kind::File]),
    ("fn", &[Kind::Fn, Kind::Method]),
    ("func", &[Kind::Fn, Kind::Method]),
    ("function", &[Kind::Fn, Kind::Method]),
    ("functions", &[Kind::Fn, Kind::Method]),
    ("interface", &[Kind::Interface, Kind::Trait]),
    ("library", &[Kind::Package, Kind::Dependency]),
    ("macro", &[Kind::Macro]),
    ("method", &[Kind::Method, Kind::Fn]),
    ("methods", &[Kind::Method, Kind::Fn]),
    ("mod", &[Kind::Module]),
    ("module", &[Kind::Module]),
    ("modules", &[Kind::Module]),
    ("namespace", &[Kind::Module]),
    ("package", &[Kind::Package, Kind::Dependency]),
    ("protocol", &[Kind::Interface, Kind::Trait]),
    ("static", &[Kind::Static, Kind::Const]),
    ("struct", &[Kind::Struct]),
    ("structs", &[Kind::Struct]),
    ("structure", &[Kind::Struct]),
    ("strukt", &[Kind::Struct]),
    ("trait", &[Kind::Trait, Kind::Interface]),
    ("traits", &[Kind::Trait, Kind::Interface]),
    ("type", &[Kind::Type, Kind::Struct, Kind::Enum, Kind::Class, Kind::Trait, Kind::Interface, Kind::Union]),
    ("union", &[Kind::Union]),
    ("datei", &[Kind::File]),
    ("funktion", &[Kind::Fn, Kind::Method]),
    ("klasse", &[Kind::Class, Kind::Struct]),
    ("konstante", &[Kind::Const, Kind::Static]),
    ("makro", &[Kind::Macro]),
    ("methode", &[Kind::Method, Kind::Fn]),
    ("modul", &[Kind::Module]),
    ("struktur", &[Kind::Struct]),
    ("paket", &[Kind::Package, Kind::Dependency]),
];

/// The kinds a spoken kind word may mean (`struct`, `function`, `crate`, ... and the usual
/// mishearing of `enum`); empty for any other word. `word` is lower case.
#[must_use]
pub fn kind_words(word: &str) -> &'static [Kind] {
    KIND_WORDS.iter().find(|(w, _)| *w == word).map_or(&[], |(_, kinds)| kinds)
}

/// Whether `word` (lower case) only joins other words.
#[must_use]
pub fn is_stop_word(word: &str) -> bool {
    STOP_WORDS.binary_search(&word).is_ok()
}

/// Whether `word` (lower case) is an everyday word, or an everyday word with an ending (`errors`,
/// `wanted`, `looking`).
#[must_use]
pub fn is_common_word(word: &str) -> bool {
    if COMMON_WORDS.binary_search(&word).is_ok() {
        return true;
    }
    ["s", "es", "ed", "d", "ing"]
        .iter()
        .filter_map(|ending| word.strip_suffix(ending))
        .any(|stem| stem.chars().count() >= 3 && COMMON_WORDS.binary_search(&stem).is_ok())
}

/// The letters a spoken word may be (`see` is `c`, `esses` is `ss`); empty for any other word.
#[must_use]
pub fn letter_options(word: &str) -> Vec<&'static str> {
    LETTER_NAMES
        .binary_search_by(|(w, _)| (*w).cmp(word))
        .map(|at| vec![LETTER_NAMES[at].1])
        .unwrap_or_default()
}

/// What number words say, in the way a person says a number: `thirty two` is 32, `two fifty six` is
/// 256, `one hundred twenty` is 120. The digits and how many words they took, or `None` when the
/// first word is no number.
#[must_use]
pub fn number_string(words: &[&str]) -> Option<(String, usize)> {
    fn value(word: &str) -> Option<u32> {
        Some(match word {
            "zero" => 0,
            "one" => 1,
            "two" => 2,
            "three" => 3,
            "four" => 4,
            "five" => 5,
            "six" => 6,
            "seven" => 7,
            "eight" => 8,
            "nine" => 9,
            "ten" => 10,
            "eleven" => 11,
            "twelve" => 12,
            "thirteen" => 13,
            "fourteen" => 14,
            "fifteen" => 15,
            "sixteen" => 16,
            "seventeen" => 17,
            "eighteen" => 18,
            "nineteen" => 19,
            "twenty" => 20,
            "thirty" => 30,
            "forty" => 40,
            "fifty" => 50,
            "sixty" => 60,
            "seventy" => 70,
            "eighty" => 80,
            "ninety" => 90,
            _ => return None,
        })
    }
    /// A tens word, a teen or a unit at `at`, with the unit that may follow a tens word: the value and
    /// the words taken.
    fn group(words: &[&str], at: usize) -> Option<(u32, usize)> {
        let v = value(words.get(at)?)?;
        if v >= 20 {
            return Some(match words.get(at + 1).and_then(|w| value(w)) {
                Some(u) if (1..=9).contains(&u) => (v + u, 2),
                _ => (v, 1),
            });
        }
        Some((v, 1))
    }
    let mut at = 0;
    let mut out = String::new();
    while at < words.len() && out.len() < 6 {
        let Some((v, used)) = group(words, at) else {
            break;
        };
        if (1..10).contains(&v) && words.get(at + used) == Some(&"hundred") {
            // `one hundred twenty`
            let mut total = v * 100;
            let mut taken = used + 1;
            if let Some((rest, more)) = group(words, at + taken) {
                total += rest;
                taken += more;
            }
            out.push_str(&total.to_string());
            at += taken;
            continue;
        }
        out.push_str(&v.to_string());
        at += used;
    }
    (!out.is_empty()).then_some((out, at))
}

/// Whether letters `s` may be an acronym: a listed one (two letters or more), or three letters or
/// more that are a word of the workspace.
fn valid_acronym(s: &str, known: &dyn Fn(&str) -> bool) -> bool {
    s.len() >= 2 && (KNOWN_ACRONYMS.binary_search(&s).is_ok() || (s.len() >= 3 && known(s)))
}

/// The readings of a run of letter names at the start of `rest`: how many words each takes, the
/// tokens, the `via` bits.
fn letter_runs(rest: &[SpokenWord], known: &dyn Fn(&str) -> bool) -> Vec<(usize, Vec<String>, u8)> {
    let mut units: Vec<(&'static str, usize)> = Vec::new();
    let mut p = 0;
    while p < rest.len() && units.len() < MAX_RUN {
        let word = rest[p].clean.as_str();
        if word == "double" && rest.get(p + 1).is_some_and(|w| w.clean == "you" || w.clean == "u") {
            units.push(("w", p + 2));
            p += 2;
            continue;
        }
        match letter_options(word).first() {
            Some(letters) => {
                units.push((letters, p + 1));
                p += 1;
            }
            None => break,
        }
    }
    let mut out = Vec::new();
    let mut letters = String::new();
    for (n, (part, end)) in units.iter().enumerate() {
        letters.push_str(part);
        if n >= 1 && valid_acronym(&letters, known) {
            out.push((*end, vec![letters.clone()], VIA_LETTERS));
        }
        let ahead: Vec<&str> = rest[*end..].iter().take(MAX_RUN).map(|w| w.clean.as_str()).collect();
        if let Some((digits, used)) = number_string(&ahead) {
            if KNOWN_ACRONYMS.binary_search(&letters.as_str()).is_ok() || known(&letters) {
                out.push((end + used, vec![letters.clone(), digits], VIA_LETTERS | VIA_DIGITS));
            }
        }
    }
    out
}

/// The readings a window has for a SEARCH for similar names, not for an exact match: a plural letter
/// name (`sees`, `esses`) is also the letter and an `s` (`cs`, `ss`: a recogniser that heard "C S S" may write
/// "sees"), so `sees error` is `cs error`, a word away from `css error`. Each says it used letters.
#[must_use]
pub fn plural_letter_readings(window: &[SpokenWord]) -> Vec<Reading> {
    let mut out = Vec::new();
    for (at, word) in window.iter().enumerate() {
        let letters = letter_options(&word.clean);
        let Some(first) = letters.first().and_then(|l| (l.len() == 2).then_some(l)) else {
            continue;
        };
        if !word.clean.ends_with('s') {
            continue;
        }
        let mut tokens = Vec::new();
        for (i, w) in window.iter().enumerate() {
            if i == at {
                tokens.push(format!("{}s", &first[..1]));
            } else {
                tokens.extend(literal_tokens(w).0);
            }
        }
        push_reading(&mut out, tokens, VIA_LETTERS);
    }
    out
}

/// The longest run of spelled-out letters that starts at `pos` and is an acronym (a listed one, or a
/// word of the workspace of three letters or more): the position after it and the acronym.
#[must_use]
pub fn acronym_run_at(window: &[SpokenWord], pos: usize, known: &dyn Fn(&str) -> bool) -> Option<(usize, String)> {
    let rest = window.get(pos..)?;
    letter_runs(rest, known)
        .into_iter()
        .filter(|(_, tokens, via)| tokens.len() == 1 && *via == VIA_LETTERS)
        .max_by_key(|(taken, _, _)| *taken)
        .map(|(taken, mut tokens, _)| (pos + taken, tokens.remove(0)))
}

/// The words of one heard word as written: split as an identifier (`CssError` is `css error`).
fn literal_tokens(word: &SpokenWord) -> (Vec<String>, u8) {
    let spelled_part = word.text.chars().count() != trim_edges(&word.text).chars().count() && word.clean.chars().count() == 1;
    let source = if spelled_part || word.clean.chars().count() == 1 { word.clean.as_str() } else { trim_edges(&word.text) };
    let tokens = split_identifier(source);
    let via = if word.identifier_like { VIA_IDENTIFIER } else { 0 };
    (tokens, via)
}

fn push_reading(out: &mut Vec<Reading>, tokens: Vec<String>, via: u8) {
    if tokens.is_empty() || out.len() >= MAX_READINGS {
        return;
    }
    if let Some(existing) = out.iter_mut().find(|r| r.tokens == tokens) {
        existing.via |= via;
    } else {
        out.push(Reading { tokens, via });
    }
}

fn walk(window: &[SpokenWord], pos: usize, cur: &mut Vec<String>, via: u8, out: &mut Vec<Reading>, known: &dyn Fn(&str) -> bool, budget: &mut usize) {
    if *budget == 0 || out.len() >= MAX_READINGS {
        return;
    }
    *budget -= 1;
    if pos == window.len() {
        if via != 0 {
            push_reading(out, cur.clone(), via);
        }
        return;
    }
    let rest = &window[pos..];
    let before = cur.len();
    for (taken, tokens, bits) in letter_runs(rest, known) {
        cur.extend(tokens);
        walk(window, pos + taken, cur, via | bits, out, known, budget);
        cur.truncate(before);
    }
    for (heard, target) in &HOMOPHONES {
        if heard.len() <= rest.len() && heard.iter().zip(rest).all(|(h, w)| *h == w.clean) {
            cur.extend(target.iter().map(|t| (*t).to_string()));
            walk(window, pos + heard.len(), cur, via | VIA_HOMOPHONE, out, known, budget);
            cur.truncate(before);
        }
    }
    let ahead: Vec<&str> = rest.iter().take(MAX_RUN).map(|w| w.clean.as_str()).collect();
    if let Some((digits, used)) = number_string(&ahead) {
        cur.push(digits);
        walk(window, pos + used, cur, via | VIA_DIGITS, out, known, budget);
        cur.truncate(before);
    }
    let (tokens, bits) = literal_tokens(&rest[0]);
    cur.extend(tokens);
    walk(window, pos + 1, cur, via | bits, out, known, budget);
    cur.truncate(before);
}

/// What the words of `window` may say: the words as heard (an identifier split into its words), and
/// every way a run of them may be a spelled-out acronym (`known` says whether a word is one of the
/// workspace's), a number, or a mishearing of a code word. At most [`MAX_READINGS`], the words as
/// heard first.
#[must_use]
pub fn readings(window: &[SpokenWord], known: &dyn Fn(&str) -> bool) -> Vec<Reading> {
    if window.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut literal = Vec::new();
    let mut via = 0u8;
    for word in window {
        let (tokens, bits) = literal_tokens(word);
        literal.extend(tokens);
        via |= bits;
    }
    push_reading(&mut out, literal, via);
    let mut budget = BUDGET;
    walk(window, 0, &mut Vec::new(), 0, &mut out, known, &mut budget);
    out
}

#[cfg(test)]
#[path = "symbols_spoken_test.rs"]
mod symbols_spoken_test;
