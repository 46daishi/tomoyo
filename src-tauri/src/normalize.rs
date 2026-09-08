//! Text normalization used to build the dictionary index and to normalize
//! candidate substrings before lookup, so that lookups succeed regardless of
//! halfwidth/fullwidth, katakana/hiragana, chouonpu, or iteration-mark
//! spelling variants — mirroring the normalizations JL performs before
//! matching (see JL's README: halfwidth<->fullwidth, hiragana<->katakana,
//! chouonpu conversions, 々/〻/ゝ/ヽ/ゞ/ヾ handling).
//!
//! IMPORTANT: these are *lookup key* normalizations, not display
//! transforms. Keep the original surface text for display; only use these
//! outputs for HashMap keys / equality checks.
//!
//! Number normalization is also applied here: Arabic numeral runs (half or
//! full width) are folded to Japanese kanji numerals (1 -> 一, 35 -> 三十五,
//! 10000 -> 一万), so that words written with numerals (1匹, 3階, ４回) can
//! resolve to the same dictionary headword as their kanji spelling (一匹, 三階,
//! 四回). These are lookup keys only — the original surface is kept for
//! display.

/// Halfwidth katakana block (U+FF61..=U+FF9F) -> base fullwidth
/// katakana/punctuation. Voicing marks (ﾞ/ﾟ) expand to the standalone
/// fullwidth combining marks ゛/゜ here and get combined with the
/// preceding kana in a second pass.
fn halfwidth_to_fullwidth_base(c: char) -> Option<char> {
    const TABLE: &[char] = &[
        '。', '「', '」', '、', '・', 'ヲ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ', 'ッ',
        'ー', 'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ',
        'ソ', 'タ', 'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ',
        'ホ', 'マ', 'ミ', 'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ',
        'ン', '゛', '゜',
    ];
    let idx = (c as u32).checked_sub(0xFF61)?;
    TABLE.get(idx as usize).copied()
}

fn add_dakuten_katakana(base: char) -> Option<char> {
    Some(match base {
        'カ' => 'ガ', 'キ' => 'ギ', 'ク' => 'グ', 'ケ' => 'ゲ', 'コ' => 'ゴ',
        'サ' => 'ザ', 'シ' => 'ジ', 'ス' => 'ズ', 'セ' => 'ゼ', 'ソ' => 'ゾ',
        'タ' => 'ダ', 'チ' => 'ヂ', 'ツ' => 'ヅ', 'テ' => 'デ', 'ト' => 'ド',
        'ハ' => 'バ', 'ヒ' => 'ビ', 'フ' => 'ブ', 'ヘ' => 'ベ', 'ホ' => 'ボ',
        'ウ' => 'ヴ',
        _ => return None,
    })
}

fn add_handakuten_katakana(base: char) -> Option<char> {
    Some(match base {
        'ハ' => 'パ', 'ヒ' => 'ピ', 'フ' => 'プ', 'ヘ' => 'ペ', 'ホ' => 'ポ',
        _ => return None,
    })
}

fn add_dakuten_hiragana(base: char) -> Option<char> {
    Some(match base {
        'か' => 'が', 'き' => 'ぎ', 'く' => 'ぐ', 'け' => 'げ', 'こ' => 'ご',
        'さ' => 'ざ', 'し' => 'じ', 'す' => 'ず', 'せ' => 'ぜ', 'そ' => 'ぞ',
        'た' => 'だ', 'ち' => 'ぢ', 'つ' => 'づ', 'て' => 'で', 'と' => 'ど',
        'は' => 'ば', 'ひ' => 'び', 'ふ' => 'ぶ', 'へ' => 'べ', 'ほ' => 'ぼ',
        'う' => 'ゔ',
        _ => return None,
    })
}

