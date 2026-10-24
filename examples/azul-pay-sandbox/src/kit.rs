//! The page kit every sandbox page is built from: the classic split screen (the order on the
//! left, a few inputs on the right), and what makes a page unmistakably a test: "[TEST]" in its
//! title, the fixed hazard ribbon, a diagonal watermark across the form and a hazard-striped
//! frame in a palette no provider uses. Plain CSS, no images, no fonts, nothing from elsewhere.

/// The ribbon at the top of every page.
pub const RIBBON: &str = "TEST - local fake, not a real payment provider. Nothing is charged; only documented test data works.";
/// The watermark across every form.
pub const WATERMARK: &str = "TEST - NOT A REAL PAYMENT PAGE";

/// The headers of every answer: only this computer may frame a page (the app's popover shows it
/// as a top-level page anyway), no referrer leaves, nothing is sniffed or cached.
pub const CSP: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; frame-ancestors 'self' http://127.0.0.1:* http://localhost:* http://[::1]:*";

pub fn security_headers() -> Vec<(String, String)> {
    vec![
        ("Content-Security-Policy".into(), CSP.into()),
        ("X-Frame-Options".into(), "SAMEORIGIN".into()),
        ("Referrer-Policy".into(), "no-referrer".into()),
        ("X-Content-Type-Options".into(), "nosniff".into()),
        ("Cache-Control".into(), "no-store".into()),
    ]
}

const CSS: &str = "\
*{box-sizing:border-box}\
body{margin:0;font:15px/1.45 system-ui,sans-serif;color:#151515;background:#fff}\
.ribbon{position:fixed;top:0;left:0;right:0;z-index:5;padding:6px;text-align:center;font-weight:700;font-size:13px;\
background:repeating-linear-gradient(45deg,#ffd400 0 14px,#151515 14px 28px)}\
.ribbon span{background:#ffd400;padding:2px 8px}\
.hazard{min-height:100vh;padding-top:34px;border:8px solid #ffd400;\
border-image:repeating-linear-gradient(45deg,#ffd400 0 12px,#151515 12px 24px) 8}\
.split{display:flex;flex-wrap:wrap;min-height:calc(100vh - 50px)}\
.left{flex:1 1 300px;padding:36px;background:#f3f3ee}\
.right,.single{flex:1 1 340px;padding:36px;position:relative;overflow:hidden}\
.single{max-width:460px;margin:0 auto}\
.watermark{position:absolute;top:45%;left:-10%;right:-10%;text-align:center;transform:rotate(-24deg);\
font-size:26px;font-weight:800;color:rgba(190,0,0,.14);pointer-events:none;white-space:nowrap}\
.shape{font-size:12px;text-transform:uppercase;letter-spacing:.06em;color:#6a6a60}\
.amount{font-size:30px;font-weight:700;margin:8px 0}\
label{display:block;margin:12px 0 4px;font-size:13px;color:#4a4a44}\
input,select{width:100%;padding:10px;font:inherit;border:1px solid #b9b9b0;border-radius:6px;background:#fff}\
.row{display:flex;gap:10px}.row>div{flex:1}\
button{width:100%;margin-top:16px;padding:11px;font:inherit;font-weight:700;background:#ffd400;color:#151515;\
border:2px solid #151515;border-radius:6px;cursor:pointer}\
button.plain{background:#fff}\
button[disabled]{opacity:.5}\
.error{color:#b00020;min-height:1.4em;margin-top:10px}\
.muted{color:#6a6a60;font-size:13px}\
.qr{border:4px dashed #151515;padding:16px;margin:12px 0;text-align:center;word-break:break-all;background:#fff}\
a{color:#151515}";

/// HTML-escaped text.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// `EUR 59.99` (an amount in cents).
pub fn money(cents: i64, currency: &str) -> String {
    format!("{} {}.{:02}", currency.to_ascii_uppercase(), cents / 100, (cents % 100).abs())
}

fn head(title: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta name=\"referrer\" content=\"no-referrer\"><title>[TEST] {}</title><style>{CSS}</style></head>",
        esc(title)
    )
}

fn ribbon() -> String {
    format!("<div class=\"ribbon\"><span>{}</span></div>", esc(RIBBON))
}

fn watermark() -> String {
    format!("<div class=\"watermark\" aria-hidden=\"true\">{WATERMARK}</div>")
}

/// A split-screen page: `left` the order (HTML), `right` the form (HTML), `shape` what kind of
/// test provider it is ("Stripe-shaped test provider"), `script` its own few lines.
pub fn split(title: &str, shape: &str, left: &str, right: &str, script: &str) -> String {
    format!(
        "{}<body><div class=\"hazard\">{}<div class=\"split\"><div class=\"left\"><div class=\"shape\">{}</div>{left}</div>\
         <div class=\"right\">{}{right}</div></div></div><script>{script}</script></body></html>",
        head(title),
        ribbon(),
        esc(shape),
        watermark()
    )
}

