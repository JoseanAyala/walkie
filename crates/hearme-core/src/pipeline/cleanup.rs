use regex::Regex;

/// Fast, deterministic transcript cleanup. No LLMs here — see pipeline::polish.
pub fn clean(raw: &str, fillers: &[String]) -> String {
    let mut s = raw.trim().to_string();
    for f in fillers {
        let re = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(f))).unwrap();
        s = re.replace_all(&s, "").into_owned();
    }
    s = collapse_repeats(&s);
    for (pat, rep) in [
        (r"\s{2,}", " "),
        (r"\s+([,.!?;:])", "$1"),
        (r",\s*,", ","),
        (r"\.\s*\.", "."),
        (r"!\s*!", "!"),
        (r"\?\s*\?", "?"),
        (r";\s*;", ";"),
        (r":\s*:", ":"),
    ] {
        s = Regex::new(pat).unwrap().replace_all(&s, rep).into_owned();
    }
    let s = s
        .trim()
        .trim_start_matches(|c: char| ",.;: ".contains(c))
        .trim();
    capitalize_first(s)
}

/// Collapse "the the" → "the", but never across sentence-ending punctuation
/// and never for different words ("the theme" stays).
fn collapse_repeats(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for w in s.split_whitespace() {
        let norm = normalize(w);
        if let Some(prev) = out.last() {
            let prev_ends_sentence = prev.ends_with(['.', '!', '?']);
            if !norm.is_empty() && normalize(prev) == norm && !prev_ends_sentence {
                // Keep whichever carries punctuation (the longer token).
                if w.len() > prev.len() {
                    let i = out.len() - 1;
                    out[i] = w;
                }
                continue;
            }
        }
        out.push(w);
    }
    out.join(" ")
}

fn normalize(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

fn capitalize_first(s: &str) -> String {
    let mut done = false;
    s.chars()
        .map(|c| {
            if !done && c.is_alphabetic() {
                done = true;
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::clean;

    fn en() -> Vec<String> {
        vec!["um".into(), "uh".into(), "you know".into()]
    }
    fn es() -> Vec<String> {
        vec!["este".into(), "eh".into(), "o sea".into()]
    }

    #[test]
    fn strips_leading_filler_and_capitalizes() {
        assert_eq!(clean("um, hello world.", &en()), "Hello world.");
    }

    #[test]
    fn strips_mid_sentence_filler_and_fixes_commas() {
        assert_eq!(clean("It was, you know, fine.", &en()), "It was, fine.");
    }

    #[test]
    fn strips_spanish_fillers() {
        assert_eq!(
            clean("Este, hola a todos, eh, gracias.", &es()),
            "Hola a todos, gracias."
        );
    }

    #[test]
    fn collapses_immediate_repeats() {
        assert_eq!(clean("this is is a test", &en()), "This is a test");
    }

    #[test]
    fn repeat_collapse_ignores_prefix_words() {
        assert_eq!(clean("the theme is good", &en()), "The theme is good");
    }

    #[test]
    fn repeat_kept_across_sentence_boundary() {
        assert_eq!(
            clean("stop. stop right there", &en()),
            "Stop. stop right there"
        );
    }

    #[test]
    fn multiword_filler_removed() {
        assert_eq!(clean("so you know it works", &en()), "So it works");
    }

    #[test]
    fn empty_and_whitespace_safe() {
        assert_eq!(clean("", &en()), "");
        assert_eq!(clean("   ", &en()), "");
    }

    #[test]
    fn accents_survive() {
        assert_eq!(clean("qué rápido funciona", &es()), "Qué rápido funciona");
    }
}
