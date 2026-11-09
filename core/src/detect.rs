// SPDX-License-Identifier: GPL-3.0-or-later

//! What a piece of text holds that an app can act on: a web link to offer to open, a one-time
//! code (2FA) to put on a computer's clipboard, the shelf of the history it goes on
//! ([`category`]), and whether it must never be kept ([`looks_secret`]).
//!
//! The text is untrusted (a clip from another device, a notification from any app): these
//! functions are bounded, never panic, and are fuzzed (`fuzz/fuzz_targets/detect.rs`). They are
//! plain rules, so the same text always gets the same answer, in every language.

/// Longest link offered to open. Browsers take more, but a copied link is rarely longer.
pub const MAX_LINK_LEN: usize = 2048;

/// A text that is nothing but a web link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The link as copied, without the spaces around it.
    pub url: String,
    /// The site it leads to, lowercase, to show the user before they open it: "example.com".
    pub host: String,
}

/// The text as a link to open, if it is a single `http` or `https` link and nothing else.
///
/// Strict on purpose: the user is shown [`Link::host`] and decides from it, so anything that
/// could make the host misleading is refused rather than shown.
pub fn link(text: &str) -> Option<Link> {
    let url = text.trim();
    if url.is_empty() || url.len() > MAX_LINK_LEN {
        return None;
    }
    // Plain ASCII without spaces, quotes or brackets, which is how browsers copy links (they
    // percent-encode the rest). Rules out text around the link, look-alike letters in the host,
    // and characters a command line could read differently.
    if !url
        .bytes()
        .all(|b| b.is_ascii_graphic() && !b"\"<>\\^`{|}".contains(&b))
    {
        return None;
    }
    let rest = strip_prefix_ignore_case(url, "https://")
        .or_else(|| strip_prefix_ignore_case(url, "http://"))?;
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    // "https://bank.example@evil.example" leads to evil.example: refused.
    if authority.contains('@') {
        return None;
    }
    let host = host_of(authority)?;
    Some(Link {
        url: url.to_owned(),
        host,
    })
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// The host of `host[:port]`, lowercase, if both are well formed.
fn host_of(authority: &str) -> Option<String> {
    let (host, port) = match authority.strip_prefix('[') {
        // An IPv6 address: "[2001:db8::1]:8080".
        Some(rest) => {
            let (inside, _) = rest.split_once(']')?;
            let valid = !inside.is_empty()
                && inside
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.');
            if !valid {
                return None;
            }
            authority.split_at(inside.len() + 2)
        }
        None => {
            let (name, port) = authority.split_at(authority.find(':').unwrap_or(authority.len()));
            if !is_host_name(name) {
                return None;
            }
            (name, port)
        }
    };
    if !port.is_empty() {
        let digits = port.strip_prefix(':')?;
        let valid = (1..=5).contains(&digits.len())
            && digits.bytes().all(|b| b.is_ascii_digit())
            && digits.parse::<u32>().is_ok_and(|n| n <= 65_535);
        if !valid {
            return None;
        }
    }
    // "example.com." is shown as "example.com".
    Some(host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase())
}

/// "example.com", "localhost", "192.168.1.20": labels of letters, digits and inner hyphens.
fn is_host_name(name: &str) -> bool {
    // A final dot is allowed: "example.com." is the same site.
    let name = name.strip_suffix('.').unwrap_or(name);
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Only the start of a notification is searched for a code.
pub const MAX_CODE_TEXT_CHARS: usize = 4096;

/// How much farther a word counts when a sentence ends between it and a number.
const SENTENCE_GAP: usize = 100;

/// Shortest and longest one-time codes, in digits.
pub const CODE_DIGITS: std::ops::RangeInclusive<usize> = 4..=8;

/// Words that come with a one-time code, lowercase. English, French, Spanish, Portuguese,
/// Italian, German. Without one of them nearby, a number is just a number (an order, an amount).
const CODE_WORDS: &[&str] = &[
    "code",
    "otp",
    "pin",
    "2fa",
    "passcode",
    "password",
    "verification",
    "verify",
    "authentication",
    "one-time",
    "one time",
    "security",
    "login",
    "log in",
    "sign in",
    "sign-in",
    "mot de passe",
    "vérification",
    "authentification",
    "usage unique",
    "connexion",
    "confirmation",
    "código",
    "codigo",
    "contraseña",
    "verificación",
    "senha",
    "codice",
    "verifica",
    "bestätigung",
    "passwort",
    "kennwort",
    "anmeld",
];

/// The one-time code in a notification's text, digits only: "G-482913 is your Google
/// verification code" gives "482913".
///
/// A code is 4 to 8 digits, possibly in two groups ("482 913", "4829-1374"), standing on its own
/// (not part of a word, an amount, a date, a time or a phone number), in a text that has a word
/// such as "code" or "mot de passe". If several numbers qualify, the one closest to such a word
/// wins; a tie gives nothing rather than a guess.
pub fn one_time_code(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().take(MAX_CODE_TEXT_CHARS).collect();
    // Lowercase char by char, so positions stay the same.
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let words = code_words(&lower);
    if words.is_empty() {
        return None;
    }

    // How many sentence ends come before each position, to tell in O(1) whether a word is in
    // another sentence than a number.
    let mut sentence_ends = Vec::with_capacity(chars.len() + 1);
    sentence_ends.push(0);
    let mut ends = 0;
    for c in &chars {
        ends += usize::from(matches!(c, '.' | '!' | '?' | '\n'));
        sentence_ends.push(ends);
    }

    let mut best: Option<(usize, String)> = None;
    let mut tie = false;
    for (start, end, code) in candidates(&chars) {
        let distance = words
            .iter()
            .map(|&(word_start, word_end)| {
                let gap = if word_end <= start {
                    word_end..start
                } else if end <= word_start {
                    end..word_start
                } else {
                    return 0;
                };
                // A word in another sentence counts for less: "card 1234. Code: 584921".
                let other_sentence = sentence_ends[gap.end] > sentence_ends[gap.start];
                gap.len() + if other_sentence { SENTENCE_GAP } else { 0 }
            })
            .min()
            .unwrap_or(usize::MAX);
        match &best {
            Some((closest, found)) if distance == *closest && *found != code => tie = true,
            Some((closest, _)) if distance >= *closest => {}
            _ => {
                best = Some((distance, code));
                tie = false;
            }
        }
    }
    if tie {
        None
    } else {
        best.map(|(_, code)| code)
    }
}

/// Where the words of [`CODE_WORDS`] are, as `(start, end)` char positions. A word must start
/// a word of the text ("pin", not "shipping"); short ones must end one too, or be plural.
fn code_words(lower: &[char]) -> Vec<(usize, usize)> {
    let words: Vec<Vec<char>> = CODE_WORDS
        .iter()
        .map(|word| word.chars().collect())
        .collect();
    let mut found = Vec::new();
    for start in 0..lower.len() {
        if start > 0 && lower[start - 1].is_alphanumeric() {
            continue;
        }
        for word in &words {
            let end = start + word.len();
            if lower.get(start..end) != Some(&word[..]) {
                continue;
            }
            let ends_well = word.len() > 4
                || match lower.get(end) {
                    None => true,
                    Some('s') => lower.get(end + 1).is_none_or(|c| !c.is_alphanumeric()),
                    Some(c) => !c.is_alphanumeric(),
                };
            if ends_well {
                found.push((start, end));
            }
        }
    }
    found
}

/// Numbers that could be a code: `(start, end, digits)`.
fn candidates(chars: &[char]) -> Vec<(usize, usize, String)> {
    let digits_at = |at: usize| {
        chars[at..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count()
    };
    let mut found = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let first = digits_at(at);
        let mut end = at + first;
        // "482 913" or "4829-1374": two groups of 3 or 4 digits, the same length.
        if (3..=4).contains(&first)
            && matches!(chars.get(end), Some(' ' | '-'))
            && end + 1 < chars.len()
            && digits_at(end + 1) == first
        {
            end += 1 + first;
        }
        let code: String = chars[at..end]
            .iter()
            .filter(|c| c.is_ascii_digit())
            .collect();
        if CODE_DIGITS.contains(&code.len()) && stands_alone(chars, at, end) {
            found.push((at, end, code));
        }
        at = end;
    }
    found
}

/// Not part of a word, an amount, a date, a time, a version or a phone number.
fn stands_alone(chars: &[char], start: usize, end: usize) -> bool {
    let before = start.checked_sub(1).map(|i| chars[i]);
    let after = chars.get(end).copied();
    let after_next = chars.get(end + 1).copied();
    let joined = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    if joined(before) || joined(after) {
        return false;
    }
    if matches!(before, Some('+' | '#' | '/' | ':' | '€' | '$' | '£' | '¥')) {
        return false;
    }
    if matches!(after, Some('/' | ':' | '%' | '€' | '$' | '£' | '¥')) {
        return false;
    }
    // "1.234,56", "12.05.2026", "v1.2": a separator between digits.
    let digit = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit());
    let before_previous = start.checked_sub(2).map(|i| chars[i]);
    if matches!(before, Some('.' | ',')) && digit(before_previous) {
        return false;
    }
    if matches!(after, Some('.' | ',')) && digit(after_next) {
        return false;
    }
    true
}

/// The shelf a clip goes on in the history.
///
/// [`category`] gives the text ones. `Screenshot` and `Image` come from the clip's type and
/// source, never from [`category`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Anything that is none of the others.
    Text,
    /// A web link and nothing else (see [`link`]).
    Link,
    /// An email address and nothing else.
    Email,
    /// A phone number and nothing else.
    Phone,
    /// Source code or a command line.
    Code,
    /// A JSON object or array.
    Json,
    /// A colour: `#ff8800`, `rgb(255, 136, 0)`, `hsl(32 100% 50%)`.
    Color,
    /// A screenshot from the phone's gallery.
    Screenshot,
    /// Any other image.
    Image,
}

