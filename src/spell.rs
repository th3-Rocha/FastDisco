//! Spell checking for the message field, as Discord's: Hunspell
//! dictionaries for Brazilian Portuguese and American English, a word
//! counts as right when either knows it. The system's dictionaries are
//! used when installed; otherwise LibreOffice's are downloaded once into
//! the cache folder, as Discord's desktop app downloads Chromium's.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

use spellbook::Dictionary;

/// LibreOffice's dictionaries, at a release tag so they do not move.
const SOURCE: &str =
    "https://raw.githubusercontent.com/LibreOffice/dictionaries/libreoffice-25.8.1.1";

/// The languages checked, as (Hunspell name, folder in LibreOffice's tree).
const LANGUAGES: [(&str, &str); 2] = [("pt_BR", "pt_BR"), ("en_US", "en")];

pub struct Speller {
    dictionaries: Vec<Dictionary>,
}

impl std::fmt::Debug for Speller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Speller({} dictionaries)", self.dictionaries.len())
    }
}

impl Speller {
    /// Whether any dictionary knows the word, as written or lowercased
    /// (the first word of a sentence is capitalised).
    pub fn check(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        self.dictionaries
            .iter()
            .any(|d| d.check(word) || d.check(&lower))
    }

    /// Up to five corrections, the first dictionary's first.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for dictionary in &self.dictionaries {
            let mut list = Vec::new();
            dictionary.suggest(word, &mut list);
            for s in list {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            if out.len() >= 5 {
                break;
            }
        }
        out.truncate(5);
        out
    }

    /// The byte ranges of the words in `text` no dictionary knows, leaving
    /// out code, links, mentions, emoji, numbers and acronyms, and the
    /// user's own words.
    pub fn misspelled(&self, text: &str, own: &HashSet<String>) -> Vec<Range<usize>> {
        words(text)
            .into_iter()
            .filter(|r| {
                let word = &text[r.clone()];
                !own.contains(&word.to_lowercase()) && !self.check(word)
            })
            .collect()
    }
}

/// The words worth checking, as byte ranges: text between spaces that
/// is not code, a link, a mention, an emoji or a number, and in it the
/// runs of letters (with apostrophes and hyphens between them) that are
/// not acronyms.
pub fn words(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut code = false;
    let mut at = 0;
    for token in text.split_inclusive(char::is_whitespace) {
        let start = at;
        at += token.len();
        let token = token.trim_end();
        // A backtick opens or closes code; what it covers is not checked,
        // and after the backtick that closes it the token counts again.
        let ticks = token.matches('`').count();
        let was_code = code;
        if ticks % 2 == 1 {
            code = !code;
        }
        if code || (ticks == 0 && was_code) {
            continue;
        }
        if token.starts_with("http")
            || token.starts_with("www.")
            || token.starts_with('<')
            || token.starts_with(':')
            || token.starts_with('@')
            || token.starts_with('#')
            || token.contains("://")
            || token.contains('/')
            || token.contains('_')
            || token.chars().any(|c| c.is_ascii_digit())
        {
            continue;
        }
        // After a closing backtick only the rest of the token counts.
        let (base, token) = match token.rfind('`') {
            Some(i) => (start + i + 1, &token[i + 1..]),
            None => (start, token),
        };
        let chars: Vec<(usize, char)> = token.char_indices().collect();
        let mut begin: Option<usize> = None;
        for (i, &(offset, c)) in chars.iter().enumerate() {
            let inner = matches!(c, '\'' | '’' | '-')
                && begin.is_some()
                && chars.get(i + 1).is_some_and(|(_, n)| n.is_alphabetic());
            if c.is_alphabetic() || inner {
                begin.get_or_insert(offset);
            } else if let Some(b) = begin.take() {
                push_word(&mut out, text, base + b..base + offset);
            }
        }
        if let Some(b) = begin {
            push_word(&mut out, text, base + b..base + token.len());
        }
    }
    out
}

fn push_word(out: &mut Vec<Range<usize>>, text: &str, range: Range<usize>) {
    let word = &text[range.clone()];
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    let acronym = word.chars().all(|c| !c.is_lowercase());
    if letters > 1 && !acronym {
        out.push(range);
    }
}