/// A one-column page (the 3-D Secure step, a bank's confirmation, the index).
pub fn single(title: &str, shape: &str, body: &str, script: &str) -> String {
    format!(
        "{}<body><div class=\"hazard\">{}<div class=\"single\">{}<div class=\"shape\">{}</div>{body}</div></div>\
         <script>{script}</script></body></html>",
        head(title),
        ribbon(),
        watermark(),
        esc(shape)
    )
}

/// The left half: what is being bought.
pub fn order(product: &str, amount: &str, note: &str) -> String {
    format!(
        "<p class=\"muted\">Pay a test merchant</p><div>{}</div><div class=\"amount\">{}</div><p class=\"muted\">{}</p>",
        esc(product),
        esc(amount),
        esc(note)
    )
}

/// The card inputs' few lines of script: spacing the number, the expiry's slash, digits only in
/// the CVC, and a check before the form is sent (the server checks again).
pub const CARD_SCRIPT: &str = "\
var f=document.querySelector('form.card');if(f){\
var n=f.card_number,e=f.exp,c=f.cvc,err=f.querySelector('.error'),b=f.querySelector('button');\
function d(s){return s.replace(/\\D/g,'')}\
function luhn(s){var t=0;for(var i=0;i<s.length;i++){var x=+s[s.length-1-i];if(i%2){x*=2;if(x>9)x-=9}t+=x}return s.length>11&&t%10==0}\
n.oninput=function(){n.value=d(n.value).slice(0,19).replace(/(.{4})/g,'$1 ').trim()};\
e.oninput=function(){var v=d(e.value).slice(0,4);e.value=v.length>2?v.slice(0,2)+' / '+v.slice(2):v};\
c.oninput=function(){c.value=d(c.value).slice(0,4)};\
f.onsubmit=function(ev){var m='',x=d(e.value),now=new Date(),y=2000+ +x.slice(2),mo=+x.slice(0,2);\
if(!luhn(d(n.value)))m='Your card number is invalid.';\
else if(x.length<4)m='Your card\\'s expiration date is incomplete.';\
else if(mo<1||mo>12)m='Your card\\'s expiration date is invalid.';\
else if(y<now.getFullYear()||(y==now.getFullYear()&&mo<now.getMonth()+1))m='Your card\\'s expiration date is in the past.';\
else if(d(c.value).length<3)m='Your card\\'s security code is incomplete.';\
if(m){ev.preventDefault();err.textContent=m;return}b.disabled=true;b.textContent='Processing...'}}";

/// The IBAN input's script: groups of four, the checksum before the form is sent.
pub const IBAN_SCRIPT: &str = "\
var g=document.querySelector('form.iban');if(g){\
var i=g.iban,ge=g.querySelector('.error'),gb=g.querySelector('button');\
function cmp(s){return s.replace(/[^0-9a-zA-Z]/g,'').toUpperCase()}\
function ok(s){if(s.length<15)return false;var r=0,t=s.slice(4)+s.slice(0,4);for(var k=0;k<t.length;k++){\
var v=parseInt(t[k],36);if(isNaN(v))return false;r=(v>9?r*100+v:r*10+v)%97}return r==1}\
i.oninput=function(){i.value=cmp(i.value).slice(0,34).replace(/(.{4})/g,'$1 ').trim()};\
g.onsubmit=function(ev){var s=cmp(i.value);if(!ok(s)){ev.preventDefault();ge.textContent=s.length<15?'Your IBAN is incomplete.':'Your IBAN is invalid.';return}\
gb.disabled=true;gb.textContent='Processing...'}}";

/// The index page: what this is, and where each provider's API and the switches are.
pub fn index(providers: &[(&str, &str)]) -> String {
    let mut list = String::new();
    for (id, prefix) in providers {
        list.push_str(&format!("<li><b>{}</b>: API at <code>{}</code></li>", esc(id), esc(prefix)));
    }
    let body = format!(
        "<h1>Payment sandbox</h1><p>A local test tool: payment-provider-shaped test APIs, pages and signed webhooks \
         for testing an app's purchase flow without a provider account. It does not reproduce any provider's \
         real UI or branding, and it refuses real payment data.</p><ul>{list}</ul>\
         <p class=\"muted\">Switches: POST /_dev/switch {{\"name\": \"decline_next\" | \"three_ds_next\" | \
         \"return_next_debit\" | \"chargeback_next\" | \"drop_next_webhook\" | \"duplicate_next_webhook\"}}, \
         POST /_dev/delay {{\"secs\"}}, POST /_dev/advance {{\"months\"}}, GET /_dev/state.</p>"
    );
    single("Payment sandbox", "Local test tool", &body, "")
}