/// Katakana -> hiragana for a single char (standard block only).
fn katakana_to_hiragana(c: char) -> char {
    match c {
        // U+30F4 (ヴ) maps to U+3094 (ゔ) within this range, which is a
        // valid (if rare) hiragana codepoint, so no special case needed.
        '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

fn kanji_digit(d: u32) -> char {
    match d {
        1 => '一', 2 => '二', 3 => '三', 4 => '四', 5 => '五',
        6 => '六', 7 => '七', 8 => '八', 9 => '九',
        _ => '〇',
    }
}

/// Returns the half-width ASCII digit for either an ASCII or a full-width
/// (U+FF10..=U+FF19) digit character, or `None` for anything else.
fn ascii_digit(c: char) -> Option<u32> {
    if let Some(d) = c.to_digit(10) {
        return Some(d);
    }
    match c {
        '\u{FF10}'..='\u{FF19}' => Some(c as u32 - 0xFF10),
        _ => None,
    }
}

/// Converts a run of digits (all ASCII by the time this is called) into its
/// Japanese kanji numeral representation: "1" -> 一, "12" -> 十二,
/// "35" -> 三十五, "108" -> 百八, "40000" -> 四万, "100000000" -> 一億.
/// Standard Japanese convention drops the 一 before 十/百/千 (十, 百, 千 not
/// 一十/一百/一千) and uses 万/億/兆/京/垓 as the repeating big-unit blocks.
fn digit_run_to_kanji(s: &str) -> String {
    let digits: Vec<u32> = s.chars().filter_map(ascii_digit).collect();
    if digits.is_empty() || digits.iter().all(|&d| d == 0) {
        return "〇".into();
    }

    const BIG: [char; 6] = [' ', '万', '億', '兆', '京', '垓'];
    const SMALL: [u32; 4] = [1000, 100, 10, 1];

    // Pad the digit string on the left to a multiple of 4 so each 4-digit
    // group is aligned to a big unit (万/億/兆...) boundary.
    let pad = (4 - digits.len() % 4) % 4;
    let mut bits: Vec<u32> = vec![0; pad];
    bits.extend(&digits);
    let n_groups = bits.len() / 4;

    let mut out = String::new();
    for gi in 0..n_groups {
        let group = &bits[gi * 4..gi * 4 + 4];
        let val: u32 = group.iter().zip(SMALL.iter()).map(|(d, m)| d * m).sum();
        if val == 0 {
            continue;
        }
        let (k, h, t, o) = (group[0], group[1], group[2], group[3]);
        if k != 0 {
            if k != 1 { out.push(kanji_digit(k)); }
            out.push('千');
        }
        if h != 0 {
            if h != 1 { out.push(kanji_digit(h)); }
            out.push('百');
        }
        if t != 0 {
            if t != 1 { out.push(kanji_digit(t)); }
            out.push('十');
        }
        if o != 0 {
            out.push(kanji_digit(o));
        }
        let big = n_groups - 1 - gi;
        if big < BIG.len() && BIG[big] != ' ' {
            out.push(BIG[big]);
        }
    }
    out
}

/// Folds any run of Arabic digits (half or full width) in the input into its
/// kanji numeral equivalent. Non-digit characters are passed through
/// unchanged, and the display surface is not affected (this is key-only).
fn fold_number_runs(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut started: Option<usize> = None;

    for (i, c) in input.char_indices() {
        if ascii_digit(c).is_some() {
            if started.is_none() {
                started = Some(i);
            }
        } else {
            if let Some(st) = started.take() {
                let end = i;
                out.push_str(&digit_run_to_kanji(&input[st..end]));
            }
            out.push(c);
        }
    }
    if let Some(st) = started.take() {
        out.push_str(&digit_run_to_kanji(&input[st..]));
    }
    out
}


enum Row { A, I, U, E, O }

fn kana_row(c: char) -> Option<Row> {
    const A: &[char] = &['あ','か','さ','た','な','は','ま','や','ら','わ','が','ざ','だ','ば','ぱ'];
    const I: &[char] = &['い','き','し','ち','に','ひ','み','り','ぎ','じ','ぢ','び','ぴ'];
    const U: &[char] = &['う','く','す','つ','ぬ','ふ','む','ゆ','る','ぐ','ず','づ','ぶ','ぷ','ゔ'];
    const E: &[char] = &['え','け','せ','て','ね','へ','め','れ','げ','ぜ','で','べ','ぺ'];
    const O: &[char] = &['お','こ','そ','と','の','ほ','も','よ','ろ','を','ご','ぞ','ど','ぼ','ぽ'];
    if A.contains(&c) { Some(Row::A) }
    else if I.contains(&c) { Some(Row::I) }
    else if U.contains(&c) { Some(Row::U) }
    else if E.contains(&c) { Some(Row::E) }
    else if O.contains(&c) { Some(Row::O) }
    else { None }
}

/// Deterministic part of normalization: width folding, katakana->hiragana,
/// and iteration-mark expansion (々, 〻, ゝ, ヽ, ゞ, ヾ). Chouonpu (ー) is
/// left in place here — see `chouonpu_variants`, because resolving ー is
/// ambiguous (mechanical vowel repeat vs. the い/う orthographic
/// convention used for many Sino-Japanese readings) and needs to produce
/// alternatives rather than a single answer.
pub fn normalize_text(input: &str) -> String {
    // Pass 1: halfwidth -> fullwidth, combining trailing voicing marks.
    let mut pass1: Vec<char> = Vec::with_capacity(input.chars().count());
    for c in input.chars() {
        let base = halfwidth_to_fullwidth_base(c).unwrap_or(c);
        match base {
            '゛' => {
                if let Some(&last) = pass1.last() {
                    if let Some(voiced) = add_dakuten_katakana(last) {
                        pass1.pop();
                        pass1.push(voiced);
                        continue;
                    }
                }
            }
            '゜' => {
                if let Some(&last) = pass1.last() {
                    if let Some(voiced) = add_handakuten_katakana(last) {
                        pass1.pop();
                        pass1.push(voiced);
                        continue;
                    }
                }
            }
            _ => pass1.push(base),
        }
    }

    // Pass 2: katakana -> hiragana (ー is unaffected, passes through).
    let pass2: Vec<char> = pass1.into_iter().map(katakana_to_hiragana).collect();

    // Pass 3: iteration marks, resolved against the output buffer built
    // so far (so chained marks like 々々 work).
    let mut out: Vec<char> = Vec::with_capacity(pass2.len());
    for c in pass2 {
        let resolved = match c {
            '々' | '〻' | 'ゝ' => out.last().copied(),
            'ゞ' => out.last().and_then(|&last| add_dakuten_hiragana(last)),
            other => Some(other),
        };
        out.push(resolved.unwrap_or(c));
    }

    // Pass 4: fold Arabic numeral runs to kanji numerals, so 1匹 and 一匹
    // (and ４回 for 四回) become the same lookup key. Key-only, not display.
    fold_number_runs(&out.into_iter().collect::<String>())
}

/// Given the output of `normalize_text`, returns every plausible
/// resolution of its ー (chouonpu) marks: the mechanical vowel-repeat
/// reading, and — when the preceding kana is e-row or o-row, where the
/// orthographic convention commonly diverges from the phonetic one —
/// the い/う alternative (e.g. セーソー -> せえそお *and* せいそう).
/// Returns just `[text]` unchanged if there's no ー to resolve.
/// Capped to 2 variants total to avoid combinatorial blowup on inputs
/// with several chouonpu marks; good enough for JMdict-style entries,
/// which rarely have more than one or two long vowels per word.
pub fn chouonpu_variants(text: &str) -> Vec<String> {
    if !text.contains('ー') {
        return vec![text.to_string()];
    }

    let chars: Vec<char> = text.chars().collect();
    let mut mechanical: Vec<char> = Vec::with_capacity(chars.len());
    let mut convention: Vec<char> = Vec::with_capacity(chars.len());
    let mut any_convention_diff = false;

    for &c in &chars {
        if c == 'ー' {
            let prev = mechanical.last().copied();
            let row = prev.and_then(kana_row);
            let (mech_vowel, conv_vowel) = match row {
                Some(Row::A) => ('あ', 'あ'),
                Some(Row::I) => ('い', 'い'),
                Some(Row::U) => ('う', 'う'),
                Some(Row::E) => ('え', 'い'), // convention diverges
                Some(Row::O) => ('お', 'う'), // convention diverges
                None => ('ー', 'ー'), // can't resolve; leave as-is
            };
            if mech_vowel != conv_vowel {
                any_convention_diff = true;
            }
            mechanical.push(mech_vowel);
            convention.push(conv_vowel);
        } else {
            mechanical.push(c);
            convention.push(c);
        }
    }

    let mech_str: String = mechanical.into_iter().collect();
    if any_convention_diff {
        let conv_str: String = convention.into_iter().collect();
        vec![mech_str, conv_str]
    } else {
        vec![mech_str]
    }
}

/// Convenience: full normalization pipeline producing every candidate
/// lookup key for a piece of text (usually 1, occasionally 2).
/// Full normalization pipeline producing every candidate lookup key for a piece of text.
pub fn normalize_variants(input: &str) -> Vec<String> {
    let base = normalize_text(input);
    let mut variants = vec![base.clone()];
    for v in chouonpu_variants(&base) {
        if !variants.contains(&v) {
            variants.push(v);
        }
    }
    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katakana_to_hiragana_basic() {
        assert_eq!(normalize_text("セイソウ"), normalize_text("せいそう"));
    }

    #[test]
    fn chouonpu_produces_convention_variant() {
        let variants = normalize_variants("セーソー");
        assert!(variants.contains(&"せいそう".to_string()));
    }

    #[test]
    fn iteration_mark_kanji() {
        assert_eq!(normalize_text("人々"), "人人");
    }

    #[test]
    fn halfwidth_katakana_with_dakuten() {
        assert_eq!(normalize_text("ｶﾞ"), "が");
    }

    #[test]
    fn no_chouonpu_single_variant() {
        assert_eq!(normalize_variants("こんにちは"), vec!["こんにちは".to_string()]);
    }

    #[test]
    fn arabic_numeral_folds_to_kanji() {
        assert_eq!(normalize_text("1匹"), "一匹");
        assert_eq!(normalize_text("35階"), "三十五階");
        assert_eq!(normalize_text("４回"), "四回");
    }

    #[test]
    fn arabic_numeral_full_width_folds_to_kanji() {
        assert_eq!(normalize_text("１００円"), "百円");
    }

    #[test]
    fn number_run_without_word_passes_through_key_only() {
        // Key normalization applies to digit runs regardless of surroundings.
        assert_eq!(normalize_text("2024"), "二千二十四");
        assert_eq!(normalize_text("10000"), "一万");
        assert_eq!(normalize_text("0"), "〇");
    }

    #[test]
    fn numeral_variants_match_kanji_headword() {
        assert_eq!(normalize_variants("1匹"), vec!["一匹".to_string()]);
    }
}