/// Where dictionaries may already be on this machine.
fn system_folders() -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = [
        "/usr/share/hunspell",
        "/usr/share/myspell",
        "/usr/share/myspell/dicts",
        "/usr/local/share/hunspell",
        "/opt/homebrew/share/hunspell",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = std::env::var_os("HOME") {
        list.push(Path::new(&home).join("Library/Spelling"));
    }
    list
}

/// Hunspell files are UTF-8 or Latin-1; spellbook reads UTF-8.
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|e| e.into_bytes().iter().map(|b| char::from(*b)).collect())
}

/// One language's .aff and .dic: the system's, the cached ones, or
/// downloaded into the cache.
async fn files(
    client: &reqwest::Client,
    name: &str,
    folder: &str,
) -> Result<(String, String), String> {
    let cache = crate::settings::DIRS
        .installed()
        .map(|d| d.cache.join("dictionaries"))
        .ok_or("sem pasta de cache")?;
    for dir in system_folders().into_iter().chain([cache.clone()]) {
        let (aff, dic) = (
            dir.join(format!("{name}.aff")),
            dir.join(format!("{name}.dic")),
        );
        if let (Ok(a), Ok(d)) = (std::fs::read(&aff), std::fs::read(&dic)) {
            return Ok((text(a), text(d)));
        }
    }
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    for ext in ["aff", "dic"] {
        let url = format!("{SOURCE}/{folder}/{name}.{ext}");
        let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{url}: {}", response.status()));
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
        // Written whole, then renamed, so a cut download is never read.
        let path = cache.join(format!("{name}.{ext}"));
        let partial = path.with_extension(format!("{ext}.part"));
        std::fs::write(&partial, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
        parts.push(text(bytes));
    }
    let dic = parts.pop().unwrap_or_default();
    let aff = parts.pop().unwrap_or_default();
    Ok((aff, dic))
}

/// Every language that can be had; an error only when none can.
pub async fn load(client: reqwest::Client) -> Result<Speller, String> {
    let mut dictionaries = Vec::new();
    let mut errors = Vec::new();
    for (name, folder) in LANGUAGES {
        match files(&client, name, folder).await {
            Ok((aff, dic)) => {
                let parsed = tokio::task::spawn_blocking(move || Dictionary::new(&aff, &dic))
                    .await
                    .map_err(|e| e.to_string())?;
                match parsed {
                    Ok(d) => dictionaries.push(d),
                    Err(e) => errors.push(format!("{name}: {e}")),
                }
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    if dictionaries.is_empty() {
        return Err(errors.join("; "));
    }
    for e in errors {
        log::warn!("spell checking: {e}");
    }
    Ok(Speller { dictionaries })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        words(text).into_iter().map(|r| &text[r]).collect()
    }

    /// Downloads (once) and reads the real dictionaries:
    /// `cargo test spell::tests::the_dictionaries_correct -- --ignored`.
    #[test]
    #[ignore = "downloads the dictionaries"]
    fn the_dictionaries_correct() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let speller = runtime
            .block_on(load(reqwest::Client::new()))
            .expect("loads");
        for right in ["olá", "Amanhã", "conversa", "hello", "world", "você"] {
            assert!(speller.check(right), "{right}");
        }
        assert!(!speller.check("jogaar"));
        assert!(speller.suggest("jogaar").contains(&"jogar".to_owned()));
        let own = HashSet::from(["discofast".to_owned()]);
        let text = "oi Discofast, tudo bem com voce?";
        let wrong: Vec<&str> = speller
            .misspelled(text, &own)
            .into_iter()
            .map(|r| &text[r])
            .collect();
        assert_eq!(wrong, vec!["voce"]);
    }

    #[test]
    fn only_real_words_are_checked() {
        assert_eq!(
            found("Olá, tudo bem? Vamos jogar d'água amanhã-feira"),
            vec![
                "Olá",
                "tudo",
                "bem",
                "Vamos",
                "jogar",
                "d'água",
                "amanhã-feira"
            ]
        );
        assert_eq!(
            found(
                "veja https://exemplo.com <@123> :smile: @fulano #geral `codigo errado` NASA ok2 a"
            ),
            vec!["veja"]
        );
        assert_eq!(found("```\nfn main\n```fim"), vec!["fim"]);
    }
}