/// Only the start of a long text is read to tell code from prose.
const MAX_CODE_SCAN_LEN: usize = 16 * 1024;
const MAX_CODE_LINES: usize = 200;

/// JSON nested deeper than this is called code, not JSON.
const MAX_JSON_DEPTH: usize = 64;

/// Longest text checked for JSON. Longer ones are called code.
pub const MAX_JSON_LEN: usize = 1024 * 1024;

/// The shelf of a text clip. One of `Text`, `Link`, `Email`, `Phone`, `Code`, `Json`, `Color`.
///
/// Every category but `Code` and `Text` means the whole clip is that one thing (spaces around
/// it aside): "call 06 12 34 56 78" is text, not a phone number.
pub fn category(text: &str) -> Category {
    let trimmed = text.trim();
    if link(trimmed).is_some() {
        Category::Link
    } else if is_email(trimmed) {
        Category::Email
    } else if is_phone(trimmed) {
        Category::Phone
    } else if is_color(trimmed) {
        Category::Color
    } else if is_json(trimmed) {
        Category::Json
    } else if is_code(head(text, MAX_CODE_SCAN_LEN)) {
        Category::Code
    } else {
        Category::Text
    }
}

/// `text` cut to at most `max` bytes, on a character boundary.
pub(crate) fn head(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// "jean.dupont@example.fr": an address and nothing else. ASCII only.
fn is_email(text: &str) -> bool {
    let Some((local, domain)) = text.rsplit_once('@') else {
        return false;
    };
    let tld = domain.rsplit('.').next().unwrap_or("");
    text.len() <= 254
        && (1..=64).contains(&local.len())
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+/=?^_`{|}~.-".contains(&b))
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && domain.contains('.')
        && is_host_name(domain)
        && tld.len() >= 2
        && tld.bytes().all(|b| b.is_ascii_alphabetic())
}

/// "+33 6 12 34 56 78", "06.12.34.56.78", "(555) 123-4567", "0612345678": a number and nothing
/// else. Short numbers, dates and card numbers are left out.
fn is_phone(text: &str) -> bool {
    if text.len() > 32 {
        return false;
    }
    let (plus, body) = match text.strip_prefix('+') {
        Some(body) => (true, body),
        None => (false, text),
    };
    let bytes = body.as_bytes();
    let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    if !(first.is_ascii_digit() || first == b'(') || !last.is_ascii_digit() {
        return false;
    }
    // Digits, single separators, and at most one "(…)" group of 1 to 5 digits: "+33 (0)6…".
    let mut digits = String::new();
    let mut separators = 0;
    let mut previous_was_separator = false;
    let mut parens = 0;
    let mut in_parens: Option<usize> = None;
    for &b in bytes {
        match b {
            b'0'..=b'9' => {
                digits.push(char::from(b));
                in_parens = in_parens.map(|n| n + 1);
                previous_was_separator = false;
            }
            b' ' | b'.' | b'-' => {
                if previous_was_separator || in_parens.is_some() {
                    return false;
                }
                separators += 1;
                previous_was_separator = true;
            }
            b'(' => {
                if parens > 0 || in_parens.is_some() {
                    return false;
                }
                parens += 1;
                in_parens = Some(0);
                previous_was_separator = false;
            }
            b')' => match in_parens.take() {
                Some(1..=5) => previous_was_separator = false,
                _ => return false,
            },
            _ => return false,
        }
    }
    if in_parens.is_some() {
        return false;
    }
    let count = digits.len();
    let grouped = separators > 0 || parens > 0;
    if plus {
        (7..=15).contains(&count)
    } else if digits.starts_with("00") {
        (9..=15).contains(&count)
    } else if digits.starts_with('0') {
        // A national number with its trunk prefix: France, Germany, the UK, Italy…
        (10..=12).contains(&count)
    } else {
        // North America: (555) 123-4567, always written in groups.
        count == 10 && grouped
    }
}

/// "#f80", "#ff8800cc", "rgb(255, 136, 0)", "rgba(255 136 0 / 50%)", "hsl(32deg, 100%, 50%)".
fn is_color(text: &str) -> bool {
    if let Some(hex) = text.strip_prefix('#') {
        let bytes = hex.as_bytes();
        // "#123" is more often an issue number than a colour; "#333" and "#f80" are colours.
        let short = [3, 4].contains(&bytes.len())
            && (bytes.iter().any(u8::is_ascii_alphabetic) || bytes.iter().all(|b| *b == bytes[0]));
        return (short || [6, 8].contains(&bytes.len())) && bytes.iter().all(u8::is_ascii_hexdigit);
    }
    let Some(open) = text.find('(') else {
        return false;
    };
    let function = text[..open].to_ascii_lowercase();
    if !["rgb", "rgba", "hsl", "hsla"].contains(&function.as_str()) {
        return false;
    }
    let Some(inside) = text[open + 1..].strip_suffix(')') else {
        return false;
    };
    let values: Vec<&str> = inside
        .split(|c: char| c == ',' || c == '/' || c.is_ascii_whitespace())
        .filter(|value| !value.is_empty())
        .collect();
    (3..=4).contains(&values.len()) && values.iter().all(|value| is_css_number(value))
}

/// "255", "0.5", "50%", "32deg".
fn is_css_number(value: &str) -> bool {
    let number = value
        .strip_suffix('%')
        .or_else(|| value.strip_suffix("deg"))
        .unwrap_or(value);
    let mut parts = number.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    (!whole.is_empty() || fraction.is_some_and(|f| !f.is_empty()))
        && whole.len() <= 6
        && digits(whole)
        && fraction.is_none_or(|f| !f.is_empty() && f.len() <= 6 && digits(f))
}

/// A JSON object or array (RFC 8259), at most [`MAX_JSON_LEN`] bytes and [`MAX_JSON_DEPTH`]
/// levels. Checked without recursion, so a deep text can't overflow the stack.
fn is_json(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() <= MAX_JSON_LEN
        && matches!(bytes.first(), Some(b'{' | b'['))
        && json_end(bytes) == Some(bytes.len())
}

/// Where the JSON value at the start of `b` ends, if it is valid.
fn json_end(b: &[u8]) -> Option<usize> {
    #[derive(Clone, Copy, PartialEq)]
    enum Want {
        Value,
        ValueOrClose,
        Key,
        KeyOrClose,
        Colon,
        CommaOrClose,
    }
    // The closing bracket of each open array or object.
    let mut open: Vec<u8> = Vec::new();
    let mut want = Want::Value;
    let mut at = 0;
    loop {
        while matches!(b.get(at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            at += 1;
        }
        let c = b.get(at).copied();
        want = match want {
            Want::Value | Want::ValueOrClose => match c? {
                b']' if want == Want::ValueOrClose => {
                    open.pop();
                    at += 1;
                    Want::CommaOrClose
                }
                b'{' | b'[' => {
                    if open.len() == MAX_JSON_DEPTH {
                        return None;
                    }
                    open.push(if c == Some(b'{') { b'}' } else { b']' });
                    at += 1;
                    if c == Some(b'{') {
                        Want::KeyOrClose
                    } else {
                        Want::ValueOrClose
                    }
                }
                b'"' => {
                    at = json_string_end(b, at)?;
                    Want::CommaOrClose
                }
                b'-' | b'0'..=b'9' => {
                    at = json_number_end(b, at)?;
                    Want::CommaOrClose
                }
                _ => {
                    let word = [&b"true"[..], b"false", b"null"]
                        .into_iter()
                        .find(|word| b[at..].starts_with(word))?;
                    at += word.len();
                    Want::CommaOrClose
                }
            },
            Want::Key | Want::KeyOrClose => match c? {
                b'}' if want == Want::KeyOrClose => {
                    open.pop();
                    at += 1;
                    Want::CommaOrClose
                }
                b'"' => {
                    at = json_string_end(b, at)?;
                    Want::Colon
                }
                _ => return None,
            },
            Want::Colon => {
                if c != Some(b':') {
                    return None;
                }
                at += 1;
                Want::Value
            }
            Want::CommaOrClose => {
                let Some(&close) = open.last() else {
                    return Some(at);
                };
                match c? {
                    b',' => {
                        at += 1;
                        if close == b'}' {
                            Want::Key
                        } else {
                            Want::Value
                        }
                    }
                    c if c == close => {
                        open.pop();
                        at += 1;
                        Want::CommaOrClose
                    }
                    _ => return None,
                }
            }
        };
    }
}

/// `b[at]` is the opening quote.
fn json_string_end(b: &[u8], at: usize) -> Option<usize> {
    let mut i = at + 1;
    loop {
        match *b.get(i)? {
            b'"' => return Some(i + 1),
            b'\\' => match *b.get(i + 1)? {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => i += 2,
                b'u' => {
                    let hex = b.get(i + 2..i + 6)?;
                    if !hex.iter().all(u8::is_ascii_hexdigit) {
                        return None;
                    }
                    i += 6;
                }
                _ => return None,
            },
            0..0x20 => return None,
            _ => i += 1,
        }
    }
}

/// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`
fn json_number_end(b: &[u8], at: usize) -> Option<usize> {
    let digits = |from: usize| {
        b[from.min(b.len())..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count()
    };
    let mut i = at;
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    match digits(i) {
        0 => return None,
        n if n > 1 && b[i] == b'0' => return None,
        n => i += n,
    }
    if b.get(i) == Some(&b'.') {
        let n = digits(i + 1);
        if n == 0 {
            return None;
        }
        i += 1 + n;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let n = digits(i);
        if n == 0 {
            return None;
        }
        i += n;
    }
    Some(i)
}

/// Commands that start a command line. Only words that rarely start a sentence: not "make",
/// "find" or "go".
const COMMANDS: &[&str] = &[
    "adb",
    "apt",
    "apt-get",
    "brew",
    "cargo",
    "cd",
    "chmod",
    "chown",
    "choco",
    "cmake",
    "cp",
    "curl",
    "dnf",
    "docker",
    "fastboot",
    "ffmpeg",
    "flatpak",
    "gcc",
    "git",
    "gradle",
    "grep",
    "hyprctl",
    "journalctl",
    "kubectl",
    "ls",
    "mkdir",
    "mv",
    "node",
    "npm",
    "npx",
    "pacman",
    "paru",
    "pip",
    "pip3",
    "pnpm",
    "python",
    "python3",
    "rm",
    "rsync",
    "rustup",
    "scp",
    "sed",
    "ssh",
    "sudo",
    "systemctl",
    "tar",
    "wget",
    "winget",
    "yarn",
    "yay",
];

/// Words that start a line of code and hardly ever start a line of prose in lowercase.
const CODE_KEYWORDS: &[&str] = &[
    "async",
    "const",
    "def",
    "elif",
    "enum",
    "export",
    "fn",
    "func",
    "impl",
    "import",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "struct",
    "trait",
    "val",
    "var",
];

/// Words that start a line of code only if brackets or `=`, `:`, `;` follow: "if (x)", not
/// "if you can".
const PROSE_KEYWORDS: &[&str] = &[
    "class", "else", "for", "function", "if", "let", "match", "switch", "try", "type", "use",
    "while",
];

/// Operators that hardly ever appear in prose. Not arrows: notes use them ("Paris -> Lyon").
const CODE_OPERATORS: &[&str] = &["::", "&&", "||", "!=", "==", "+=", ":=", "</"];

/// Source code or a command line: most lines look like code, or the one line clearly is.
fn is_code(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(MAX_CODE_LINES)
        .collect();
    match lines[..] {
        [] => false,
        [line] => is_code_alone(line),
        _ => {
            let code = lines.iter().filter(|line| is_code_line(line)).count();
            code >= 2 && code * 2 >= lines.len()
        }
    }
}

/// A line of a longer text that looks like code. `line` is trimmed.
fn is_code_line(line: &str) -> bool {
    if is_statement(line) {
        return true;
    }
    let first = line.split_whitespace().next().unwrap_or("");
    let has_more = line.len() > first.len();
    let brackets = line.contains(['(', ')', '{', '}', '[', ']', '=', ':', ';']);
    let keyword =
        has_more && (CODE_KEYWORDS.contains(&first) || PROSE_KEYWORDS.contains(&first) && brackets);
    // "- flour;" is a list, not code.
    let bullet = line.starts_with(['-', '*', '•', '–']);
    let punctuated = !bullet && line.ends_with(';') && !line.ends_with(" ;")
        || line.ends_with(['{', '}'])
        || line.starts_with(['}', ')', ']'])
        || line.starts_with("//")
        || line.starts_with("/*")
        || line.starts_with("#[")
        || line.starts_with("#include")
        || is_call(line)
        || is_assignment(line);
    keyword || punctuated || CODE_OPERATORS.iter().any(|op| line.contains(op))
}

/// A text of one line that clearly is code: a command, a statement, `fn main() {`,
/// `console.log(x);`, `.note { color: red; }`.
fn is_code_alone(line: &str) -> bool {
    if is_statement(line) {
        return true;
    }
    let first = line.split_whitespace().next().unwrap_or("");
    let keyword = line.len() > first.len()
        && (CODE_KEYWORDS.contains(&first) || PROSE_KEYWORDS.contains(&first));
    let brackets = line.contains(['(', '{', '=', ';']);
    let statement = line.ends_with(';') && !line.ends_with(" ;") && line.contains(['(', '=']);
    let block = line.contains('{') && line.ends_with('}') && line.contains([':', ';', '(']);
    let operator = CODE_OPERATORS.iter().any(|op| line.contains(op)) && line.contains('(');
    keyword && brackets || statement || block || operator
}

/// A command line, a SQL statement or a markup tag, whether alone or among other lines.
fn is_statement(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    let after_first = line[first.len()..].trim_start();
    // "$ cargo test", "# systemctl restart sshd", "sudo pacman -S git", "./gradlew build".
    // Not "$ 5" (money) or "# Title" (a heading).
    let prompt = first == "$"
        && after_first.starts_with(|c: char| c.is_ascii_lowercase() || c == '.')
        || first == "#" && COMMANDS.iter().any(|c| after_first.starts_with(c));
    let command = (COMMANDS.contains(&first) || first.starts_with("./")) && !after_first.is_empty();
    // SELECT … FROM, INSERT INTO, UPDATE … SET.
    let sql = [
        "SELECT ", "INSERT ", "UPDATE ", "DELETE ", "CREATE ", "ALTER ",
    ]
    .iter()
    .any(|start| line.starts_with(start))
        && [" FROM ", " INTO ", " SET ", " TABLE ", " WHERE "]
            .iter()
            .any(|word| line.contains(word));
    // `<div class="note">`, `</p>`.
    let markup = line.starts_with('<')
        && line.ends_with('>')
        && line[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/');
    prompt || command || sql || markup
}

/// `print(x)`, `foo.bar(1, 2)`: a name, its arguments, nothing after.
fn is_call(line: &str) -> bool {
    let Some(open) = line.find('(') else {
        return false;
    };
    let name = &line[..open];
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        && line.ends_with(')')
}

/// `message = "Hello " + name`, `self.count = 0`: a name, " = ", a value.
fn is_assignment(line: &str) -> bool {
    line.split_once(" = ").is_some_and(|(name, value)| {
        !name.is_empty()
            && !value.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'[' | b']'))
            && !name.bytes().all(|b| b.is_ascii_digit())
    })
}

/// Only the start of a text is checked for secrets. The history never keeps more than this.
pub const MAX_SECRET_SCAN_LEN: usize = 1024 * 1024;

/// Whether the text holds something that must never be kept: a private key, a 2FA seed
/// (`otpauth://`), an API token, a card number, or what looks like a password.
///
/// Errs on the side of caution: a false alarm only keeps a clip out of the history. Only the
/// first [`MAX_SECRET_SCAN_LEN`] bytes are read.
pub fn looks_secret(text: &str) -> bool {
    let text = head(text, MAX_SECRET_SCAN_LEN);
    has_private_key(text)
        || contains_ignore_ascii_case(text, "otpauth://")
        || contains_ignore_ascii_case(text, "otpauth-migration://")
        || text
            .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
            .any(|token| is_token(token.trim_end_matches('.')))
        || has_card_number(text)
        || is_password(text.trim())
}

/// "-----BEGIN OPENSSH PRIVATE KEY-----", "-----BEGIN PGP PRIVATE KEY BLOCK-----", a PuTTY key.
fn has_private_key(text: &str) -> bool {
    text.contains("PuTTY-User-Key-File-")
        || text.match_indices("-----BEGIN ").any(|(at, _)| {
            let line = &text[at..];
            let line = &line[..line.find('\n').unwrap_or(line.len())];
            head(line, 64).contains("PRIVATE KEY")
        })
}

fn contains_ignore_ascii_case(text: &str, needle: &str) -> bool {
    text.as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Characters allowed after a token's prefix.
#[derive(Clone, Copy)]
enum Charset {
    /// Letters and digits.
    Alnum,
    /// Uppercase letters and digits.
    UpperAlnum,
    /// Letters, digits, `_` and `-`.
    Base64Url,
}

/// API tokens by their prefix: (prefix, shortest rest, longest rest, characters of the rest).
const TOKENS: &[(&str, usize, usize, Charset)] = &[
    // GitHub.
    ("ghp_", 30, 255, Charset::Alnum),
    ("gho_", 30, 255, Charset::Alnum),
    ("ghu_", 30, 255, Charset::Alnum),
    ("ghs_", 30, 255, Charset::Alnum),
    ("ghr_", 30, 255, Charset::Alnum),
    ("github_pat_", 40, 255, Charset::Base64Url),
    // GitLab.
    ("glpat-", 20, 255, Charset::Base64Url),
    // Slack: xoxb-, xoxp-, xoxa-, xoxe-, xoxr-, xoxs-.
    ("xoxb-", 10, 255, Charset::Base64Url),
    ("xoxp-", 10, 255, Charset::Base64Url),
    ("xoxa-", 10, 255, Charset::Base64Url),
    ("xoxe-", 10, 255, Charset::Base64Url),
    ("xoxr-", 10, 255, Charset::Base64Url),
    ("xoxs-", 10, 255, Charset::Base64Url),
    // OpenAI, Anthropic ("sk-ant-…") and others.
    ("sk-", 20, 255, Charset::Base64Url),
    // Stripe.
    ("sk_live_", 16, 255, Charset::Alnum),
    ("rk_live_", 16, 255, Charset::Alnum),
    // AWS access key IDs.
    ("AKIA", 16, 16, Charset::UpperAlnum),
    ("ASIA", 16, 16, Charset::UpperAlnum),
    // Google API keys.
    ("AIza", 35, 35, Charset::Base64Url),
    // npm.
    ("npm_", 36, 36, Charset::Alnum),
];

/// An API token or a JSON Web Token.
fn is_token(token: &str) -> bool {
    let prefixed = TOKENS.iter().any(|&(prefix, min, max, charset)| {
        token.strip_prefix(prefix).is_some_and(|rest| {
            let bytes = rest.as_bytes();
            // Generated, so letters and digits are mixed: "sk-learn-tutorial" is not a key.
            let kinds = [
                bytes.iter().any(u8::is_ascii_lowercase),
                bytes.iter().any(u8::is_ascii_uppercase),
                bytes.iter().any(u8::is_ascii_digit),
            ];
            (min..=max).contains(&rest.len())
                && kinds.iter().filter(|k| **k).count() >= 2
                && bytes.iter().all(|&b| match charset {
                    Charset::Alnum => b.is_ascii_alphanumeric(),
                    Charset::UpperAlnum => b.is_ascii_uppercase() || b.is_ascii_digit(),
                    Charset::Base64Url => b.is_ascii_alphanumeric() || b == b'_' || b == b'-',
                })
        })
    });
    prefixed || is_jwt(token)
}

/// "eyJhbGciOi….eyJzdWIiOi….SflKxwRJ…": header, claims and signature in base64url.
fn is_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.splitn(4, '.').collect();
    let base64url = |part: &str| {
        part.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    matches!(parts[..], [header, claims, signature]
        if header.starts_with("eyJ") && claims.starts_with("eyJ")
            && header.len() >= 10 && claims.len() >= 10 && signature.len() >= 16
            && base64url(header) && base64url(claims) && base64url(signature))
}

/// A payment card number anywhere in the text: 13 to 19 digits, alone or in groups of at least
/// 4 split by single spaces or dashes, starting with 2 to 6, that pass the Luhn check.
fn has_card_number(text: &str) -> bool {
    let b = text.as_bytes();
    let mut at = 0;
    while at < b.len() {
        let starts_number =
            b[at].is_ascii_digit() && (at == 0 || !b[at - 1].is_ascii_alphanumeric());
        if !starts_number {
            at += 1;
            continue;
        }
        // "FR76 3000 6000 …": the groups of an IBAN, which is not a secret.
        let after_iban = at >= 5
            && b[at - 1] == b' '
            && b[at - 5..at - 3].iter().all(u8::is_ascii_uppercase)
            && b[at - 3..at - 1].iter().all(u8::is_ascii_digit);
        let mut digits = Vec::new();
        let mut i = at;
        // Just after the last group taken.
        let mut end = at;
        loop {
            let group = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
            // "4111 1111 1111 1111 12/27": a short group is not part of the number.
            if group < 4 && !digits.is_empty() {
                break;
            }
            digits.extend_from_slice(&b[i..i + group]);
            i += group;
            end = i;
            let separated = matches!(b.get(i), Some(b' ' | b'-'))
                && b.get(i + 1).is_some_and(u8::is_ascii_digit);
            if group < 4 || digits.len() >= 19 || !separated {
                break;
            }
            i += 1;
        }
        let ends_number = b.get(end).is_none_or(|c| !c.is_ascii_alphanumeric());
        if ends_number
            && !after_iban
            && (13..=19).contains(&digits.len())
            && (b'2'..=b'6').contains(&digits[0])
            && luhn(&digits)
        {
            return true;
        }
        at = end.max(at + 1);
    }
    false
}

/// The Luhn check digit test, on ASCII digits.
fn luhn(digits: &[u8]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            let d = u32::from(d - b'0');
            if i % 2 == 1 {
                if d * 2 > 9 { d * 2 - 9 } else { d * 2 }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// A whole clip that looks like a generated password: one ASCII word of 12 to 128 characters,
/// with 3 kinds of characters (lowercase, uppercase, digits, symbols) mixed together, high
/// entropy, and no real word in it. Links, emails, paths, file names, versions, hex strings
/// (hashes, UUIDs) and identifiers (`user_name123`, `getElementById`) are not.
///
/// Tuned on the cases in the tests; a password made of words ("MaisonDupont2019") is missed.
fn is_password(token: &str) -> bool {
    let len = token.len();
    if !(12..=128).contains(&len) || !token.bytes().all(|b| b.is_ascii_graphic()) {
        return false;
    }
    let looks_like_something_else = token.contains("://")
        || token.starts_with("www.")
        || token.starts_with(['/', '~', '.', '\\'])
        || token.get(1..3) == Some(":\\")
        || is_email(token)
        || is_phone(token)
        || has_site_before_slash(token)
        || has_file_extension(token)
        || is_version(token)
        || token
            .bytes()
            .filter(|b| !matches!(b, b'-' | b':'))
            .all(|b| b.is_ascii_hexdigit());
    if looks_like_something_else {
        return false;
    }
    let class = |b: u8| match b {
        b'a'..=b'z' => 0,
        b'A'..=b'Z' => 1,
        b'0'..=b'9' => 2,
        _ => 3,
    };
    let bytes = token.as_bytes();
    let mut kinds = [false; 4];
    for &b in bytes {
        kinds[class(b)] = true;
    }
    // Runs of the same kind: "user_name123" has 4, "kT9mWq2z" has 8.
    let runs = 1 + bytes
        .windows(2)
        .filter(|pair| class(pair[0]) != class(pair[1]))
        .count();
    let digits = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    kinds.iter().filter(|k| **k).count() >= 3
        && runs * 2 >= len
        // Mostly digits: a date, a time, a reference.
        && digits * 2 <= len
        && longest_word(bytes) < 6
        && entropy_bits(bytes) >= 3.0
}

/// "example.com/abc": a site, then a path.
fn has_site_before_slash(token: &str) -> bool {
    token.split_once('/').is_some_and(|(site, _)| {
        let tld = site.rsplit('.').next().unwrap_or("");
        site.contains('.')
            && is_host_name(site)
            && tld.len() >= 2
            && tld.bytes().all(|b| b.is_ascii_alphabetic())
    })
}

/// "report_2024-Q3.pdf", "IMG_1234.jpeg".
fn has_file_extension(token: &str) -> bool {
    token.rsplit_once('.').is_some_and(|(name, extension)| {
        !name.is_empty()
            && (2..=4).contains(&extension.len())
            && extension.bytes().all(|b| b.is_ascii_lowercase())
    })
}

/// "v1.2.3", "2.4.0-beta.1", "1.25-alpine".
fn is_version(token: &str) -> bool {
    let token = token.strip_prefix(['v', 'V']).unwrap_or(token);
    let (numbers, _suffix) = token.split_once(['-', '+']).unwrap_or((token, ""));
    numbers.contains('.')
        && numbers
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// Length of the longest word-like run: lowercase letters, maybe after one capital ("Report").
fn longest_word(bytes: &[u8]) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for &b in bytes {
        current = if b.is_ascii_lowercase() {
            current + 1
        } else if b.is_ascii_uppercase() {
            1
        } else {
            0
        };
        longest = longest.max(current);
    }
    longest
}

/// Shannon entropy of the characters, in bits per character.
fn entropy_bits(bytes: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for &b in bytes {
        counts[usize::from(b)] += 1;
    }
    let len = bytes.len() as f64;
    counts
        .iter()
        .filter(|&&n| n > 0)
        .map(|&n| {
            let p = f64::from(n) / len;
            -p * p.log2()
        })
        .sum()
}

/// A number of 4 to 8 digits and nothing else, maybe in two groups ("482 913", "4829-1374"):
/// often a 2FA code copied by hand, sometimes a postcode. The history keeps it for a short time
/// only.
pub fn bare_code(text: &str) -> bool {
    let text = text.trim();
    let digits = text.bytes().filter(u8::is_ascii_digit).count();
    let groups: Vec<&str> = text.split([' ', '-']).collect();
    let well_formed = match groups[..] {
        [one] => !one.is_empty(),
        [a, b] => a.len() == b.len() && (3..=4).contains(&a.len()),
        _ => false,
    };
    well_formed
        && CODE_DIGITS.contains(&digits)
        && groups
            .iter()
            .all(|group| group.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(text: &str) -> Option<String> {
        link(text).map(|link| link.host)
    }

    #[test]
    fn a_copied_link_is_found_with_its_host() {
        let found = link("  https://www.Example.com/path?q=1#top\n").unwrap();
        assert_eq!(found.url, "https://www.Example.com/path?q=1#top");
        assert_eq!(found.host, "www.example.com");

        assert_eq!(host("HTTP://localhost:8080"), Some("localhost".into()));
        assert_eq!(
            host("http://192.168.1.20/admin"),
            Some("192.168.1.20".into())
        );
        assert_eq!(
            host("https://[2001:DB8::1]:443/"),
            Some("[2001:db8::1]".into())
        );
        assert_eq!(host("https://example.com."), Some("example.com".into()));
        assert_eq!(
            host("https://fr.wikipedia.org/wiki/Presse-papiers_(informatique)"),
            Some("fr.wikipedia.org".into())
        );
    }

    #[test]
    fn text_that_is_more_than_a_link_is_not_one() {
        for text in [
            "",
            "example.com",
            "look: https://example.com",
            "https://example.com and more",
            "https://example.com\nhttps://example.org",
            "www.example.com/page",
            "ftp://example.com/file",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://",
            "https:///path",
        ] {
            assert_eq!(link(text), None, "{text:?}");
        }
    }

    #[test]
    fn misleading_or_malformed_hosts_are_refused() {
        for text in [
            // Leads to evil.example, not to the bank.
            "https://bank.example@evil.example/login",
            "https://user:pass@example.com",
            // Look-alike letters.
            "https://exаmple.com",
            "https://-example.com",
            "https://example-.com",
            "https://exa_mple.com",
            "https://example..com",
            "https://example.com:",
            "https://example.com:99999",
            "https://example.com:80a",
            "https://[]/",
            "https://[zz::1]/",
            "https://example.com/\"quoted\"",
            "https://example.com/a|b",
        ] {
            assert_eq!(link(text), None, "{text:?}");
        }
    }

    #[test]
    fn codes_are_found_in_real_messages() {
        for (text, code) in [
            ("G-482913 is your Google verification code.", "482913"),
            (
                "Votre code de vérification est 482 913. Ne le partagez pas.",
                "482913",
            ),
            (
                "Your WhatsApp code: 123-456\nDon't share this code",
                "123456",
            ),
            ("Your Uber code: 7391. Never share this code.", "7391"),
            (
                "Amazon: 731948 is your one-time password. Don't share it.",
                "731948",
            ),
            ("Tu código de Instagram es 58204917", "58204917"),
            ("Ihr Bestätigungscode lautet 4821", "4821"),
            ("123456 est votre mot de passe à usage unique", "123456"),
            ("Use 4829-1374 to sign in", "48291374"),
            // The number next to the word, not the card or the amount.
            (
                "Paiement de 49,99 € avec la carte 1234. Code : 584921",
                "584921",
            ),
            ("Meeting at 12:30, door code 4821", "4821"),
        ] {
            assert_eq!(one_time_code(text).as_deref(), Some(code), "{text:?}");
        }
    }

    #[test]
    fn numbers_that_are_not_codes_are_left_alone() {
        for text in [
            "Your order #123456 has shipped",
            "Your parcel 482913 is shipping today",
            "Payment of 1234,56 accepted",
            "Your code expires in 10 minutes",
            "Promo code SUMMER2025",
            "Call +33612345678 to verify",
            "Code valid until 12/05/2026",
            "Version 2.4.1801 installed, see the release code",
            "Your code costs 1500€",
            "",
            // Two codes, as close to a word each: no guess.
            "Door code 1234, wifi password 56789",
        ] {
            assert_eq!(one_time_code(text), None, "{text:?}");
        }
    }

    #[test]
    fn only_the_start_of_a_long_text_is_searched() {
        let text = format!("{} code 482913", "x ".repeat(MAX_CODE_TEXT_CHARS));
        assert_eq!(one_time_code(&text), None);
    }

    #[test]
    fn length_is_bounded() {
        let long = format!("https://example.com/{}", "a".repeat(MAX_LINK_LEN));
        assert_eq!(link(&long), None);
        let label = format!("https://{}.com", "a".repeat(64));
        assert_eq!(link(&label), None);
    }

    fn assert_category(category_of: Category, texts: &[&str]) {
        for text in texts {
            assert_eq!(category(text), category_of, "{text:?}");
        }
    }

    #[test]
    fn links_emails_and_phone_numbers_alone() {
        assert_category(
            Category::Link,
            &[
                "https://github.com/Nexoryx-business/paste-get",
                " http://192.168.1.20:8080/admin\n",
            ],
        );
        assert_category(
            Category::Email,
            &[
                "jean.dupont@example.fr",
                " a+tag@sub.example.co.uk\n",
                "o'brien@example.ie",
            ],
        );
        assert_category(
            Category::Phone,
            &[
                "+33 6 12 34 56 78",
                "+33 (0)6 12 34 56 78",
                "06 12 34 56 78",
                "06.12.34.56.78",
                "0612345678",
                "0033 6 12 34 56 78",
                "+4915112345678",
                "(555) 123-4567",
                "555-123-4567",
                "+1-555-123-4567",
                "020 7946 0958",
            ],
        );
    }

    #[test]
    fn text_around_or_odd_forms_are_just_text() {
        assert_category(
            Category::Text,
            &[
                // Not emails.
                "@example.com",
                "user@localhost",
                "user@example",
                "user@@example.com",
                "user@exa_mple.com",
                "user@example.com.",
                "jean dupont@example.fr",
                "écris à jean@example.fr",
                ".jean@example.fr",
                "jean..dupont@example.fr",
                // Not phone numbers: dates, codes, cards, ISBNs, sentences.
                "12.05.2026",
                "2026-11-09",
                "12:30",
                "482913",
                "1234567890",
                "4111 1111 1111 1111",
                "978-2-07-036822-8",
                "+33",
                "06 12 34",
                "appelle le 06 12 34 56 78",
                "06--12-34-56-78",
                "(06) (12) 34 56 78",
                "(0612345678",
                "",
                "   ",
            ],
        );
    }

    #[test]
    fn colours() {
        assert_category(
            Category::Color,
            &[
                "#f80",
                "#333",
                "#FF8800",
                "#ff8800cc",
                "#000000",
                "rgb(255, 136, 0)",
                "RGBA(255 136 0 / 50%)",
                "hsl(32deg, 100%, 50%)",
                "hsla(32, 100%, 50%, 0.5)",
                "rgb(.5, 1.0, 2)",
            ],
        );
        assert_category(
            Category::Text,
            &[
                "#123",
                "#ff880",
                "#hashtag",
                "#123456789",
                "rgb(255, 136)",
                "rgb(a, b, c)",
                "color: rgb(1, 2, 3)",
                "rgb(1, 2, 3, 4, 5)",
            ],
        );
        assert_eq!(category("rgb(255, 136, 0"), Category::Text);
    }

    #[test]
    fn json_objects_and_arrays() {
        assert_category(
            Category::Json,
            &[
                "{}",
                "[]",
                " [1, 2, 3] ",
                r#"{"a": 1, "b": [true, false, null, -1.5e3, 0, "x\n\u00e9 \"q\""]}"#,
                "{\n  \"name\": \"paste-get\",\n  \"tags\": [\"clipboard\", \"sync\"],\n  \"n\": {\"x\": {}}\n}",
            ],
        );
        for text in [
            "{a: 1}",
            r#"{"a": 1,}"#,
            "[1, 2",
            r#"{"a" 1}"#,
            "[01]",
            "[1.]",
            "[-]",
            "[1e]",
            r#"["\x"]"#,
            r#"["\u12"]"#,
            "[\"tab\there\"]",
            "[1] [2]",
            r#""just a string""#,
            "[nul]",
            "{\"a\": 1}}",
        ] {
            assert_ne!(category(text), Category::Json, "{text:?}");
        }
        let deep = format!(
            "{}{}",
            "[".repeat(MAX_JSON_DEPTH),
            "]".repeat(MAX_JSON_DEPTH)
        );
        assert_eq!(category(&deep), Category::Json);
        let too_deep = format!("[{deep}]");
        assert_ne!(category(&too_deep), Category::Json);
        // Very deep input doesn't overflow the stack.
        let abyss = "[".repeat(1_000_000);
        assert_ne!(category(&abyss), Category::Json);
    }

    #[test]
    fn code_and_command_lines() {
        assert_category(
            Category::Code,
            &[
                "fn main() {\n    let x = 5;\n    println!(\"{x}\");\n}",
                "def greet(name):\n    message = \"Hello \" + name\n    return message",
                "const total = items.reduce((a, b) => a + b, 0);\nconsole.log(total);",
                "#include <stdio.h>\nint main(void) {\n  return 0;\n}",
                ".note {\n  color: red;\n  margin: 0;\n}",
                "<ul>\n  <li>One</li>\n  <li>Two</li>\n</ul>",
                "git clone https://github.com/Nexoryx-business/paste-get",
                "sudo pacman -S --needed rustup git",
                "$ cargo test --workspace",
                "./gradlew assembleFossDebug",
                "SELECT id, name FROM users WHERE id = 1;",
                "console.log(x);",
                "fn main() {",
                "const x = 5;",
                ".note { color: red; }",
                "if (a == b) {",
                "<div class=\"note\">",
                "use std::io;",
            ],
        );
        assert_category(
            Category::Text,
            &[
                "Rendez-vous à 14h30 devant la gare ; n'oublie pas les clés.",
                "Courses :\n- 200 g de farine ;\n- 3 œufs ;\n- du lait.",
                "Shopping:\n- flour;\n- eggs;\n- milk.",
                "if you can, let me know\nfor sure\nwhile you're there",
                "Paris -> Lyon\nLyon -> Marseille",
                "Hello (world)",
                "cd",
                "git",
                "$ 5 for a coffee",
                "# Title",
                "Note: see below.",
                "12 rue de la Paix\n75002 Paris",
                "Le vent se lève ;\nil faut tenter de vivre.",
                "Total = 12 €",
                "Dear Paul,\nthanks for the call.\nBest,\nAnne",
                "return to sender",
                "{name}",
            ],
        );
    }

    /// `prefix` followed by its Luhn check digit.
    fn with_check_digit(prefix: &str) -> String {
        (0..=9)
            .map(|d| format!("{prefix}{d}"))
            .find(|n| luhn(n.as_bytes()))
            .unwrap()
    }

    /// Tokens are put together at run time, so no file here looks like it leaks a real one.
    fn tokens() -> Vec<String> {
        let mixed = "aB3dE5gH7jK9mN1pQ3sT5vW7yZ9bC2eF4";
        vec![
            format!("ghp_{}", &mixed[..36.min(mixed.len())]),
            format!("gho_{mixed}"),
            format!("github_pat_11AB{mixed}_{mixed}"),
            format!("glpat-{}", &mixed[..20]),
            format!("xoxb-1234567890-{}", &mixed[..12]),
            format!("sk-{mixed}"),
            format!("sk-ant-api03-{mixed}-{mixed}"),
            format!("sk-proj-{mixed}"),
            format!("sk_live_{}", &mixed[..24]),
            format!("AKIA{}", "IOSFODNN7EXAMPLE"),
            format!("AIza{}", &format!("{mixed}{mixed}")[..35]),
            format!("npm_{}", &format!("{mixed}{mixed}")[..36]),
            format!(
                "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.{}",
                "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
            ),
        ]
    }

    #[test]
    fn keys_tokens_and_seeds_are_secrets() {
        let mut secrets = vec![
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjE\n-----END OPENSSH PRIVATE KEY-----".to_owned(),
            "-----BEGIN RSA PRIVATE KEY-----".into(),
            "-----BEGIN PRIVATE KEY-----".into(),
            "-----BEGIN ENCRYPTED PRIVATE KEY-----".into(),
            "-----BEGIN EC PRIVATE KEY-----".into(),
            "-----BEGIN PGP PRIVATE KEY BLOCK-----".into(),
            "my key:\n-----BEGIN OPENSSH PRIVATE KEY-----\n…".into(),
            "PuTTY-User-Key-File-3: ssh-ed25519".into(),
            "otpauth://totp/GitHub:me?secret=JBSWY3DPEHPK3PXP&issuer=GitHub".into(),
            "OTPAUTH://hotp/x?secret=ABC".into(),
            "otpauth-migration://offline?data=CjEKCkhlbGxvId6".into(),
        ];
        for token in tokens() {
            secrets.push(format!("export API_TOKEN=\"{token}\""));
            secrets.push(format!("GITHUB_TOKEN={token}."));
            secrets.push(token);
        }
        for text in &secrets {
            assert!(looks_secret(text), "{text:?}");
        }
    }

    #[test]
    fn card_numbers_pass_the_luhn_check() {
        for text in [
            "4111 1111 1111 1111",
            "4111111111111111",
            "4111-1111-1111-1111",
            "5555 5555 5555 4444",
            "3782 822463 10005",
            "378282246310005",
            "6011111111111117",
            "30569309025904",
            "3530111333300000",
            "Carte : 4242 4242 4242 4242, exp. 12/27",
            "4242 4242 4242 4242 12/27",
        ] {
            assert!(looks_secret(text), "{text:?}");
        }
        for text in [
            // Luhn fails.
            "4111 1111 1111 1112",
            // Too short, too long, or in short groups like a phone number.
            "4111 1111 1111",
            "41111111111111111111",
            "41 11 11 11 11 11 11 11",
            // Not a card's first digit.
            &with_check_digit("811111111111111"),
            &with_check_digit("111111111111111"),
            // Part of a word or a longer number.
            &format!("A{}", with_check_digit("411111111111111")),
            &format!("{}x", with_check_digit("411111111111111")),
            // An IBAN, even when its digits happen to pass.
            &format!("FR76 {}", with_check_digit("411111111111111")),
            "FR76 3000 6000 0112 3456 7890 189",
            "0612345678 0698765432",
        ] {
            assert!(!looks_secret(text), "{text:?}");
        }
        assert!(looks_secret(&with_check_digit("411111111111111")));
    }

    #[test]
    fn generated_passwords_are_secrets() {
        for text in [
            "x7#Kp2!vQz9&mR4t",
            "Tr0ub4dor&3xQ",
            "P@ssw0rd!2024",
            "Yq3uV9cK2mT8bZ",
            "hX8$pL2#qN5!wR",
            "aZ3$fG7*hJ1@",
            "  3f#Gh9!Lm2&Qw\n",
            "VK7JG-NPHTM-C97JM-9MPGT-3V66T",
        ] {
            assert!(looks_secret(text), "{text:?}");
        }
    }

    #[test]
    fn everyday_clips_are_not_secrets() {
        for text in [
            // IBANs are printed on invoices.
            "FR76 3000 6000 0112 3456 7890 189",
            "FR7630006000011234567890189",
            "DE89370400440532013000",
            // Codes are handled by the history (kept a short while).
            "482913",
            "75002",
            "Rendez-vous demain à 14h30 devant la gare ; apporte les clés.",
            "https://example.com/reset?token=abc123XYZ",
            "https://github.com/Nexoryx-business/paste-get",
            "www.example.com/AbC123",
            "t.co/AbC123xYz",
            "fn main() { println!(\"hi\"); }",
            "{\"key\": \"value\"}",
            "#ff8800",
            "user_name123",
            "MyClass.method()",
            "getElementById('x')",
            "camelCaseVariable2",
            "SKU-12345-XL",
            "REF-2026-AB12CD",
            "Report_2024-Q3.pdf",
            "IMG_20261109_123045.jpg",
            "v1.2.3-beta.4",
            "nginx:1.25-alpine",
            "2026-11-09T12:30:00Z",
            "C:\\Users\\Paul\\Documents",
            "/home/user/.config/paste-get",
            "~/Downloads/file.txt",
            "jean.dupont42@example.fr",
            "+33 6 12 34 56 78",
            "@Nexoryx_business",
            "#ParisJO2024Live",
            "Livebox-A1B2",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "5092158a1b2c",
            "550e8400-e29b-41d4-a716-446655440000",
            "00:1A:2B:3C:4D:5E",
            "1Z999AA10123456784",
            // Look-alikes of token prefixes.
            "sk-learn-tutorial-for-beginners",
            "desk-AB12345678901234567890",
            "AKIA",
            "ghp_short",
            "the sk- prefix",
            "BEGIN PRIVATE KEY",
            "-----BEGIN CERTIFICATE-----",
            "-----BEGIN PUBLIC KEY-----",
            "otpauth",
            "",
        ] {
            assert!(!looks_secret(text), "{text:?}");
        }
    }

    #[test]
    fn only_the_start_of_a_huge_text_is_searched_for_secrets() {
        let key = "-----BEGIN PRIVATE KEY-----";
        let early = format!("{key}{}", "x".repeat(MAX_SECRET_SCAN_LEN));
        assert!(looks_secret(&early));
        let late = format!("{}{key}", "x".repeat(MAX_SECRET_SCAN_LEN));
        assert!(!looks_secret(&late));
        // Cutting never splits a character.
        let accents = "é".repeat(MAX_SECRET_SCAN_LEN);
        assert!(!looks_secret(&accents));
        assert_eq!(head("aé", 2), "a");
        assert_eq!(head("aé", 3), "aé");
        assert_eq!(category(&"é".repeat(MAX_CODE_SCAN_LEN + 1)), Category::Text);
    }

    proptest::proptest! {
        // The fuzz target's checks, on text made of the characters these rules look at.
        #[test]
        fn rules_are_stable_and_never_panic(text in "[ -~\\téü€\n]{0,80}") {
            let found = category(&text);
            proptest::prop_assert_eq!(category(&text), found);
            proptest::prop_assert_eq!(found == Category::Link, link(&text).is_some());
            let secret = looks_secret(&text);
            proptest::prop_assert_eq!(looks_secret(&format!(" {text}\n")), secret);
            if bare_code(&text) {
                proptest::prop_assert!(!secret);
            }
        }
    }

    #[test]
    fn bare_codes_are_numbers_alone() {
        for text in [
            "482913",
            " 4829 ",
            "482 913",
            "4829-1374",
            "75002",
            "12345678",
        ] {
            assert!(bare_code(text), "{text:?}");
        }
        for text in [
            "123",
            "123456789",
            "48 29 13",
            "482 9134",
            "4829 13",
            "code 482913",
            "48291a",
            "-4829",
            "",
        ] {
            assert!(!bare_code(text), "{text:?}");
        }
    }
}
