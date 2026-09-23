mod deconjugate;
mod normalize;
mod discord_rpc;
mod settings;

use settings::{get_settings, save_settings, SettingsState};
use deconjugate::{Deconjugator, DeconjugatedForm};
use normalize::normalize_variants;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use vibrato::{Dictionary, Tokenizer};
use serde::Serialize;
use tauri_plugin_sql::{Migration, MigrationKind};
use zstd::Decoder;
use serde::Deserialize;
use std::collections::HashSet;
use discord_rpc::DiscordState;

#[derive(Serialize)]
struct TokenOut {
    surface: String,
    reading: String,
    pos: String,
    base_form: String,
}

struct TokenizerState(Mutex<Tokenizer>);

/// A single morphological token (vibrato/MeCab) with the fields needed for
/// lookups: character offsets, surface, dictionary base form, POS, and the
/// normalized (hiragana) reading MeCab assigned in context.
#[derive(Clone, serde::Serialize)]
struct MorphToken {
    start: usize,
    end: usize,
    surface: String,
    base_form: String,
    pos: String,
    reading: String,
}

/// Cache of sentence -> morphological tokens, so repeated lookups against the
/// same sentence (hover, cycle, scan) don't re-run the tokenizer.
struct MorphCacheState(Mutex<HashMap<String, Vec<MorphToken>>>);

#[derive(Deserialize, Serialize, Clone)]
struct DictEntry {
    id: u32,
    spellings: Vec<String>,
    readings: Vec<String>,
    definitions: Vec<String>,
    pos: Vec<String>,
    priority: Vec<String>,
}

struct DictionaryIndex {
    by_text: HashMap<String, Vec<Arc<DictEntry>>>,
    pub(crate) by_id: HashMap<u32, Arc<DictEntry>>,
    // Maps each bigram (and, for single-character text, each unigram) to
    // the set of entry ids that contain it somewhere in a spelling or
    // reading. Used to narrow "contains" searches to a small candidate
    // set instead of scanning every entry.
    by_bigram: HashMap<String, HashSet<u32>>,
}

fn bigrams(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 {
        // single-character text: index the character itself so short
        // queries/spellings are still reachable
        return vec![chars.iter().collect()];
    }
    chars.windows(2).map(|w| w.iter().collect()).collect()
}

impl DictionaryIndex {
    /// Built exclusively from JMdict entries. The morphological tokenizer
    /// (IPADIC/UniDic — whichever splitter is configured) never creates
    /// entries; it only informs segmentation (token boundaries, base forms,
    /// context readings). Every `MatchSpan.entries` id is therefore always a
    /// JMdict id present in `by_id`.
    fn build(entries: Vec<DictEntry>) -> Self {
        let mut by_text: HashMap<String, Vec<Arc<DictEntry>>> = HashMap::new();
        let mut by_id: HashMap<u32, Arc<DictEntry>> = HashMap::new();
        let mut by_bigram: HashMap<String, HashSet<u32>> = HashMap::new();

        for entry in entries {
            let entry = Arc::new(entry);

            for spelling in entry.spellings.iter().chain(entry.readings.iter()) {
                // Index every chouonpu variant (セーソー under both せえそお
                // and せいそう) so queries with or without ー both hit, in
                // either direction.
                for key in normalize::normalize_variants(spelling) {
                    by_text.entry(key.clone()).or_default().push(Arc::clone(&entry));

                    for gram in bigrams(&key) {
                        by_bigram.entry(gram).or_default().insert(entry.id);
                    }
                }
                // Also index katakana script forms raw (シャイ under シャイ,
                // not just しゃい): normalization folds katakana away, so an
                // exact-script match outranks normalized-only homophones at
                // lookup time (シャイ the loanword beats 謝意/社医 for
                // katakana シャイ). Hiragana/kanji raw forms are skipped —
                // their normalized keys already reach the same entries, and
                // re-ranking those would dethrone morphological winners
                // (しろ imperative -> する must beat 白).
                let has_katakana = spelling
                    .chars()
                    .any(|c| ('\u{30A0}'..='\u{30FF}').contains(&c));
                if has_katakana
                    && normalize::normalize_variants(spelling)
                        .iter()
                        .all(|k| k != spelling)
                {
                    by_text.entry(spelling.clone()).or_default().push(Arc::clone(&entry));
                }
            }

            by_id.insert(entry.id, Arc::clone(&entry));
        }

        Self { by_text, by_id, by_bigram }
    }
}

/// Group verbs that directly inflect a te/で-form within a constructions
/// phrase — て+いる (teiru), て+くる (inceptive), て+しまう (completion),
/// て+みる (attempt), て+おく (preparative), and the benefactive て+くれる/
/// あげる/もらう. These stay inside the suru-noun phrase (調査している ->
/// 調査), while an independent verb after て (徹底して伏せた -> 徹底 + 伏せた)
/// starts a fresh clause and does not.
const TE_AUX_VERBS: &[&str] = &[
    "いる", "くる", "いく", "おく", "しまう", "みる", "くれる", "あげる", "もらう",
];

/// Contracted てしまう/でしまう auxiliaries (～ちゃう/じゃう and their
/// inflections, base ちゃう/じゃう when MeCab knows them). They continue a
/// suru-verb chain exactly like てる does (遅刻しちゃう -> 遅刻,
/// 準備しちゃいな -> 準備).
const CONTRACTION_AUX_VERBS: &[&str] = &["ちゃう", "じゃう", "ちまう", "じまう"];

/// Inflected surfaces of the ～ちゃう/じゃう contractions, for when MeCab
/// mis-analyzes the piece as an unknown token (base "*") instead.
const CONTRACTION_SURFACES: &[&str] = &[
    "ちゃう", "ちゃっ", "ちゃい", "ちゃえ", "ちゃお", "ちゃわ", "ちゃいな", "ちゃいなさい",
    "じゃう", "じゃっ", "じゃい", "じゃえ", "じゃお", "じゃわ", "じゃいな", "じゃいなさい",
    "ちまう", "ちまっ", "ちまい", "ちまえ", "ちまお", "ちまわ",
    "じまう", "じまっ", "じまい", "じまえ", "じまお", "じまわ",
];

fn is_bound_only(entry: &DictEntry) -> bool {
    !entry.pos.is_empty()
        && entry.pos.iter().all(|p| p.eq_ignore_ascii_case("suffix") || p.eq_ignore_ascii_case("prefix"))
}

/// True for characters that should never take part in a looked-up span:
/// Japanese and ASCII punctuation plus whitespace. Excludes the katakana
/// chouonpu ー and the nakaguro ・, which are word-internal (コーヒー,
/// オブジェクト・指向). Used both to skip a cursor that lands on or after
/// leading punctuation (e.g. the ".." in "..苦労") and to trim trailing
/// punctuation from candidate spans.
fn is_punct_char(c: char) -> bool {
    matches!(
        c,
        '。' | '、' | '，' | '．' | '：' | '；' | '！' | '？' | '…' | '‥' | '〜' | '～'
            | '「' | '」' | '『' | '』' | '【' | '】' | '（' | '）' | '〔' | '〕'
            | '〈' | '〉' | '《' | '》' | '＝' | '＊' | '　'
            | ' ' | '\t' | '\n' | '\r'
            | ',' | '.' | ';' | ':' | '!' | '?' | '(' | ')' | '[' | ']' | '{' | '}'
            | '<' | '>' | '"' | '\'' | '`' | '~' | '^' | '*' | '-' | '_' | '+' | '='
            | '/' | '\\' | '|' | '@' | '#' | '$' | '%' | '&'
    )
}

fn priority_score(entry: &DictEntry) -> u16 {
    let base = entry.priority.iter().map(|t| match t.as_str() {
        "ichi1" => 950,
        "news1" => 900,
        "gai1"  => 850,
        "spec1" => 800, // common but not corpus-measured — treat as mid-tier by default
        "spec2" | "ichi2" | "news2" | "gai2" => 500,
        t if t.starts_with("nf") => {
            let n: u16 = t[2..].parse().unwrap_or(48);
            1000 - n * 10
        }
        _ => 0,
    }).max().unwrap_or(0);

    let is_particle = entry.pos.iter().any(|p| p.eq_ignore_ascii_case("particle"));
    if is_particle { base + 200 } else { base }
}

fn find_containing(query: &str, index: &DictionaryIndex, limit: usize) -> Vec<Arc<DictEntry>> {
    // Match any chouonpu variant of the query, so related entries work in
    // both directions (せいそう finds セーソー entries and vice versa).
    let variants = normalize::normalize_variants(query);
    if variants.iter().all(|v| v.is_empty()) {
        return Vec::new();
    }

    let mut candidate_ids: HashSet<u32> = HashSet::new();
    for normalized in &variants {
        if normalized.is_empty() {
            continue;
        }
        let grams = bigrams(normalized);

        // Intersect posting lists, starting from the smallest to minimize work.
        let mut posting_sets: Vec<&HashSet<u32>> = grams
            .iter()
            .filter_map(|g| index.by_bigram.get(g))
            .collect();

        if posting_sets.len() < grams.len() {
            // at least one bigram in the query doesn't exist anywhere in the
            // dictionary at all, so no entry can possibly contain the query
            continue;
        }

        posting_sets.sort_by_key(|s| s.len());

        let mut candidates: HashSet<u32> = posting_sets[0].clone();
        for set in &posting_sets[1..] {
            candidates.retain(|id| set.contains(id));
        }
        candidate_ids.extend(candidates);
    }

    let mut results: Vec<(Arc<DictEntry>, bool)> = Vec::new();
    for id in candidate_ids {
        if let Some(entry) = index.by_id.get(&id) {
            let forms: Vec<String> = entry.spellings.iter().chain(entry.readings.iter())
                .flat_map(|s| normalize::normalize_variants(s))
                .collect();
    
            let is_exact = variants.iter().any(|v| forms.iter().any(|f| f == v));
            let actually_contains = is_exact || variants.iter().any(|v| forms.iter().any(|f| f.contains(v)));
    
            if actually_contains {
                results.push((Arc::clone(entry), is_exact));
            }
        }
    }
    
    results.sort_by(|(a, a_exact), (b, b_exact)| {
        b_exact.cmp(a_exact)
            .then(priority_score(b).cmp(&priority_score(a)))
    });
    results.truncate(limit);
    
    results.into_iter().map(|(e, _)| e).collect()
}

struct DeconjRulesState(Deconjugator);

#[derive(serde::Serialize)]
struct MatchSpan {
    start: usize,
    end: usize,
    surface: String,
    entries: Vec<Arc<DictEntry>>,
    deconjugated_from: Option<String>,
    related_entries: Vec<Arc<DictEntry>>, // entries containing `surface`, excluding exact matches already in `entries`
}

// Character count (not morpheme count) a phrase match can span. This is
// purely a performance/sanity cap on how far the longest-match scan looks
// ahead from a given position — it is NOT a linguistic boundary. JL does
// not use POS tagging or any tokenizer to decide where a match is allowed
// to end; the dictionary (plus deconjugation) is the only thing that
// decides that. Whatever doesn't resolve to a real entry at a given
// length just falls through to a shorter candidate at the same position.
const MAX_CHARS_COMBINED: usize = 16;

/// Mirrors JL's actual interaction model: JL does not pre-segment or
/// pre-highlight a whole sentence. It resolves exactly one match, starting
/// at the exact character position the user is pointing at (mouse
/// position / cursor / click), by trying the longest candidate substring
/// first and shrinking one character at a time until something resolves
/// against the dictionary (literally or via deconjugation). Nothing is
/// computed for the rest of the text — if the guess is wrong, the user
/// just points one character over and a fresh lookup runs from there.
///
/// `skip` selects which successful match to return, counting from longest
/// (skip = 0) downward — e.g. if 今日は, 今日, and 今 are all separately
/// in the dictionary, skip=1 returns 今日 and skip=2 returns 今, letting
/// a shorter word that a longer match "swallows" still be reached from
/// the same starting character (JL/Yomitan expose this as a
/// cycle-to-shorter-candidate hotkey rather than making longest-match
/// smarter, since there's no general way to know which length the user
/// actually wants).
///
/// Returns `None` if `position` is out of bounds, or if `skip` asks for
/// more candidates than exist at this position (the caller should treat
/// that as "wrap back to skip = 0"). A position with no dictionary/
/// deconjugation match at all still returns `Some` at skip = 0, as a
/// one-character span with empty `entries`.
fn lookup_from_position(
    text: &str,
    mut position: usize,
    skip: usize,
    index: &DictionaryIndex,
    decon: &Deconjugator,
    tokens: &[MorphToken],
) -> Option<MatchSpan> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if position >= len {
        return None;
    }

    // Punctuation never participates in a span. If the cursor lands on or
    // after leading punctuation (e.g. the ".." in "..苦労", a comma, an
    // opening bracket), skip forward to the first content character so the
    // span matches only the actual word.
    while position < len && is_punct_char(chars[position]) {
        position += 1;
    }
    if position >= len {
        return None;
    }

    // The token at (or containing) the cursor gives the in-context reading,
    // used to order kanji homographs (前 -> まえ in 前にある, ぜん inside 午前).
    // The base form is only used when the cursor is at the very start of a
    // verb token, since that's when the whole token's conjugation is what the
    // user is looking at (e.g. the し of します -> する).
    let token_at_pos = tokens.iter().find(|t| position >= t.start && position < t.end);
    let context_reading = token_at_pos
        .and_then(|t| {
            if t.reading.is_empty() {
                None
            } else {
                Some(t.reading.as_str())
            }
        });
    let morph_base = tokens
        .iter()
        .find(|t| t.start == position)
        .and_then(|t| {
            // A lone っ tagged as a verb (ったく shredded as っ|たく) is a
            // fragment, never a verb stem: trusting its base (く) promotes
            // unrelated entries (ったく -> 九) ahead of the real reading
            // match (the ったく interjection).
            if t.pos == "動詞" && t.base_form != t.surface && t.surface != "っ" {
                Some(t.base_form.as_str())
            } else {
                None
            }
        });

    // Punctuation never forms a span.
    if let Some(t) = token_at_pos {
        if t.pos == "記号" {
            return None;
        }
    }

    // Function words (particles が/を/は/も/に/の/と/で, auxiliaries
    // ます/た/だ/ん/たい, conjunctions) are themselves dictionary entries
    // (が -> 蛾, ます -> 鱒) and so stay lookup-able — but only as their own
    // single token. They must never extend into the next word, which is what
    // produced があ, をして, もできる, はしません, にさせたい, もない,
    // はよ, のこと and なんだ -> 涙.
    let function_word = token_at_pos
        .map(|t| {
            matches!(t.pos.as_str(), "助詞" | "助動詞" | "接続詞")
                // たん (the past た merged with the explanatory ん) is
                // mis-tagged 名詞 by MeCab; it must stay a single token and
                // never absorb the copula tail into one span (寝てたんじゃなかった
                // -> たんじゃなかった -> 肝).
                || (t.pos == "名詞" && t.surface == "たん" && t.base_form == "たん")
        })
        .unwrap_or(false);

    // Candidate spans are token-aligned: sub-spans inside the token at the
    // cursor (so 今 can still be reached inside 今日 for the skip feature)
    // plus whole-token extensions across following tokens. Spans never end
    // mid-way through a later token, which is what produced があ / はよ /
    // のこ. Function words never extend beyond their own token.
    let mut ends: Vec<usize> = Vec::new();
    if let Some(t) = token_at_pos {
        for e in (position + 1)..=t.end.min(len) {
            ends.push(e);
        }
        // Fixed-expression completions across token splits: なんで ("why")
        // split as な|んで, だった as だっ|た, でした as でし|た, になると
        // as に|なる|と, そういえば as そう|いえ|ば (or そう|言え|ば),
        // なさい as な|さ|いね. The heads are either locked to a single
        // token (function words) or cut off by a verb guard (そう + いえ)
        // — but the merged forms are real entries, so allow exactly the
        // merged span; longest-first lookup falls back to the pieces when
        // it doesn't apply. Exact-match only (never prefixes): 相手なんだ
        // splits as な|ん|だ, where な+ん ("なん") must NOT merge. Tails may
        // span several tokens (いえ|ば), may match normalized surfaces
        // (言え|ば), and may complete mid-token (な|さ|いね for ゴメンなさい
        // の さい). Function-head pairs also run at later token boundaries
        // (ゴメン|な|さ|いね from a ゴメン cursor), so fixed expressions
        // stay reachable wherever the user points inside them.
        let mut push_completion = |head_tok: &MorphToken, sai_only: bool| {
            const COMPLETIONS: &[(&str, &str, bool)] = &[
                // (head, tail, function_head_only)
                ("な", "んで", true),
                ("だっ", "た", true),
                ("だっ", "たら", true),
                ("でし", "た", true),
                ("に", "なる", true),
                ("そう", "いえば", false),
                ("そう", "いや", false),
                ("な", "さい", true),
            ];
            let head_is_function = matches!(
                head_tok.pos.as_str(),
                "助詞" | "助動詞" | "接続詞"
            );
            for (head, tail, fn_only) in COMPLETIONS {
                if *head != head_tok.surface {
                    continue;
                }
                if sai_only && *tail != "さい" {
                    continue;
                }
                if *fn_only && !head_is_function {
                    continue;
                }
                // Normalized once per token: readings first (言えば matches
                // いえば — normalization never maps kanji to readings), with
                // normalized-surface fallback for unknown/empty readings.
                let tail_norm = normalize::normalize_text(tail);
                let mut end = head_tok.end;
                let mut acc = String::new();
                for tok in tokens.iter().filter(|tok| tok.start >= head_tok.end) {
                    if tok.start != end {
                        break;
                    }
                    // Length-safe: the per-token contribution is measured in
                    // the same space it is appended in.
                    let cur = if tok.reading.is_empty() {
                        normalize::normalize_text(&tok.surface)
                    } else {
                        tok.reading.clone()
                    };
                    let before = acc.chars().count();
                    acc.push_str(&cur);
                    end = tok.end;
                    if acc == tail_norm {
                        let mut nend = end.min(len);
                        if nend > position {
                            ends.push(nend);
                        }
                        // Absorb immediately-following particles (になると's
                        // と): function-word heads have no extension loop of
                        // their own, so the completion must carry the span
                        // through them. Longest-first falls back when the
                        // longer span doesn't resolve.
                        while let Some(p) = tokens
                            .iter()
                            .find(|tok| tok.start == nend && tok.pos == "助詞")
                        {
                            nend = p.end.min(len);
                            if nend > position {
                                ends.push(nend);
                            } else {
                                break;
                            }
                        }
                        break;
                    }
                    if tail_norm.starts_with(&acc) {
                        continue;
                    }
                    // Mid-token completion (ゴメン|な|さ|いね, tail さい):
                    // the tail completes inside the current token. Only when
                    // the token's two spaces agree in length (kana), so the
                    // end lands on the right character.
                    if acc.starts_with(&tail_norm)
                        && cur.chars().count() == tok.surface.chars().count()
                    {
                        let extra = tail_norm.chars().count() - before;
                        let nend = (tok.start + extra).min(len);
                        if nend > position {
                            ends.push(nend);
                        }
                    }
                    break;
                }
            }
        };
        if position == t.start {
            push_completion(t, false);
            // (sai_only=false: all pairs, original head-anchored semantics.)
        }
        // Function-head pairs at later boundaries (bounded window).
        // Restricted to the さい pair (ゴメン|な|さ|いね from a ゴメン
        // cursor): other pairs stay head-anchored, otherwise a completion
        // end attributed to the cursor's span could swallow an independent
        // neighbor word (から|だっ|た from a から cursor -> からだった,
        // losing the だった span).
        for t2 in tokens.iter().filter(|tok| {
            tok.start > position
                && tok.start <= position + MAX_CHARS_COMBINED
                && tok.surface == "な"
        }) {
            push_completion(t2, true);
        }
        // If the cursor is on an unknown token (empty reading — katakana slang
        // like マズ), a filler token (フィラー — often mis-analyzed tokens
        // like ともう being one token), or a prefix (接頭詞 like す in すぐ,
        // which only ever forms words together with what follows), allow the
        // span to continue character-by-character into the next token so a
        // real literal word can still form across the false boundary.
        if t.reading.is_empty() || t.pos == "フィラー" || t.pos == "接頭詞" {
            if let Some(next) = tokens.iter().find(|tok| tok.start > position) {
                for e in (t.end + 1)..=next.end.min(len) {
                    ends.push(e);
                }
            }
        }
        // Referential こ/そ/あ/ど: a pure-kana noun like もうこ (もう + こ)
        // merges the referential こ with the preceding word. When the leading
        // part is itself a dictionary word and the next token is the の that
        // heads この/その/あの/どの, withhold the full-token end at the token
        // start so もう wins over もうこ -> 蒙古 and この can form across the
        // token boundary (this is もうこの世界, not 蒙古の世界). The kana-only
        // and length guards keep ここ/そこ/あそこ/どこ themselves unsplittable.
        let referential_split = {
            let s = &t.surface;
            let n = s.chars().count();
            if n >= 3
                && t.pos == "名詞"
                && position == t.start
                && s == &t.reading
                && s
                    .chars()
                    .all(|c| matches!(c, 'ぁ'..='ん' | 'ァ'..='ン' | 'ー'))
                && matches!(s.chars().next_back(), Some('こ' | 'そ' | 'あ' | 'ど'))
            {
                let leading: String = s.chars().take(n - 1).collect();
                let next_is_no = tokens
                    .iter()
                    .find(|n| n.start == t.end)
                    .map_or(false, |n| n.pos == "助詞" && n.surface == "の");
                next_is_no
                    && normalize::normalize_variants(&leading)
                        .iter()
                        .any(|k| index.by_text.contains_key(k))
            } else {
                false
            }
        };
        if referential_split {
            ends.retain(|e| *e <= t.end - 1);
        }
        // Okurigana merge: MeCab sometimes shreds a word so its final kana
        // lands in the next token (悪いし -> 悪|いし), making the real word
        // unreachable because spans never end mid-token. If exactly one more
        // hiragana char completes a literal dictionary word, allow ending
        // there. Literal-only (never a deconjugated end), function words stay
        // locked, referential splits keep priority — so のこ/はよ-style
        // false positives can't form. Kana-led cursors never merge: okurigana
        // attaches to kanji stems, and kana words (さん, そう, この, ヘン,
        // マジ, ち) plus the next kana would form coincidental words
        // (さんい/三位, そうい/相違, このこ/海鼠子, へんな/ヘナ, マジか/間近).
        if !function_word && !referential_split {
            // Kana-led cursors never merge (checked first): okurigana
            // attaches to kanji stems.
            let cursor_starts_kanji = token_at_pos.map_or(false, |t| {
                t.surface.chars().next().map_or(false, |c| {
                    let cp = c as u32;
                    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
                })
            });
            if cursor_starts_kanji {
                if let Some(t) = token_at_pos {
                    if let Some(next) = tokens.iter().find(|tok| tok.start == t.end) {
                        let e = t.end + 1;
                        if e <= next.end.min(len) && e <= position + MAX_CHARS_COMBINED {
                            if let Some(c) = chars.get(t.end) {
                                if matches!(c, 'ぁ'..='ん') {
                                    let merged: String =
                                        chars[position..e].iter().collect();
                                    if normalize::normalize_variants(&merged)
                                        .iter()
                                        .any(|k| index.by_text.contains_key(k))
                                    {
                                        ends.push(e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !function_word {
            // When the cursor is on a verb, stop extension at the first noun —
            // a noun after a verb always starts a new phrase (e.g. 引いたそう
            // must not swallow そう so 引く can be found via 引いた).
            let cursor_is_verb = token_at_pos.map_or(false, |t| t.pos == "動詞");
            // Split compound verbs (着|くずした -> 着崩す, 着|崩した shape):
            // when the whole compound deconjugates to a dictionary entry
            // sharing kanji with the cursor token, it is one word, not a
            // phrase boundary. 今泣いてる -> 忌む shares nothing with 今,
            // so it still splits; resolution POS-validates later anyway.
            // Looks ahead through the next few tokens: the inflection that
            // makes the compound resolvable (くずした's した) may sit
            // beyond the boundary token itself (くず).
            let compound_shares_deconj_kanji = |end: usize| {
                let cursor_kanji: Vec<char> = token_at_pos
                    .map_or(String::new(), |t| t.surface.clone())
                    .chars()
                    .filter(|c| {
                        let cp = *c as u32;
                        (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
                    })
                    .collect();
                if cursor_kanji.is_empty() {
                    return false;
                }
                let reaches_entry = |e2: usize| {
                    let compound: String = chars[position..e2.min(len)].iter().collect();
                    decon.deconjugate(&compound).iter().any(|f| {
                        index
                            .by_text
                            .get(&normalize::normalize_text(&f.text))
                            .map_or(false, |es| {
                                es.iter().any(|e| {
                                    e.spellings.iter().any(|s| {
                                        cursor_kanji.iter().any(|k| s.contains(*k))
                                    })
                                })
                            })
                    })
                };
                if reaches_entry(end) {
                    return true;
                }
                let mut e2 = end;
                for _ in 0..3 {
                    match tokens.iter().find(|t| t.start == e2) {
                        Some(n) => e2 = n.end,
                        None => break,
                    }
                    if e2 > position + MAX_CHARS_COMBINED {
                        break;
                    }
                    if reaches_entry(e2) {
                        return true;
                    }
                }
                false
            };
            // Formal-noun こと + なる grammar construct (通うことになった):
            // こと|に|なった — the に trips the separator guard below and the
            // inflected compound is never literal-known, so the なる verb's
            // end is exempted from both verb guards and the span resolves via
            // normal deconjugation (なった -> なる) to ことになる.
            let koto_naru_end: Option<usize> = match token_at_pos {
                Some(t) if t.pos == "名詞" && (t.surface == "こと" || t.surface == "事") => {
                    let mut rest = tokens.iter().filter(|tok| tok.start >= t.end);
                    let verb_tok = match rest.next() {
                        Some(f) if f.pos == "助詞" && f.surface == "に" => rest.next(),
                        other => other,
                    };
                    verb_tok
                        .filter(|v| v.pos == "動詞" && v.base_form == "なる")
                        .map(|v| v.end)
                }
                _ => None,
            };
            let is_koto_naru = |end: usize| koto_naru_end == Some(end);
            // A noun span stops once a non-te-form particle (でも/は/と/に...)
            // is followed by a content verb: 風船でも割れる must split as 風船 /
            // でも / 割れる rather than deconjugating the whole string into
            // 諷する. The te-form て/で, the suru verb する, and te-auxiliaries
            // stay within the noun's own construction (調査している / 会議してる /
            // 解除された / 質問をしている). Symmetrically, a noun must never
            // absorb a following content verb with no particle between: 今泣いてる
            // splits as 今 / 泣いてる rather than deconjugating the whole kana
            // into 忌む. Either guard is skipped when the whole compound up to
            // the verb is itself a dictionary entry (気になる, できる限り).
            let mut crossed_separator = false;
            let mut in_te_aux_chain = false;
            // A negation + conditional particle opens a "must" construction
            // (なければ|いけない, ないと|いけない, なくては|...): the following
            // いく/いける/なる verb continues it instead of starting a phrase.
            // Without this, noun-start spans (宿題をしなければいけない) get cut
            // before いけない and the tail becomes its own span. Colloquial
            // negations (なきゃ/なくちゃ/ねば) open the chain directly.
            let mut in_must_chain = false;
            // A suru-noun cursor (調査, 会議, 解除, 交信, 消失, 完了) absorbs
            // する-inflections directly: せる/させる (causative), れる
            // (passive), できる (potential), てる (contraction).
            let cursor_suru_noun = token_at_pos.map_or(false, |t| {
                t.pos == "名詞"
                    && index
                        .by_text
                        .get(&normalize::normalize_text(&t.surface))
                        .map_or(false, |es| {
                            es.iter().any(|e| {
                                e.pos.iter().any(|p| {
                                    p.contains("takes the aux. verb suru")
                                        || p.contains("suru verb")
                                })
                            })
                        })
            });
            for tok in tokens.iter().filter(|tok| tok.start > position) {
                // Sentence-final う glued into a mis-split token
                // (帰りましょうかっ -> 帰り|ましょ|うかっ): allow ending
                // right after the う so the volitional still resolves.
                // Shorter than the token end, so longest-first only reaches
                // it when longer spans fail. (か is deliberately excluded:
                // マジ|かっ would otherwise resolve to マジか/間近 instead
                // of マジ.)
                if let Some(&c) = chars.get(tok.start) {
                    if c == 'う' && tok.start + 1 > position && tok.start + 1 < tok.end.min(len) {
                        ends.push(tok.start + 1);
                    }
                }
                // Trailing punctuation and unknown tokens (emphatic kana
                // like ぅぇぁ, stray ー, dot runs, etc. — MeCab tags them
                // base "*" with an empty reading) never extend a span; they
                // only produced 疲れるぅ -> つく and 苦労…… -> 繰る.
                // Unknown tokens (emphatic kana like ぅぇぁ, stray ー, dot
                // runs, etc. — MeCab tags them base "*" with an empty
                // reading) never extend a span; they only produced
                // 疲れるぅ -> つく and 苦労…… -> 繰る. One exception: when the
                // kana crossing INCLUDING the unknown token is itself a real
                // dictionary reading (くしゃっと tokenizes as くし + ゃっと),
                // the span continues so the word is found.
                if tok.pos == "記号" {
                    break;
                }
                // Listing/reason し (鍛えてないし -> 鍛えてない + し, 悪いし,
                // 行くし来るし): the 助詞-し never continues a span — it
                // always starts its own (し as verb stem (する/死ぬ…) or noun
                // (氏/師/市/4…) is a different POS and unaffected). ただし/
                // すし/もし/よし stay whole as single tokens.
                if tok.pos == "助詞" && tok.surface == "し" {
                    break;
                }
                // Split causative さ|せ (気を悪くさせて tokenized 悪く|さ|
                // せ|て, そうさせる as そう|さ|せ|る): せ continues the さ
                // (する-stem), so it never starts a new phrase. させる whole
                // after a する-stem or a ku-form adjective is the same
                // construction unsplit; a bare さ surface before せ/させる
                // is likewise always the causative split (さ as a sentence
                // particle never precedes せ/させる).
                let prev_tok = tokens.iter().filter(|t| t.end <= tok.start).last();
                let tok_is_split_cause = matches!(tok.base_form.as_str(), "せる" | "させる")
                    && prev_tok.map_or(false, |p| {
                        p.base_form == "する"
                            || (tok.base_form == "させる" && p.pos == "形容詞")
                            || p.surface == "さ"
                    });
                // Topic は after an adverb always starts a new phrase (そうは ->
                // そう + は): an adverb+は merge only ever resolves to
                // coincidental reading homophones (走破/争覇) — the real
                // adverb+は words (まずは, または) are single tokens. Other
                // particles (か/に/も) stay continuable so なぜか/どうか/
                // そうです keep resolving.
                if token_at_pos.map_or(false, |t| t.pos == "副詞")
                    && tok.pos == "助詞"
                    && tok.surface == "は"
                {
                    break;
                }
                // Adverbs never take continuations (そうほいほい -> そう +
                // ほいほい, not そうほ/相補; とても親切 -> とても + 親切;
                // よく書く -> よく + 書く): content words always start new
                // phrases after an adverb — only particles, auxiliaries,
                // adnominals, and completions continue the span. Three
                // exemptions: the explanatory ん (そうなんだ still reaches
                // だ); split-causative せ/させる (そうさせる still reaches
                // せ for the causative-shorten rule); fixed adverbial
                // compounds with noun continuations (もう一つ, もう一度)
                // and lexicalized adverb+する units (ことにする,
                // ちゃんとする) — longest-first falls back otherwise.
                if token_at_pos.map_or(false, |t| t.pos == "副詞")
                    && matches!(
                        tok.pos.as_str(),
                        "名詞" | "動詞" | "形容詞" | "副詞"
                    )
                    && !(tok.surface == "ん" && tok.pos == "名詞")
                    && !tok_is_split_cause
                {
                    let compound_known = (tok.pos == "名詞"
                        || (tok.pos == "動詞" && tok.base_form == "する"))
                        && {
                            let compound: String =
                                chars[position..tok.end].iter().collect();
                            normalize::normalize_variants(&compound)
                                .iter()
                                .any(|k| index.by_text.contains_key(k))
                        };
                    if !compound_known {
                        break;
                    }
                }
                if tok.base_form == "*" {
                    let unknown_compound: String = chars[position..tok.end].iter().collect();
                    // Compare normalized variants so katakana/number spellings
                    // (くしゃっと, １匹) still match their index keys.
                    let known = normalize::normalize_variants(&unknown_compound)
                        .iter()
                        .any(|k| index.by_text.contains_key(k));
                    if !known {
                        // Contraction pieces MeCab didn't recognize (し|ちゃう
                        // with ちゃう as unknown): the ～ちゃう/じゃう forms
                        // are always continuations, like てる, so the span
                        // extends through them. Longest-first lookup still
                        // falls back when nothing resolves.
                        let is_contraction =
                            CONTRACTION_SURFACES.contains(&tok.surface.as_str());
                        if !is_contraction {
                            // Adnominal な glued onto an unknown token
                            // (おおざっぱ tokenized as お|お|ざっぱな): if the
                            // compound minus the trailing な is a real word,
                            // allow ending there (sub-token end) as well as
                            // at the token end. The な itself never resolves,
                            // so longest-first falls through to the word.
                            let na_stripped = tok.surface.ends_with('な')
                                && tok.end > position + 1
                                && {
                                    let stripped: String =
                                        chars[position..tok.end - 1].iter().collect();
                                    normalize::normalize_variants(&stripped)
                                        .iter()
                                        .any(|k| index.by_text.contains_key(k))
                                };
                            if na_stripped {
                                let sub = (tok.end - 1).min(len);
                                if sub > position {
                                    ends.push(sub);
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
                if tok.pos == "助詞" && tok.surface != "て" && tok.surface != "で" {
                    crossed_separator = true;
                    in_te_aux_chain = false;
                }
                // The te-form て/で opens a grammaticalized auxiliary chain
                // (ている/てくる/てしまう/ていく...); the auxiliaries below
                // continue the same construction instead of starting a phrase.
                if tok.pos == "助詞" && (tok.surface == "て" || tok.surface == "で") {
                    in_te_aux_chain = true;
                }
                // Must-chain tracking: ば/と/は/では right after a negation
                // (ない/なけれ/なく…, base ない), ては/では after なく/ない,
                // or a colloquial negation token on its own.
                const MUST_NEG_READINGS: &[&str] = &["なきゃ", "なくちゃ", "なくっちゃ", "ねば"];
                let is_neg_token = |t: &MorphToken| {
                    t.base_form == "ない" || MUST_NEG_READINGS.contains(&t.reading.as_str())
                };
                if ["なきゃ", "なくちゃ", "なくっちゃ", "ねば"].contains(&tok.reading.as_str()) {
                    in_must_chain = true;
                }
                if tok.pos == "助詞"
                    && matches!(tok.surface.as_str(), "ば" | "と" | "は" | "では")
                {
                    let prev_tok = tokens.iter().filter(|t| t.end <= tok.start).last();
                    let prev_neg =
                        prev_tok.map_or(false, |p| is_neg_token(p));
                    let tewa_neg = tok.surface == "は"
                        && prev_tok.map_or(false, |p| p.surface == "て" || p.surface == "で")
                        && prev_tok
                            .and_then(|p| {
                                tokens.iter().filter(|t| t.end <= p.start).last()
                            })
                            .map_or(false, |p| is_neg_token(p));
                    if prev_neg || tewa_neg {
                        in_must_chain = true;
                    }
                }
                let tok_is_must_aux = tok.pos == "動詞"
                    && matches!(tok.base_form.as_str(), "いく" | "いける" | "なる");
                let tok_is_te_aux = tok.pos == "動詞"
                    && (TE_AUX_VERBS.contains(&tok.base_form.as_str())
                        || tok.base_form == "てる"
                        || tok.base_form == "れる"
                        || tok.base_form == "できる");
                // てる is a bound contraction (て+いる/おる) that never starts
                // a phrase, so it always extends.
                let tok_is_bound = tok.base_form == "てる";
                // Past-aux た + emphatic sokuon (よかったっ -> よかっ|たっ):
                // MeCab lemmatizes the merged たっ as 立つ, but it is the
                // past auxiliary plus emphasis — never a new word. Real
                // 立つ continuations (立っ|た) keep 立っ and た separate, so
                // this exact shape is unambiguous. Push both the た end
                // (before the っ) and the full end, like the たん arm.
                let tok_is_past_tsu =
                    tok.surface == "たっ" && tok.base_form == "たつ";
                if tok_is_past_tsu {
                    let sub_end = tok.end - 1;
                    if sub_end > position {
                        ends.push(sub_end);
                    }
                    let e = tok.end.min(len);
                    if e > position {
                        ends.push(e);
                    }
                    continue;
                }
                // Suru-noun cursors absorb their inflections directly.
                let tok_is_suru_infl = cursor_suru_noun
                    && matches!(
                        tok.base_form.as_str(),
                        "せる" | "させる" | "れる" | "できる"
                    )
                    // Contracted てしまう/でしまう (遅刻しちゃう,
                    // 準備しちゃいな): the contraction continues the suru
                    // verb like any other inflection.
                    || (cursor_suru_noun
                        && CONTRACTION_AUX_VERBS.contains(&tok.base_form.as_str()));
                // Adjective causative/passive (悪くさせる -> 悪い): the
                // ku-stem adjective continues into せる/させる/される
                // instead of starting a new phrase. Resolution is handled by
                // the adjective-causative supplemental rules.
                let tok_is_adj_cause = token_at_pos.map_or(false, |t| t.pos == "形容詞")
                    && matches!(
                        tok.base_form.as_str(),
                        "せる" | "させる" | "される"
                    );
                // Unknown-kanji cursor (淹れ tokenized as 淹|れ, MeCab clueless
                // about 淹): the following verb is almost certainly the
                // okurigana continuation, not a new phrase (今泣いてる's
                // split applies to known nouns). Longest-first lookup still
                // falls back when the merged span doesn't resolve.
                let cursor_unknown = token_at_pos.map_or(false, |t| {
                    t.reading.is_empty() || t.base_form == "*"
                });
                if !cursor_is_verb
                    && crossed_separator
                    && tok.pos == "動詞"
                    && tok.base_form != "する"
                    && !tok_is_bound
                    && !tok_is_past_tsu
                    && !(in_te_aux_chain && tok_is_te_aux)
                    && !(in_must_chain && tok_is_must_aux)
                    && !is_koto_naru(tok.end)
                    && !tok_is_split_cause
                {
                    break;
                }
                // Noun-start spans never absorb a following content verb
                // directly (今泣いてる -> 今 + 泣いてる): verbs that continue
                // a te-auxiliary chain, a must construction (宿題をしなければ
                // いけない), the suru verb, a suru-noun inflection,
                // the bound てる, or a real dictionary compound (気になる)
                // still extend.
                if !cursor_is_verb
                    && tok.pos == "動詞"
                    && tok.base_form != "する"
                    && !tok_is_bound
                    && !tok_is_past_tsu
                    && !tok_is_suru_infl
                    && !tok_is_adj_cause
                    && !tok_is_split_cause
                    && !(in_te_aux_chain && tok_is_te_aux)
                    && !(in_must_chain && tok_is_must_aux)
                    && !is_koto_naru(tok.end)
                    && !(cursor_unknown && token_at_pos.map_or(false, |t| t.start == position))
                {
                    let compound: String = chars[position..tok.end].iter().collect();
                    let compound_known = normalize::normalize_variants(&compound)
                        .iter()
                        .any(|k| index.by_text.contains_key(k));
                    if !compound_known && !compound_shares_deconj_kanji(tok.end) {
                        break;
                    }
                }
                if tok.pos == "動詞" && !tok_is_te_aux && tok.base_form != "する" {
                    in_te_aux_chain = false;
                }
                // A content word ends the must construction (the exempted
                // いく/いける/なる above already consumed it; conditionals
                // like なければきっと行く must still split).
                if matches!(tok.pos.as_str(), "動詞" | "名詞" | "形容詞" | "副詞") {
                    in_must_chain = false;
                }
                if cursor_is_verb && tok.pos == "名詞" {
                    // Volitional う/よう glued into the next token
                    // (帰りましょうかっ -> 帰り|ましょ|うかっ): the う
                    // continues a volitional construction (plain volitional
                    // after a verb stem, polite volitional ましょう,
                    // conjecture だろう), never a new word. The exact end
                    // comes from the う/か single-char rule below; here just
                    // don't break.
                    let prev_volitional_base = tokens
                        .iter()
                        .filter(|t| t.end <= tok.start)
                        .last()
                        .map_or(false, |p| {
                            p.pos == "動詞"
                                || matches!(
                                    p.surface.as_str(),
                                    "ましょ" | "でしょ" | "だろ"
                                )
                        });
                    if (tok.surface.starts_with('う') || tok.surface.starts_with("よう"))
                        && prev_volitional_base
                    {
                        let e = tok.end.min(len);
                        if e > position {
                            ends.push(e);
                        }
                        continue;
                    }
                    // ん followed by だ is the explanatory copula んだ (= のだ),
                    // not a real noun: したんだ must split as した + んだ,
                    // never resolve as one span even if the compound reads
                    // like an entry. The ん of してん (slurred している) is
                    // NOT followed by だ and stays attached.
                    if tok.surface == "ん" && tok.base_form == "ん" {
                        let next = tokens.iter().find(|n| n.start == tok.end);
                        let next_is_da = next
                            .map_or(false, |n| n.pos == "助動詞" && n.base_form == "だ");
                        if next_is_da {
                            break;
                        }
                        // んじゃ heads the prohibitive tail (買うんじゃない
                        // "don't", 買うんじゃなかった "shouldn't have"): ん
                        // attaches to the verb, so it must not act as a noun
                        // boundary here. Skip the dictionary compound check
                        // and let じゃ/なかっ/た extend the verb span so the
                        // standalone suffix conjugation is reachable.
                        let next_is_ja = next
                            .map_or(false, |n| n.pos == "助詞" && n.surface == "じゃ");
                        if next_is_ja {
                            let e = tok.end.min(len);
                            if e > position {
                                ends.push(e);
                            }
                            continue;
                        }
                    }
                    // たん is the past auxiliary た merged with the
                    // explanatory ん: the span ends one candidate right after
                    // the た (tok.end - 1) so 寝てた -> 寝る stays reachable
                    // via skip-cycling, but the full span keeps extending
                    // through the copula tail so 寝てたんじゃなかった resolves
                    // as one span to 寝る. The 肝 false positive is only
                    // reachable from lookups starting AT たん itself, which
                    // take a separate path and are unaffected.
                    if tok.surface == "たん" && tok.base_form == "たん" {
                        let sub_end = tok.end - 1;
                        if sub_end > position {
                            ends.push(sub_end);
                        }
                        let e = tok.end.min(len);
                        if e > position {
                            ends.push(e);
                        }
                        continue;
                    }
                    // なさそう-hearsay shredded as な|さ|そう (考えてなさそう):
                    // nominalizer さ + hearsay そう continue the
                    // construction instead of starting a new phrase.
                    if tok.surface == "さ" && tok.pos == "名詞" {
                        let next_is_sou = tokens
                            .iter()
                            .find(|n| n.start == tok.end)
                            .map_or(false, |n| n.surface == "そう");
                        if next_is_sou {
                            let e = tok.end.min(len);
                            if e > position {
                                ends.push(e);
                            }
                            continue;
                        }
                    }
                    if tok.surface == "そう" && tok.pos == "名詞" {
                        let prev_is_sa = tokens
                            .iter()
                            .filter(|t| t.end <= tok.start)
                            .last()
                            .map_or(false, |p| p.surface == "さ" && p.pos == "名詞");
                        if prev_is_sa {
                            let e = tok.end.min(len);
                            if e > position {
                                ends.push(e);
                            }
                            continue;
                        }
                    }
                    // できる限り / 出来る限り is a real dictionary phrase
                    // that must not be split — 引いたそう (no dict entry
                    // 引いたそう) still breaks at the noun そう, unless the
                    // compound deconjugates to a kanji-sharing entry (着くず
                    // した -> 着崩す) per above.
                    let compound: String = chars[position..tok.end].iter().collect();
                    let compound_known = normalize::normalize_variants(&compound)
                        .iter()
                        .any(|k| index.by_text.contains_key(k));
                    if !compound_known && !compound_shares_deconj_kanji(tok.end) {
                        break;
                    }
                }
                let e = tok.end.min(len);
                if e > position {
                    ends.push(e);
                }
            }
        }
    } else {
        for e in (position + 1)..=len {
            ends.push(e);
        }
    }
    ends.sort_unstable();
    ends.dedup();
    ends.retain(|e| *e <= position + MAX_CHARS_COMBINED);

    let mut found = 0usize;
    for &end in ends.iter().rev() {
        // Trim trailing punctuation so a candidate never ends in 記号/UNK
        // characters absorbed into the cursor token (e.g. 苦労。 when MeCab
        // merges them). This pairs with the forward-extension break above.
        let mut eff_end = end;
        while eff_end > position && is_punct_char(chars[eff_end - 1]) {
            eff_end -= 1;
        }
        if eff_end <= position {
            continue;
        }
        let candidate: String = chars[position..eff_end].iter().collect();
        if let Some((entries, deconj_info)) =
            lookup_candidate(&candidate, index, decon, context_reading, morph_base, tokens, position)
        {
            if found == skip {
                let mut eff_end = eff_end;
                let mut candidate = candidate;
                let mut entries = entries;
                let mut deconj_info = deconj_info;
                // Even-if stem preference (不快でも -> 不快, not 深い): the
                // win came from stripping でも/ても/っても, but the stem
                // alone is a dictionary word — resolve as the stem instead.
                // Te-form/negation stems (行って, 急がなくて) are never
                // literal, so real ても-constructions are unaffected; stems
                // ending in て/で/っ are skipped outright since they continue
                // the same word's conjugation (行っても -> 行く, 行っ is a
                // sokuon stem, never a complete word). Only the first
                // (shortest-tail) match applies: 行っても ends with both ても
                // and っても, and the っても stem (い = 胃) is coincidental.
                if deconj_info
                    .as_deref()
                    .map_or(false, |d| d.contains("even if"))
                {
                    for tail in ["でも", "ても", "っても"] {
                        if !candidate.ends_with(tail) {
                            continue;
                        }
                        let stem: String = candidate
                            .chars()
                            .take(candidate.chars().count() - tail.chars().count())
                            .collect();
                        if stem.ends_with('て')
                            || stem.ends_with('で')
                            || stem.ends_with('っ')
                        {
                            break;
                        }
                        let stem_hit = normalize::normalize_variants(&stem)
                            .iter()
                            .any(|k| index.by_text.contains_key(k));
                        if stem_hit {
                            if let Some((se, si)) = lookup_candidate(
                                &stem,
                                index,
                                decon,
                                context_reading,
                                morph_base,
                                tokens,
                                position,
                            ) {
                                eff_end = position + stem.chars().count();
                                candidate = stem;
                                entries = se;
                                deconj_info = si;
                            }
                        }
                        break;
                    }
                }
                // Causative-stem preference (そうさせる -> そう, not archaic
                // 奏する): the win came through a causative strip, but the
                // stem alone is a complete non-verb word — resolve as the
                // stem (the させる tail stays reachable on hover). Ku-stem
                // (悪くさせる) and verb-stem (食べさせる, 来させる) shapes
                // never trigger this: their stems aren't literal words, or
                // resolve to verbs.
                if deconj_info
                    .as_deref()
                    .map_or(false, |d| d.contains("causative"))
                {
                    const CAUSE_SUFFIXES: &[&str] = &[
                        "させて", "させた", "させない", "させます", "させよう",
                        "させろ", "させる", "させ", "せて", "せた", "せない",
                        "せます", "せよう", "せろ", "せる", "せ",
                    ];
                    for suffix in CAUSE_SUFFIXES {
                        if !candidate.ends_with(suffix) || candidate.len() <= suffix.len() {
                            continue;
                        }
                        let stem: String = candidate
                            .chars()
                            .take(candidate.chars().count() - suffix.chars().count())
                            .collect();
                        let stem_hit = normalize::normalize_variants(&stem)
                            .iter()
                            .any(|k| index.by_text.contains_key(k));
                        if stem_hit {
                            if let Some((se, si)) = lookup_candidate(
                                &stem,
                                index,
                                decon,
                                context_reading,
                                morph_base,
                                tokens,
                                position,
                            ) {
                                let stem_is_word = se.first().map_or(false, |e| {
                                    !e.pos.iter().any(|p| p.contains("verb"))
                                });
                                if stem_is_word {
                                    eff_end = position + stem.chars().count();
                                    candidate = stem;
                                    entries = se;
                                    deconj_info = si;
                                }
                            }
                        }
                        break;
                    }
                }
                // Obscure-literal preference (そこに -> そこ, not 底荷;
                // さんと -> さん, not 三都; ものは -> もの, not もの派):
                // the winner is a priority-less literal whose surface splits
                // at a token boundary into a stem that resolves to a common
                // word of a DIFFERENT entry. Same-identity extensions
                // (くせに -> 癖, ために -> 為, ところで -> 所, 残念ながら ->
                // 残念, 今日, 食べ物) stay whole, as do conjugations (literal
                // winners only) and standalone-な tails (owned by the
                // rentaikei rule below). Continuative particles
                // (ながら/たり/だり/がてら/つつ) never split either — they
                // inflect the verb rather than casing a noun.
                if deconj_info.is_none()
                    && priority_score(&entries[0]) == 0
                {
                    const CONTINUATIVE: &[&str] =
                        &["ながら", "たり", "だり", "がてら", "つつ"];
                    if let Some(last) = tokens
                        .iter()
                        .filter(|t| t.start > position && t.end == eff_end)
                        .last()
                    {
                        let na_owned = last.pos == "助動詞"
                            && last.base_form == "だ"
                            && last.surface == "な";
                        let continuative = last.pos == "助詞"
                            && CONTINUATIVE.iter().any(|s| *s == last.surface);
                        if !na_owned && last.pos == "助詞" && !continuative {
                            let stem: String =
                                chars[position..last.start].iter().collect();
                            let stem_hit = normalize::normalize_variants(&stem)
                                .iter()
                                .any(|k| index.by_text.contains_key(k));
                            if stem_hit {
                                if let Some((se, si)) = lookup_candidate(
                                    &stem,
                                    index,
                                    decon,
                                    context_reading,
                                    morph_base,
                                    tokens,
                                    position,
                                ) {
                                    let stem_common = se
                                        .iter()
                                        .any(|e| priority_score(e) != 0);
                                    let same_entry = se
                                        .iter()
                                        .any(|e| e.id == entries[0].id);
                                    if stem_common && !same_entry {
                                        eff_end = last.start;
                                        candidate = stem;
                                        entries = se;
                                        deconj_info = si;
                                    }
                                }
                            }
                        }
                    }
                }
                // Rentaikei-な preference (ヘンな噂 -> ヘン + な, not the
                // literal ヘンナ "henna" plant): the winner ends in a
                // standalone copula-rentaikei な, the stem is a na-adjective
                // (keiyodoshi — 変, 静か, 綺麗…), and the stem alone
                // resolves — then な is adnominal, not part of the word.
                // The な must be its own 助動詞-だ token, so ひな祭り,
                // さかな, こんな (single tokens) never trigger this; and
                // non-na-adjective stems (よう/様 in ような) keep the whole
                // span for the ranking boost below.
                if candidate.ends_with('な') && candidate.chars().count() > 1 {
                    let stem: String =
                        candidate.chars().take(candidate.chars().count() - 1).collect();
                    let stem_end = position + stem.chars().count();
                    let na_is_copula = tokens.iter().any(|t| {
                        t.pos == "助動詞" && t.base_form == "だ" && t.start == stem_end && t.end == eff_end
                    });
                    if na_is_copula {
                        if let Some((se, si)) = lookup_candidate(
                            &stem,
                            index,
                            decon,
                            context_reading,
                            morph_base,
                            tokens,
                            position,
                        ) {
                            // Only na-adjective stems shorten (変, 静か…):
                            // noun/auxiliary stems (よう/様, 本当) keep the
                            // whole span for the adnominal ranking boost.
                            let stem_is_na_adj = se.iter().any(|e| {
                                e.pos.iter().any(|p| p.contains("keiyodoshi"))
                            });
                            if !se.is_empty() && stem_is_na_adj {
                                eff_end = stem_end;
                                candidate = stem;
                                entries = se;
                                deconj_info = si;
                            }
                        }
                    }
                }
                // Colloquial ない + listing/reason し mis-tagged as one
                // conjunction token (ないし -> 乃至): in dialogue-heavy text
                // this is almost always negation + し, so prefer the ない
                // stem when it resolves to 無い. Formal 乃至 ("A to B" /
                // "A nor B") is preserved by position: a content word right
                // after (死刑乃至無期懲役) keeps the whole span, since 乃至
                // takes nominal conjuncts while reason-し trails off into
                // particles/punctuation/end. ただし/もし/すし/よし never
                // match (their stems lack 無い).
                if candidate == "ないし" {
                    let next_is_content = tokens
                        .iter()
                        .find(|t| t.start == eff_end)
                        .map_or(false, |t| {
                            matches!(t.pos.as_str(), "名詞" | "動詞" | "形容詞")
                        });
                    if !next_is_content {
                        if let Some((se, si)) = lookup_candidate(
                            "ない",
                            index,
                            decon,
                            context_reading,
                            morph_base,
                            tokens,
                            position,
                        ) {
                            if se.iter().any(|e| {
                                e.spellings.iter().any(|s| s == "無い")
                            }) {
                                eff_end = position + 2;
                                candidate = "ない".to_string();
                                entries = se;
                                deconj_info = si;
                            }
                        }
                    }
                }
                let exact_ids: HashSet<u32> = entries.iter().map(|e| e.id).collect();
                let related = find_containing(&candidate, index, 20)
                    .into_iter()
                    .filter(|e| !exact_ids.contains(&e.id))
                    .collect();

                return Some(MatchSpan {
                    start: position,
                    end: eff_end,
                    surface: candidate,
                    entries,
                    deconjugated_from: deconj_info,
                    related_entries: related, // NEW field
                });
            }
            found += 1;
        }
    }

    if skip == 0 {
        let surface: String = chars[position..position + 1].iter().collect();

        // ── NEW: related entries for the no-match fallback too ──
        let related = find_containing(&surface, index, 20);

        return Some(MatchSpan {
            start: position,
            end: position + 1,
            surface,
            entries: vec![],
            deconjugated_from: None,
            related_entries: related, // NEW field
        });
    }

    None
}



/// Tries every normalized variant of `candidate` (there can be more than
/// one due to chouonpu ambiguity — see normalize::chouonpu_variants)
/// against the dictionary index: literal match first, then deconjugation.
/// Deconjugation uses the JL/Nazeka engine, which records each resolved
/// form with the fewest proper rule steps per (text, word class), and the
/// resulting word class is validated against each entry's POS (JL's
/// GetValidDeconjugatedResults) so a coincidental conjugation can't surface
/// a wrong homograph.

/// How an entry was reached for a given surface. Used as a tie-breaker so a
/// direct spelling match outranks a homophone reached only through an
/// alternate spelling or a reading — e.g. 前(まえ) beats 先(さき) when both
/// match the surface 前, since 先 merely lists 前 as a secondary spelling.
/// Ordering here (lower = better) is the sort precedence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchKind {
    ExactScript, // entry lists the raw surface script itself (katakana
    // シャイ for シャイ) — outranks normalized-only homophones (謝意).
    PrimarySpelling, // normalized surface == entry's primary (first) spelling
    Spelling,        // normalized surface == some other spelling
    Morphological,   // reached via the tokenizer's base form (e.g. します -> する)
    Reading,         // normalized surface == a reading
    Deconjugated,    // reached by deconjugating a conjugated surface
}

fn match_kind(entry: &DictEntry, key: &str) -> MatchKind {
    // Compare against every chouonpu variant so セーソー entries still count
    // as spelling matches for a せいそう query and vice versa.
    let spellings: Vec<String> = entry
        .spellings
        .iter()
        .flat_map(|s| normalize::normalize_variants(s))
        .collect();
    let readings: Vec<String> = entry
        .readings
        .iter()
        .flat_map(|s| normalize::normalize_variants(s))
        .collect();

    // Primary means the key matches a variant of the first spelling; the
    // first spelling's own variants are checked, not just its raw form.
    let primary_variants = entry
        .spellings
        .first()
        .map(|s| normalize::normalize_variants(s))
        .unwrap_or_default();
    if primary_variants.iter().any(|s| s == key) {
        MatchKind::PrimarySpelling
    } else if spellings.iter().any(|s| s == key) {
        MatchKind::Spelling
    } else if readings.iter().any(|s| s == key) {
        MatchKind::Reading
    } else {
        // A literal match must have come from a spelling or reading, so this
        // arm only fires for deconjugation-reached entries in practice.
        MatchKind::Deconjugated
    }
}

/// Does any of this entry's readings match the reading the tokenizer assigned
/// to the surface in context (e.g. 前 read まえ in 前にある vs ぜん in 午前)?
fn reading_matches_context(entry: &DictEntry, context_reading: &str) -> bool {
    let ctx_variants = normalize::normalize_variants(context_reading);
    entry.readings.iter().any(|r| {
        let entry_variants = normalize::normalize_variants(r);
        ctx_variants
            .iter()
            .any(|c| entry_variants.iter().any(|e| e == c))
    })
}

/// Maps a JL/Nazeka deconjugation tag to the JMdict English POS labels an
/// entry must carry for the deconjugated result to be valid (JL's
/// GetValidDeconjugatedResults). "any" (tomoyo's supplementary rules) and
/// unknown tags are POS-unrestricted.
fn deconj_tag_to_dict_pos(tag: &str) -> &'static [&'static str] {
    match tag {
        "v1" => &["Ichidan verb"],
        "v1-s" => &["Ichidan verb - kureru special class"],
        "v4r" => &["Yodan verb with 'ru' ending (archaic)"],
        "v5aru" => &["Godan verb - -aru special class"],
        "v5b" => &["Godan verb with 'bu' ending"],
        "v5g" => &["Godan verb with 'gu' ending"],
        "v5k" => &["Godan verb with 'ku' ending"],
        "v5k-s" => &["Godan verb - Iku/Yuku special class"],
        "v5m" => &["Godan verb with 'mu' ending"],
        "v5n" => &["Godan verb with 'nu' ending"],
        "v5r" => &["Godan verb with 'ru' ending"],
        "v5r-i" => &["Godan verb with 'ru' ending (irregular verb)"],
        "v5s" => &["Godan verb with 'su' ending"],
        "v5t" => &["Godan verb with 'tsu' ending"],
        "v5u" => &["Godan verb with 'u' ending"],
        "v5u-s" => &["Godan verb with 'u' ending (special class)"],
        "vk" => &["Kuru verb - special class"],
        "vs-c" => &["su verb - precursor to the modern suru"],
        "vs-i" => &["suru verb - included"],
        "vs-s" => &["suru verb - special class"],
        "vz" => &["Ichidan verb - zuru verb (alternative form of -jiru verbs)"],
        "adj-i" => &["adjective (keiyoushi)"],
        "adj-ix" => &["'ku' adjective (archaic)", "'shiku' adjective (archaic)"],
        "cop" => &["copula"],
        _ => &[],
    }
}

fn deconj_tag_matches_entry(entry_pos: &[String], tag: &str) -> bool {
    let allowed = deconj_tag_to_dict_pos(tag);
    allowed.is_empty() || entry_pos.iter().any(|p| allowed.contains(&p.as_str()))
}

/// Verb word classes the deconjugation rules can claim. Deconjugation
/// results in one of these are only trusted when the span actually contains
/// a 動詞 token — otherwise a noun/na-adjective + な (好きな) deconjugates
/// through the imperative な rule into a coincidental verb (好く).
fn is_verb_class(tag: &str) -> bool {
    matches!(
        tag,
        "v1" | "v1-s"
            | "v4r"
            | "v5aru"
            | "v5b"
            | "v5g"
            | "v5k"
            | "v5k-s"
            | "v5m"
            | "v5n"
            | "v5r"
            | "v5r-i"
            | "v5s"
            | "v5t"
            | "v5u"
            | "v5u-s"
            | "vk"
            | "vs-c"
            | "vs-i"
            | "vs-s"
            | "vz"
    )
}

/// Jargon-y JL rule names for sound changes and auxiliary helpers that say
/// nothing about the surface form — excluded from combined labels so
/// してくれました reads "polite past" rather than "polite past +
/// statement/request + unstressed infinitive".
fn is_stem_jargon(detail: &str) -> bool {
    matches!(
        detail,
        // Auxiliary helpers and sound changes that say nothing about the
        // surface form.
        "statement/request"
            | "slurred"
            | "slurred negative"
            | "rough casual"
            | "ksb"
            | "contracted"
            // JL's parenthetical stem notes, which chain_description has
            // already stripped of their parentheses.
            | "masu stem"
            | "unstressed infinitive"
            | "stem"
            | "adverbial stem"
            | "izenkei"
            | "ka stem"
            | "ke stem"
            | "mizenkei"
            | "'a' stem"
    )
}

/// Shorter, plainer names for verbose JL rule details. Ambiguous られる
/// forms (passive / potential / honorific for ichidan+くる) stay labeled
/// "passive/potential" at a glance instead of collapsing to "potential" and
/// hiding the passive reading from learners.
fn curated_name(detail: &str) -> &str {
    match detail {
        "finish/completely/end up" => "ended up",
        "passive/potential/honorific" | "passive/potential" => "passive/potential",
        "toku (for now)" => "in advance (casual)",
        other => other,
    }
}

/// Names the surface's conjugation from a deconjugation rule chain by
/// combining every meaningful rule that applied, outermost first — e.g.
/// 住んでいた -> "past + teiru", 忘れてしまった -> "past + ended up".
/// Parenthetical stem notes ("(masu stem)") are skipped, except a leading
/// one like (te) in 飲んで, which is the surface conjugation itself.
fn combined_label(chain: &str) -> Option<String> {
    let build_parts = |skip_leading_te: bool| -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for (i, detail) in chain.split('→').enumerate() {
            if detail.is_empty() {
                continue;
            }
            if detail.starts_with('(') {
                if i == 0 && detail.len() >= 2 && detail.ends_with(')') {
                    parts.push(detail[1..detail.len() - 1].to_string());
                }
                continue;
            }
            // A leading て/で is just the carrier for a trailing conjugation
            // morpheme (できて -> "potential", 忘れておく -> "in advance").
            // It is only dropped when something meaningful follows: 寝てた's
            // chain "te→unstressed infinitive" has only jargon after the て,
            // so there the て itself names the surface.
            if skip_leading_te && i == 0 && (detail == "te" || detail == "de") {
                continue;
            }
            if is_stem_jargon(detail) {
                continue;
            }
            parts.push(curated_name(detail).to_string());
        }
        parts
    };
    let mut parts = build_parts(true);
    if parts.is_empty() {
        parts = build_parts(false);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" + "))
    }
}

fn lookup_candidate(
    candidate: &str,
    index: &DictionaryIndex,
    decon: &Deconjugator,
    context_reading: Option<&str>,
    morph_base: Option<&str>,
    tokens: &[MorphToken],
    position: usize,
) -> Option<(Vec<Arc<DictEntry>>, Option<String>)> {
    let variants = normalize_variants(candidate);
    let span_len = candidate.chars().count();

    // The candidate's reading (concatenated token readings) when it is
    // token-aligned. JL deconjugates the reading, not the surface, which is
    // what keeps 入ってこない -> 入る resolving to the はいる reading instead
    // of matching both homograph readings of 入る. The reading is only used
    // when a token starts exactly at the cursor: a span that begins
    // mid-token (e.g. the き of とき inside the single token とき) has no
    // honest reading — deconjugating its tokens produced きました -> ます
    // (増す) when the surface きました correctly resolves to くる. Sub-span
    // candidates therefore fall back to surface deconjugation.
    let span_reading: Option<String> = {
        let starts_at_position = tokens.iter().any(|t| t.start == position);
        if !starts_at_position {
            None
        } else {
            // If the token at the cursor is unknown (MeCab gives it no reading
            // because it isn't in its dictionary — katakana names/slang like
            // ジイ or マズ), the concatenated span reading silently drops that
            // prefix and deconjugates just the tail, producing dishonest
            // results (ジイさんじゃない -> さん, マズいんだ -> いぬ). Falls back to
            // surface deconjugation on the full normalized form instead, which
            // resolves じいさん (爺さん) / まずい (不味い) correctly.
            let cursor_tok = tokens.iter().find(|t| t.start == position);
            if cursor_tok.map_or(false, |t| t.reading.is_empty()) {
                None
            } else {
                let in_span: Vec<&MorphToken> = tokens
                    .iter()
                    .filter(|t| t.start >= position && t.end <= position + span_len)
                    .collect();
                // Any unknown token inside the span (empty reading) makes the
                // concatenated reading dishonest the same way: it silently
                // drops that stretch and deconjugates the rest (お|お|
                // ざっぱな reads as おお, which deconjugates to おおい/多い).
                // Fall back to surface deconjugation instead.
                if in_span.is_empty() || in_span.iter().any(|t| t.reading.is_empty()) {
                    None
                } else {
                    Some(in_span.iter().map(|t| t.reading.as_str()).collect())
                }
            }
        }
    };

    // Rule-based deconjugation forms for this surface. The token reading AND
    // the surface variants are both deconjugated: the tokenizer mis-reads or
    // mis-segments often enough (volitional 入ろう tagged as a noun reading
    // にゅうろう) that the reading alone dead-ends while the surface still
    // resolves (入ろう -> 入る via ろう). Results dedupe by (text, tag)
    // keeping fewest steps; ranking below (kind, context, kanji, steps,
    // priority) decides between the two paths, and mid-token spans still use
    // surface-only (their concatenated readings are dishonest, e.g.
    // きました -> ます).
    // `reading_forms` is kept separately: the morphological gate below
    // (via_deconj) must test the kana reading only. A kanji surface trivially
    // "reaches" its kanji base (惹かれております -> 惹く), which would let
    // coincidental orphans (惹く, 知らす) hijack the morphological slot that
    // belongs to rule-based ranking.
    let reading_forms: Vec<DeconjugatedForm> = match &span_reading {
        Some(reading) => decon.deconjugate(reading),
        None => Vec::new(),
    };
    let deconj_forms: Vec<DeconjugatedForm> = {
        let mut all: Vec<DeconjugatedForm> = Vec::new();
        all.extend(reading_forms.iter().cloned());
        // Mid-token spans have no honest reading (span_reading is None) and
        // fall back to surface deconjugation exactly as before; otherwise the
        // surface variants run in addition to the reading.
        for key in &variants {
            // Skip the surface when it is identical to the reading: the
            // engine already produced these forms.
            if Some(key) == span_reading.as_ref() {
                continue;
            }
            all.extend(decon.deconjugate(key));
        }
        let mut best: HashMap<(String, String), usize> = HashMap::new();
        let mut out: Vec<DeconjugatedForm> = Vec::new();
        for f in all {
            let key = (f.text.clone(), f.tag.clone());
            match best.get(&key) {
                Some(&steps) if steps <= f.proper_steps => {}
                _ => {
                    best.insert(key, f.proper_steps);
                    if let Some(existing) =
                        out.iter_mut().find(|e| e.text == f.text && e.tag == f.tag)
                    {
                        *existing = f;
                    } else {
                        out.push(f);
                    }
                }
            }
        }
        out
    };

    // (entry, chain_len, chain_description, kind, context_match)
    let mut candidates: Vec<(Arc<DictEntry>, usize, Option<String>, MatchKind, bool)> = Vec::new();
    let mut seen_ids: HashSet<u32> = HashSet::new();

    // Literal matches first — inserted with chain_len 0, so they win ties
    // against equal-priority deconjugated results, same as before.
    // Exact-script katakana matches go first of all (シャイ the loanword
    // beats 謝意/社医 for katakana シャイ): the index carries katakana raw
    // forms alongside normalized keys for exactly this lookup.
    if candidate
        .chars()
        .any(|c| ('\u{30A0}'..='\u{30FF}').contains(&c))
    {
        if let Some(entries) = index.by_text.get(candidate) {
            for e in entries {
                if e.spellings.iter().chain(e.readings.iter()).any(|s| s.as_str() == candidate)
                    && seen_ids.insert(e.id)
                {
                    let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                    candidates.push((Arc::clone(e), 0, None, MatchKind::ExactScript, ctx));
                }
            }
        }
    }
    // A whole-candidate っと/って is the quotative/emphatic と with
    // sokuon (ちりっと走った) — te-form って never tokenizes as one
    // piece (行って splits 行っ|て), so this only ever fires for the
    // particle. Look it up as と as well.
    let mut literal_keys: Vec<String> = variants.clone();
    if (candidate == "っと" || candidate == "って")
        && !literal_keys.iter().any(|k| k == "と")
    {
        literal_keys.push("と".to_string());
    }
    for key in &literal_keys {
        if let Some(entries) = index.by_text.get(key) {
            for e in entries {
                // Bare だった would otherwise match its inflection-expression
                // entry ("was; were") as a literal reading, outranking the
                // copula だ it deconjugates to. Inflections resolve to their
                // base (だった -> だ, "past") like every other conjugation.
                if key == "だった"
                    && e.pos.len() == 1
                    && e.pos[0] == "expressions (phrases, clauses, etc.)"
                {
                    continue;
                }
                if seen_ids.insert(e.id) {
                    let kind = match_kind(e, key);
                    let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                    candidates.push((Arc::clone(e), 0, None, kind, ctx));
                }
            }
        }
    }

    // Morphological matches — the tokenizer's base form for the verb at the
    // cursor. High confidence because MeCab resolved the actual conjugation
    // (します -> し -> する), so it outranks rule-based deconjugation, which can
    // guess a wrong coincidental form (しる/知る for します). Two gates keep it
    // from absorbing unrelated morphemes:
    //   1. the base form is already a deconjugation result of the kana reading
    //      for this surface (fixes kana きませんでした -> くる). Reading-only
    //      on purpose: a kanji surface trivially reaches its kanji base and
    //      would otherwise promote coincidental orphans (惹かれております ->
    //      惹く, 知らされなかった -> 知らす) past the ranked answer.
    //   2. or the candidate is the verb token followed only by
    //      auxiliary/particle tokens (fixes 食べられます -> 食べる).
    if let Some(base) = morph_base {
        let base_norm = normalize::normalize_text(base);
        let via_deconj = reading_forms.iter().any(|f| normalize::normalize_text(&f.text) == base_norm);
        let via_aux_tail = tokens
            .iter()
            .filter(|t| t.start >= position && t.start < position + span_len && t.start != position)
            .all(|t| matches!(t.pos.as_str(), "助動詞" | "助詞" | "記号" | "接頭辞" | "接尾辞"));
        if via_deconj || via_aux_tail {
            // Name the deconjugation (e.g. した -> "past") rather than the bare
            // base form in the tooltip. The deconjugation forms are kana while
            // the base form may be kanji (のむ vs 飲む), so they are matched by
            // the dictionary entry they resolve to, not by raw text.
            let base_ids: HashSet<u32> = index
                .by_text
                .get(&base_norm)
                .map(|es| es.iter().map(|e| e.id).collect())
                .unwrap_or_default();
            let resolves_to_base = |f: &DeconjugatedForm| {
                index
                    .by_text
                    .get(&normalize::normalize_text(&f.text))
                    .map_or(false, |es| es.iter().any(|e| base_ids.contains(&e.id)))
            };
            // The span reading, then the same reading with trailing particle
            // tokens dropped (から in 飲んでから) so the te-form is still
            // reachable for the label.
            let mut label_readings: Vec<String> = Vec::new();
            if let Some(reading) = &span_reading {
                label_readings.push(reading.clone());
                let mut in_span: Vec<&MorphToken> = tokens
                    .iter()
                    .filter(|t| t.start >= position && t.end <= position + span_len)
                    .collect();
                while let Some(&last) = in_span.last() {
                    if last.pos != "助詞" {
                        break;
                    }
                    // A 助詞 directly after a 動詞 token is the te-form て/で,
                    // part of the conjugation — not a particle to strip.
                    if in_span.iter().rev().skip(1).next().map_or(false, |t| t.pos == "動詞") {
                        break;
                    }
                    in_span.pop();
                }
                let stripped: String = in_span.iter().map(|t| t.reading.as_str()).collect();
                if stripped != *reading {
                    label_readings.push(stripped);
                }
            }
            let mut label: Option<String> = None;
            for reading in &label_readings {
                if let Some(chain) = decon
                    .deconjugate(reading)
                    .iter()
                    .find(|f| resolves_to_base(f))
                    .and_then(|f| f.rule_chain.as_deref())
                {
                    label = combined_label(chain);
                    if label.is_some() {
                        break;
                    }
                }
            }
            // てから / でから (te-form + the particle から) is a grammatical
            // rule meaning "after doing" — it outranks the plain te-form name.
            let te_kara = span_reading
                .as_deref()
                .map_or(false, |r| r.ends_with("てから") || r.ends_with("でから"));
            let label = if te_kara {
                "after doing".to_string()
            } else {
                // When no rule chain reaches the base form (はじめます ->
                // はじめる dead-ends at the unrecordable ます-stem), still
                // name the surface conjugation from the first rule applied
                // (polite).
                label
                    .or_else(|| label_readings.last().and_then(|r| decon.first_rule(r)))
                    .unwrap_or_else(|| base.to_string())
            };
            if let Some(entries) = index.by_text.get(&base_norm) {
                for e in entries {
                    if seen_ids.insert(e.id) {
                        let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                        candidates.push((Arc::clone(e), 1, Some(label.clone()), MatchKind::Morphological, ctx));
                    }
                }
            }
        }
    }

    // Suru-verb nouns — a 名詞 (調査) followed by the suru verb (する/し/して/
    // している) and then only auxiliaries/particles. The dictionary headword
    // is the noun itself ("調査" carries "noun or participle which takes the
    // aux. verb suru"), so 調査している must resolve to 調査 rather than a
    // coincidental rule-based deconjugation of the whole katakana reading —
    // ちょうさしている also yields ちょうする (弔する/徴する), which is wrong.
    // The noun's own POS validates the suru construction, mirroring how the
    // verb case above trusts the tokenizer's base form.
    let noun_pos_entries: Vec<Arc<DictEntry>> = {
        let noun_tok = tokens.iter().find(|t| t.start == position);
        match noun_tok {
            Some(noun) if noun.pos == "名詞" => {
                let noun_norm = normalize::normalize_text(&noun.surface);
                let entries = index.by_text.get(&noun_norm).cloned().unwrap_or_default();
                if entries.iter().any(|e| e.pos.iter().any(|p| p.contains("takes the aux. verb suru") || p.contains("suru verb"))) {
                    // One intervening を is part of the expression (質問をしている
                    // -> 質問, since 質問をする is the JMdict headword); any other
                    // particle still starts a new phrase.
                    let suru_start = {
                        let adjacent = tokens.iter().find(|t| t.start == noun.end);
                        match adjacent {
                            Some(w) if w.pos == "助詞" && w.surface == "を" => w.end,
                            _ => noun.end,
                        }
                    };
                    let suru = tokens.iter().find(|t| t.start == suru_start);
                    match suru {
                        Some(s) if s.base_form == "する" || s.base_form == "できる" => {
                            // The tail after the suru verb should only contain
                            // auxiliaries directly inflecting する (て-form
                            // markers, tense/negation auxiliaries, causative
                            // せる/させる, conditional ば).  Independent
                            // particles like と/に/は start new phrases and must
                            // not be absorbed — e.g. 除外するとして should resolve
                            // 除外 alone, not 除外するとして; 徹底して伏せた must
                            // not swallow the independent verb 伏せた. The
                            // te-form て/で may be followed by a grammaticalized
                            // te-auxiliary verb (ている/てくる/てしまう...) so
                            // 調査している still resolves to 調査.
                            let tail_ok = {
                                let mut in_aux_chain = false;
                                tokens
                                    .iter()
                                    .filter(|t| t.start > suru_start && t.start < position + span_len)
                                    .all(|t| {
                                        if matches!(t.pos.as_str(), "助動詞" | "記号" | "接頭辞" | "接尾辞") {
                                            true
                                        } else if t.pos == "助詞"
                                            && (t.surface == "て" || t.surface == "で")
                                        {
                                            in_aux_chain = true;
                                            true
                                        } else if t.pos == "助詞"
                                            && (t.surface == "たり" || t.surface == "だり")
                                        {
                                            // Parallel-action たり/だり (保存したり
                                            // していた): links suru clauses,
                                            // neutral to the te-chain.
                                            true
                                        } else if t.pos == "助詞" && t.surface == "ながら" {
                                            // Continuative ながら (案内しながら
                                            // 行く): links the suru clause,
                                            // neutral to the te-chain like たり.
                                            true
                                        } else if t.pos == "助詞" && t.surface == "ば" {
                                            // Conditional ば (させなければ):
                                            // directly continues the inflection.
                                            true
                                        } else if t.pos == "動詞"
                                            && in_aux_chain
                                            && TE_AUX_VERBS.contains(&t.base_form.as_str())
                                        {
                                            in_aux_chain = false;
                                            true
                                        } else if t.pos == "動詞" && t.base_form == "てる" {
                                            // Merged て+いる / て+おる contraction:
                                            // MeCab tokenizes 会議してる as 会議 +
                                            // し + てる (one verb token), so the
                                            // te-auxiliary chain has no separate
                                            // て to key on — treat てる itself as
                                            // the chain.
                                            true
                                        } else if t.pos == "動詞"
                                            && CONTRACTION_AUX_VERBS
                                                .contains(&t.base_form.as_str())
                                        {
                                            // Contracted てしまう/でしまう
                                            // (遅刻しちゃう, 準備しちゃいな):
                                            // directly continues the suru verb.
                                            true
                                        } else if t.pos == "助詞" && t.surface == "な" {
                                            // Casual imperative contraction
                                            // ～ちゃいな/じゃいな (準備しちゃいな):
                                            // the な belongs to the いなさい
                                            // contraction when it directly
                                            // follows the contraction verb.
                                            tokens.iter().any(|p| {
                                                p.end == t.start
                                                    && CONTRACTION_AUX_VERBS
                                                        .contains(&p.base_form.as_str())
                                            })
                                        } else if t.pos == "動詞" && t.base_form == "れる" {
                                            // Passive される (解除される -> された):
                                            // the れ token directly continues the
                                            // suru verb, so 解除された resolves to
                                            // 解除 rather than stopping at 解除さ.
                                            true
                                        } else if t.pos == "動詞" && t.base_form == "できる" {
                                            // Potential できる (交信できて,
                                            // 調査できる): the ability form
                                            // directly continues the suru verb.
                                            true
                                        } else if t.pos == "動詞"
                                            && (t.base_form == "せる" || t.base_form == "させる")
                                        {
                                            // Causative させる (消失させる,
                                            // 完了させなければ): directly continues
                                            // the suru verb.
                                            true
                                        } else if t.pos == "動詞" && t.base_form == "する" {
                                            // The suru verb itself continuing
                                            // mid-construction (保存したりして
                                            // いた: し|たり|し|て|いた). Content
                                            // verbs and particles still break
                                            // (徹底して伏せた, 除外するとして).
                                            true
                                        } else {
                                            false
                                        }
                                    })
                            };
                            if tail_ok { entries } else { Vec::new() }
                        }
                        _ => Vec::new(),
                    }
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    };
    if !noun_pos_entries.is_empty() {
        // Label the suru construction by deconjugating the reading from the
        // suru token onward (し+て+いる -> "teiru", し+た -> "past", ...),
        // excluding the leading noun token itself. たり/だり splits the
        // construction (保存したりしていた = し + たり + していた): the
        // parallel-action listing never sits at the deconjugated end, so the
        // whole string can't reach する and the label would collapse to bare
        // "suru". Instead the final suru segment is labeled normally and
        // "tari" appended (past + teiru + tari). Split on whole tokens, never
        // substrings, so coincidental たり inside longer readings can't split.
        let tail_readings: Vec<&str> = tokens
            .iter()
            .filter(|t| t.start >= position && t.end <= position + span_len)
            .skip_while(|t| {
                t.base_form != "する"
                    && t.base_form != "できる"
                    && t.base_form != "せる"
                    && t.base_form != "させる"
            })
            .map(|t| t.reading.as_str())
            .collect();
        let mut seg_starts = vec![0usize];
        for (i, r) in tail_readings.iter().enumerate() {
            if *r == "たり" || *r == "だり" {
                seg_starts.push(i + 1);
            }
        }
        let had_tari = seg_starts.len() > 1;
        let last_seg: String = tail_readings[*seg_starts.last().unwrap_or(&0)..].concat();
        let base_label = decon
            .deconjugate(&last_seg)
            .iter()
            .find(|f| normalize::normalize_text(&f.text) == "する")
            .and_then(|f| f.rule_chain.as_deref())
            .and_then(combined_label);
        let label = match (base_label, had_tari) {
            (Some(l), true) => format!("{l} + tari"),
            (Some(l), false) => l,
            (None, true) => "tari".to_string(),
            (None, false) => "suru".to_string(),
        };
        // Continuative ながら names itself when the tail label doesn't
        // already say so (案内しながら -> "suru + while").
        let had_nagara = tokens.iter().any(|t| {
            t.start >= position && t.end <= position + span_len && t.surface == "ながら"
        });
        let label = if had_nagara && !label.contains("while") {
            format!("{label} + while")
        } else {
            label
        };
        // Bare "suru" means no rule chain reached the verb (てくれる/
        // てもらう tails): name the grammaticalized tail auxiliaries
        // directly so 手助けをしてくれる reads "suru + do for someone"
        // instead of a bare "suru".
        let label = if label == "suru" {
            const TAIL_AUX_LABELS: &[(&str, &str)] = &[
                ("くれる", "do for someone"),
                ("あげる", "do for someone"),
                ("もらう", "get someone to do"),
                ("しまう", "ended up"),
                ("みる", "try"),
                ("おく", "in advance"),
            ];
            let mut parts: Vec<String> = Vec::new();
            for t in tokens.iter().filter(|t| {
                t.start >= position
                    && t.start < position + span_len
                    && t.pos == "動詞"
                    && t.base_form != "する"
            }) {
                if let Some((_, name)) =
                    TAIL_AUX_LABELS.iter().find(|(b, _)| *b == t.base_form)
                {
                    if parts.last().map_or(true, |last| last != name) {
                        parts.push(name.to_string());
                    }
                }
            }
            if parts.is_empty() {
                label
            } else {
                format!("suru + {}", parts.join(" + "))
            }
        } else {
            label
        };
        for e in noun_pos_entries {
            if seen_ids.insert(e.id) {
                let ctx = context_reading.map_or(false, |r| reading_matches_context(&e, r));
                candidates.push((Arc::clone(&e), 1, Some(label.clone()), MatchKind::Morphological, ctx));
            }
        }
    }

    // Compound verbs (V-masu-stem + auxiliary verb): 取り過ぎた is 取る +
    // すぎる ("too much"), not the noun 取り過ぎ. The front verb's dictionary
    // entry is the headword, mirroring the suru-noun path above.
    // NOTE: MeCab base forms may be kanji (過ぎる) while the table below is
    // kana-first, so matching tries every listed spelling.
    const COMPOUND_AUX_VERBS: &[(&[&str], &str)] = &[(&["すぎる", "過ぎる"], "too much")];
    fn compound_aux_desc(base_form: &str) -> Option<(&'static str, &'static str)> {
        COMPOUND_AUX_VERBS.iter().find_map(|(spellings, desc)| {
            spellings
                .contains(&base_form)
                .then_some((spellings[0], *desc))
        })
    }
    let compound_aux: Option<(String, String, String)> = tokens
        .iter()
        .find(|t| t.start == position)
        .and_then(|v1| {
            if v1.pos != "動詞" || v1.base_form == "*" || v1.base_form == v1.surface {
                return None;
            }
            let v2 = tokens.iter().find(|t| t.start == v1.end)?;
            if v2.pos != "動詞" {
                return None;
            }
            let (kana, desc) = compound_aux_desc(&v2.base_form)?;
            Some((
                normalize::normalize_text(&v1.base_form),
                kana.to_string(),
                desc.to_string(),
            ))
        });
    if let Some((head_norm, aux_kana, aux_desc)) = compound_aux {
        // After the auxiliary only tense/negation auxiliaries may follow
        // (過ぎた, 過ぎない), and the candidate must actually reach it.
        let aux_tok = tokens
            .iter()
            .find(|t| t.start > position && t.pos == "動詞");
        let tail_ok = aux_tok.map_or(false, |v2| {
            position + span_len >= v2.end
                && tokens
                    .iter()
                    .filter(|t| t.start > v2.start && t.start < position + span_len)
                    .all(|t| {
                        matches!(t.pos.as_str(), "助動詞" | "記号" | "接頭辞" | "接尾辞")
                    })
        });
        if tail_ok {
            if let Some(entries) = index.by_text.get(&head_norm) {
                // Label the auxiliary's inflection (過ぎた -> "past"),
                // outermost first, e.g. 取り過ぎた reads "past + too much".
                // Matched against the kana spelling: MeCab's base may be the
                // kanji form (過ぎる) while deconjugation yields kana (すぎる).
                let aux_reading: String = tokens
                    .iter()
                    .filter(|t| t.start >= position && t.end <= position + span_len)
                    .skip(1)
                    .map(|t| t.reading.as_str())
                    .collect();
                let aux_kana_norm = normalize::normalize_text(&aux_kana);
                let label = decon
                    .deconjugate(&aux_reading)
                    .iter()
                    .find(|f| normalize::normalize_text(&f.text) == aux_kana_norm)
                    .and_then(|f| f.rule_chain.as_deref())
                    .and_then(combined_label)
                    .map(|l| format!("{l} + {aux_desc}"))
                    .unwrap_or_else(|| aux_desc.clone());
                for e in entries {
                    if seen_ids.insert(e.id) {
                        let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                        candidates.push((Arc::clone(e), 1, Some(label.clone()), MatchKind::Morphological, ctx));
                    }
                }
            }
        }
    }

    // Rule-based deconjugation — fallback beneath morphology. Entries already
    // found via literal/morphological paths are skipped via seen_ids, so a
    // word never appears twice just because both paths resolved to it. Each
    // deconjugation result is also validated against the entry's POS (the
    // rule's word class must appear among the entry's parts of speech), so a
    // coincidental conjugation like しる -> 知る (v5r) never surfaces.
    let starts_at_position = tokens.iter().any(|t| t.start == position);
    let span_has_verb_token = tokens
        .iter()
        .any(|t| t.start >= position && t.start < position + span_len && t.pos == "動詞");
    // A span covering exactly one token from its start carries that token's
    // own analysis: MeCab mis-tags whole inflected words often enough
    // (volitional 入ろう as a noun) that deconjugating the whole single token
    // is legitimate evidence. Multi-token crossings (好き + な) stay gated.
    let single_token_span = tokens
        .iter()
        .any(|t| t.start == position && t.end >= position + span_len);
    for form in &deconj_forms {
        // Verb-class results need a real verb token backing them (see
        // is_verb_class) — otherwise 好きな (名詞+な) resolves to 好く via the
        // imperative な rule. Sub-span candidates (cursor mid-token, e.g. the
        // き of きませんでした) are exempt: their tokens aren't aligned with
        // the conjugation, so the deconjugation itself is the best evidence.
        if starts_at_position && !single_token_span && is_verb_class(&form.tag) && !span_has_verb_token {
            continue;
        }
        // Curt な-imperative (へんな -> ヘン + "casual polite imperative")
        // fires on any surface ending in な, mislabeling na-adjective
        // rentaikei (ヘンな噂, 静かな部屋) as a command. Only verb stems
        // take it — しな/食べな keep working; ヘンな/静かな fall back to
        // the stem. Segment-exact so the ちゃいな supplemental rules
        // ("contracted + casual imperative") are unaffected.
        let is_na_imperative = form.rule_chain.as_deref().map_or(false, |c| {
            c.split('→').any(|seg| seg == "casual polite imperative")
        });
        if is_na_imperative {
            let stem_is_verb = tokens
                .iter()
                .find(|t| t.start == position)
                .map_or(false, |t| t.pos == "動詞");
            if !stem_is_verb {
                continue;
            }
        }
        let key = normalize::normalize_text(&form.text);
        if let Some(entries) = index.by_text.get(&key) {
            let chain_desc = form.rule_chain.as_deref().and_then(combined_label);
            for e in entries {
                if deconj_tag_matches_entry(&e.pos, &form.tag) && seen_ids.insert(e.id) {
                    let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                    candidates.push((
                        Arc::clone(e),
                        form.proper_steps,
                        chain_desc.clone(),
                        MatchKind::Deconjugated,
                        ctx,
                    ));
                }
            }
        }
    }
    // Explanatory/copula tails attach to a verb stem in speech but have no
    // deconjugation rules of their own, so a whole span like
    // 寝てたんじゃなかった would otherwise fall back to the shorter 寝てた.
    // Strip the longest matching tails, deconjugate the stem normally, and
    // re-attach the tail descriptions to the label. Verb-start spans only:
    // noun+copula (学生だった, ジイさんじゃない) keeps its dedicated copula
    // path, and prohibitive んじゃない ("don't", one rule step) keeps winning
    // ties via the +2 step penalty here.
    const COPULA_TAILS: &[(&str, &str)] = &[
        ("んじゃなかった", "explanatory + negative + past"),
        ("んじゃないか", "explanatory + negative + question"),
        ("じゃないか", "negative + question"),
        ("じゃなかった", "negative + past"),
        ("んじゃない", "explanatory + negative"),
        ("じゃない", "negative"),
        ("んです", "explanatory + polite"),
        ("のです", "explanatory + polite"),
        ("んだった", "explanatory + past"),
        ("のだ", "explanatory"),
        ("のか", "question"),
        ("んだ", "explanatory"),
    ];
    let verb_start = tokens
        .iter()
        .find(|t| t.start == position)
        .map_or(false, |t| t.pos == "動詞");
    // Adjective stems take the same explanatory/copula tails in speech
    // (良いんじゃない "isn't it good", 寒いんだ): without this the full
    // span falls back to the bare adjective — or worse, a coincidental
    // literal of the stripped stem (良いんじゃない -> 余韻 via the
    // じゃない copula rule). Verb-class stem forms stay gated on a real
    // verb token below, so the prohibitive reading can't leak here.
    let adj_start = tokens
        .iter()
        .find(|t| t.start == position)
        .map_or(false, |t| t.pos == "形容詞");
    // Shared tail-stripping machinery for copula + conjecture tails:
    // strips matching tails, resolves the stem via deconjugation or
    // literally, and pushes labeled candidates.
    let mut strip_tails = |tails: &[(&str, &str)], tail_bases: &[String]| {
        // A trailing question か (本なんですか, だったか, 行くだろうか)
        // peels first so the tail tables still match; the か itself stays
        // in the span. Peel-only (never replaces the full base), so
        // unrelated spans are unaffected.
        let mut all_bases: Vec<&str> = tail_bases.iter().map(|s| s.as_str()).collect();
        for base in tail_bases {
            if let Some(stripped) = base.strip_suffix('か') {
                if !stripped.is_empty() && !all_bases.iter().any(|b| *b == stripped) {
                    all_bases.push(stripped);
                }
            }
        }
        for base in all_bases {
            let mut stem = base;
            let mut tail_labels: Vec<&str> = Vec::new();
            loop {
                let mut matched = false;
                for (tail, label) in tails {
                    if stem.len() > tail.len() && stem.ends_with(tail) {
                        stem = &stem[..stem.len() - tail.len()];
                        tail_labels.push(label);
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    break;
                }
            }
            if tail_labels.is_empty() || stem.is_empty() {
                continue;
            }
            // Inner tails name first (surface order): stem-label + tail parts.
            tail_labels.reverse();
            let tail_desc = tail_labels.join(" + ");
            // Stem via deconjugation (POS-validated like the main path)...
            for form in decon.deconjugate(stem) {
                if is_verb_class(&form.tag) && !span_has_verb_token {
                    continue;
                }
                let key = normalize::normalize_text(&form.text);
                if let Some(entries) = index.by_text.get(&key) {
                    let stem_desc = form.rule_chain.as_deref().and_then(combined_label);
                    let desc = match stem_desc {
                        Some(s) => Some(format!("{s} + {tail_desc}")),
                        None => Some(tail_desc.clone()),
                    };
                    for e in entries {
                        if deconj_tag_matches_entry(&e.pos, &form.tag) && seen_ids.insert(e.id) {
                            let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                            candidates.push((
                                Arc::clone(e),
                                form.proper_steps + 2,
                                desc.clone(),
                                MatchKind::Deconjugated,
                                ctx,
                            ));
                        }
                    }
                }
            }
            // ...or a literal dictionary stem. Katakana-exact stems
            // (シャイ in シャイなのか) resolve first as ExactScript so the
            // script-faithful entry outranks normalized-only homophones
            // (謝意/社医). An entry already pushed via the normalized
            // reading base gets its kind upgraded in place (seen_ids would
            // otherwise lock in the weaker Deconjugated kind, since the
            // reading base runs first).
            if stem.chars().any(|c| ('\u{30A0}'..='\u{30FF}').contains(&c)) {
                if let Some(entries) = index.by_text.get(stem) {
                    for e in entries {
                        if !e.spellings.iter().chain(e.readings.iter()).any(|s| s.as_str() == stem) {
                            continue;
                        }
                        seen_ids.insert(e.id);
                        match candidates.iter_mut().find(|(c, _, _, _, _)| c.id == e.id) {
                            Some(slot) => {
                                slot.3 = MatchKind::ExactScript;
                            }
                            None => {
                                let ctx = context_reading
                                    .map_or(false, |r| reading_matches_context(e, r));
                                candidates.push((
                                    Arc::clone(e),
                                    2,
                                    Some(tail_desc.clone()),
                                    MatchKind::ExactScript,
                                    ctx,
                                ));
                            }
                        }
                    }
                }
            }
            for key in normalize::normalize_variants(stem) {
                if let Some(entries) = index.by_text.get(&key) {
                    for e in entries {
                        if seen_ids.insert(e.id) {
                            let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                            candidates.push((
                                Arc::clone(e),
                                2,
                                Some(tail_desc.clone()),
                                MatchKind::Deconjugated,
                                ctx,
                            ));
                        }
                    }
                }
            }
        }
    };

    if (verb_start || adj_start) && starts_at_position && (span_has_verb_token || adj_start) {
        // Tails are matched against the hiragana span reading when aligned,
        // else the normalized surface variants (same fallback order as the
        // main deconjugation above).
        let mut tail_bases: Vec<String> = Vec::new();
        if let Some(reading) = &span_reading {
            tail_bases.push(reading.clone());
        }
        // Raw surface first-class: normalized variants fold katakana away,
        // but script-faithful stems (シャイ in シャイなのか) need their raw
        // form for exact-script matching downstream.
        tail_bases.push(candidate.to_string());
        tail_bases.extend(variants.iter().cloned());
        strip_tails(COPULA_TAILS, &tail_bases);
    }

    // Conjecture だろう/でしょう (何だろう -> 何だ + conjecture): attaches
    // to verbs, adjectives, and nouns alike, but JL's だろう rule only
    // rewrites a bare だろう, so longer spans never resolve it. Adverbs
    // keep their current behavior (そうだろう -> そう) so JL-covered forms
    // stay stable.
    const CONJECTURE_TAILS: &[(&str, &str)] = &[
        ("だろう", "conjecture"),
        ("でしょう", "conjecture (polite)"),
    ];
    let conj_stem_ok = tokens
        .iter()
        .find(|t| t.start == position)
        .map_or(false, |t| {
            matches!(t.pos.as_str(), "動詞" | "形容詞" | "名詞" | "代名詞")
        });
    if conj_stem_ok && starts_at_position {
        let mut tail_bases: Vec<String> = Vec::new();
        if let Some(reading) = &span_reading {
            tail_bases.push(reading.clone());
        }
        // Raw surface first-class: normalized variants fold katakana away,
        // but script-faithful stems (シャイ in シャイなのか) need their raw
        // form for exact-script matching downstream.
        tail_bases.push(candidate.to_string());
        tail_bases.extend(variants.iter().cloned());
        strip_tails(CONJECTURE_TAILS, &tail_bases);
    }

    // なんだ-family tails: なんです/なのか/なのです/んですか/なのですか/っけ
    // attach to any stem, so unlike copula tails they need no gate — the
    // suffixes themselves are unambiguous. Plain なんだ is gated to
    // adverbial stems (そう/こんな/何 + なんだ): after nouns it stays split
    // (相手なんだ -> 相手 + な + ん + だ by deliberate design). The stem
    // resolves via the same deconjugation/literal paths (and longest-first
    // falls back when it doesn't).
    const NANDA_TAILS: &[(&str, &str)] = &[
        ("なんです", "explanatory + polite"),
        ("なのか", "question"),
        ("なのです", "explanatory + polite"),
        ("んですか", "question + polite"),
        ("なのですか", "question + polite"),
        ("っけ", "recall"),
    ];
    const NANDA_ADVERBIAL_TAILS: &[(&str, &str)] = &[("なんだ", "explanatory")];
    if starts_at_position {
        let mut tail_bases: Vec<String> = Vec::new();
        if let Some(reading) = &span_reading {
            tail_bases.push(reading.clone());
        }
        // Raw surface first-class: normalized variants fold katakana away,
        // but script-faithful stems (シャイ in シャイなのか) need their raw
        // form for exact-script matching downstream.
        tail_bases.push(candidate.to_string());
        tail_bases.extend(variants.iter().cloned());
        strip_tails(NANDA_TAILS, &tail_bases);
        let adverbial_stem = tokens
            .iter()
            .find(|t| t.start == position)
            .map_or(false, |t| {
                // Note: MeCab's top-level POS only; 代名詞 lives under 名詞
                // and is deliberately excluded (noun stems stay split).
                matches!(t.pos.as_str(), "副詞" | "連体詞" | "感動詞")
            });
        if adverbial_stem {
            strip_tails(NANDA_ADVERBIAL_TAILS, &tail_bases);
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Sort: how the entry was reached (literal spelling > morphological
    // base form > reading > rule deconjugation) first, then whether its
    // reading matches the in-context reading. A direct kanji match against
    // the PRIMARY spelling always wins next: for 引かれた, 引く (primary 引く)
    // must outrank 惹かれる even though 惹かれる lists 引かれる as a
    // secondary spelling and deconjugates in fewer steps. Primary-only on
    // purpose: secondary spellings are shared across homographs precisely
    // when the words are confusable. Pure-kana surfaces have no kanji
    // evidence, so the most common word wins there (priority before steps).
    // Among kanji surfaces, entries with no dictionary priority at all
    // ("orphans" like the intermediate potential 食べられる or the causative
    // homophone 知らす) never beat a common word — they're coincidental
    // deconjugation results, so 食べられます -> 食べる and 知らされなかった
    // -> 知る keep winning by frequency. Among common words sharing (or
    // lacking) kanji, fewest deconjugation steps decides for kanji surfaces
    // (JL ranks MinDeconjugationProcessStepCount before frequency), which is
    // what gives 惹かれております -> 惹かれる over the higher-frequency
    // 光る/引く (惹かれる's primary shares 惹; neither rival primary does).
    // For kana surfaces priority decides first. Then whether the entry's
    // spelling matches the tokenizer's base form (行かせられなかった
    // -> 行く, where 行く and 生かす tie on steps and priority), then
    // non-bound entries.
    // Any surface kanji shared with the entry's primary spelling counts — for
    // 書けない, 書く (contains 書) must outrank 掛ける (homophone, unrelated).
    let surface_kanji: Vec<char> = candidate
        .chars()
        .filter(|c| {
            let cp = *c as u32;
            (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
        })
        .collect();
    let has_kanji = !surface_kanji.is_empty();
    // Kanji-orphan filter (霧がかかった: 霧 drops 切る/斬る/剪る): when the
    // surface has kanji and some candidate shares kanji with it, drop
    // reading/deconjugation-only candidates sharing none — they matched
    // through coincidental kana. Literal and morphological (tokenizer-
    // backed) candidates always stay, and kana surfaces skip this entirely.
    // Discoverability is preserved via related_entries.
    if has_kanji {
        let shares_kanji = |e: &Arc<DictEntry>| {
            e.spellings.iter().any(|s| {
                surface_kanji.iter().any(|k| s.contains(*k))
            })
        };
        if candidates.iter().any(|(e, _, _, _, _)| shares_kanji(e)) {
            candidates.retain(|(e, _, _, kind, _)| {
                matches!(
                    kind,
                    MatchKind::PrimarySpelling
                        | MatchKind::Spelling
                        | MatchKind::Morphological
                ) || shares_kanji(e)
            });
        }
    }
    // Adnominal-な spans (ような) prefer rentaishi entries (様な over
    // 酔う): the な is a standalone copula token, so the construction is
    // stem + adnominal particle, and dictionary adnominals outrank
    // coincidental verbs sharing the reading.
    let adnominal_na = {
        let nchars: Vec<char> = candidate.chars().collect();
        nchars.last() == Some(&'な')
            && tokens.iter().any(|t| {
                t.pos == "助動詞"
                    && t.base_form == "だ"
                    && t.end == position + nchars.len()
                    && t.start == position + nchars.len() - 1
            })
    };
    // Longest-prefix specificity: among same-kind ties, an entry whose
    // spelling or reading is a strict prefix of the candidate surface
    // covers more of what the user pointed at (気を悪くさせて ->
    // 気を悪くする beats bare 悪い, whose わるい isn't even a substring).
    // Match kind still decides first, so morphological answers (食べる for
    // 食べられます) and common-word orphans (知る over 知らす, neither a
    // prefix of しらされなかった) are unaffected.
    let cand_keys = normalize::normalize_variants(candidate);
    let entry_prefix = |e: &Arc<DictEntry>| {
        e.spellings
            .iter()
            .chain(e.readings.iter())
            .flat_map(|s| normalize::normalize_variants(s))
            .any(|f| {
                !f.is_empty() && cand_keys.iter().any(|c| c.starts_with(&f) && *c != f)
            })
    };
    candidates.sort_by(|a, b| {
        let a_prio = priority_score(&a.0);
        let b_prio = priority_score(&b.0);
        let a_orphan = a_prio == 0;
        let b_orphan = b_prio == 0;
        let primary_share = |e: &Arc<DictEntry>| {
            !surface_kanji.is_empty()
                && e.spellings
                    .first()
                    .map_or(false, |s| surface_kanji.iter().any(|k| s.contains(*k)))
        };
        let a_kanji_share = primary_share(&a.0);
        let b_kanji_share = primary_share(&b.0);
        let a_prefix = entry_prefix(&a.0);
        let b_prefix = entry_prefix(&b.0);
        let a_base_match = match morph_base {
            Some(base) => a.0.spellings.iter().any(|s| normalize::normalize_text(s) == base),
            None => false,
        };
        let b_base_match = match morph_base {
            Some(base) => b.0.spellings.iter().any(|s| normalize::normalize_text(s) == base),
            None => false,
        };
        let ord = a.3.cmp(&b.3)
            .then(b.4.cmp(&a.4)) // context-match: true first
            .then({
                // Demonstrative + rentaishi agreement (このこと -> 此の, not
                // 九): when the cursor token is an adnominal (連体詞), prefer
                // pre-noun-adjectival entries. Both readings match here, so
                // reading context alone cannot decide. The same boost applies
                // to adnominal-な spans (ような -> 様な, not 酔う).
                let rentai = tokens
                    .iter()
                    .find(|t| t.start == position)
                    .map_or(false, |t| t.pos == "連体詞");
                let ra = (rentai || adnominal_na)
                    && a.0.pos.iter().any(|p| p.contains("rentaishi"));
                let rb = (rentai || adnominal_na)
                    && b.0.pos.iter().any(|p| p.contains("rentaishi"));
                rb.cmp(&ra)
            })
            .then({
                // Interjection agreement (はいはい -> "yeah yeah", not 這い這い):
                // when the cursor token is an interjection (感動詞), prefer
                // interjection entries. Same-kind ties only — kind and reading
                // context still decide first.
                let kandoushi = tokens
                    .iter()
                    .find(|t| t.start == position)
                    .map_or(false, |t| t.pos == "感動詞");
                let ka = kandoushi
                    && a.0.pos.iter().any(|p| p.contains("interjection"));
                let kb = kandoushi
                    && b.0.pos.iter().any(|p| p.contains("interjection"));
                kb.cmp(&ka)
            });
        if has_kanji {
            ord.then(b_kanji_share.cmp(&a_kanji_share)) // kanji match: true first
                .then(b_prefix.cmp(&a_prefix)) // longest-prefix entry first
                .then(a_orphan.cmp(&b_orphan)) // common word first
                .then(a.1.cmp(&b.1)) // fewest deconj steps first
                .then(b_prio.cmp(&a_prio))
                .then(b_base_match.cmp(&a_base_match)) // morph-base spelling: true first
                .then(is_bound_only(&a.0).cmp(&is_bound_only(&b.0))) // false (not bound) sorts before true
        } else {
            // Pure-kana surface: no kanji evidence, most common word wins.
            ord.then(b_prefix.cmp(&a_prefix)) // longest-prefix entry first
                .then(a_orphan.cmp(&b_orphan)) // common word first
                .then(b_prio.cmp(&a_prio))
                .then(a.1.cmp(&b.1)) // fewest deconj steps first
                .then(b_base_match.cmp(&a_base_match)) // morph-base spelling: true first
                .then(is_bound_only(&a.0).cmp(&is_bound_only(&b.0)))
        }
    });

    let deconjugated_from = candidates[0].2.clone();
    let entries: Vec<Arc<DictEntry>> = candidates.into_iter().map(|(e, _, _, _, _)| e).collect();

    Some((entries, deconjugated_from))
}

struct DictState(DictionaryIndex);

fn tokenize_tokens(tokenizer_mutex: &Mutex<Tokenizer>, text: &str) -> Vec<MorphToken> {
    let tokenizer = tokenizer_mutex.lock().unwrap();
    let mut worker = tokenizer.new_worker();
    worker.reset_sentence(text);
    worker.tokenize();

    worker
        .token_iter()
        .map(|t| {
            let range = t.range_char();
            let feature = t.feature(); // comma-separated MeCab features
            let fields: Vec<&str> = feature.split(',').collect();
            MorphToken {
                start: range.start,
                end: range.end,
                surface: t.surface().to_string(),
                base_form: fields.get(6).map(|s| s.to_string()).unwrap_or_else(|| t.surface().to_string()),
                pos: fields.get(0).unwrap_or(&"").to_string(),
                // readings come out in katakana; normalize to hiragana so they
                // can be compared against dictionary readings.
                reading: normalize::normalize_text(fields.get(7).unwrap_or(&"")),
            }
        })
        .collect()
}

fn tokenize_cached(
    cache_state: &Mutex<HashMap<String, Vec<MorphToken>>>,
    tokenizer_state: &Mutex<Tokenizer>,
    text: &str,
) -> Vec<MorphToken> {
    if let Some(tokens) = cache_state.lock().unwrap().get(text) {
        return tokens.clone();
    }

    let tokens = tokenize_tokens(tokenizer_state, text);

    let mut cache = cache_state.lock().unwrap();
    if cache.len() > 200 {
        cache.clear();
    }
    cache.insert(text.to_string(), tokens.clone());
    tokens
}

#[tauri::command]
fn lookup_at_position(
    dict_state: tauri::State<DictState>,
    decon_state: tauri::State<DeconjRulesState>,
    morph_cache: tauri::State<MorphCacheState>,
    tokenizer_state: tauri::State<TokenizerState>,
    text: String,
    position: usize,
    skip: usize,
) -> Option<MatchSpan> {
    let tokens = tokenize_cached(&morph_cache.0, &tokenizer_state.0, &text);
    lookup_from_position(&text, position, skip, &dict_state.0, &decon_state.0, &tokens)
}

/// Morphological tokens for a whole sentence (with char offsets), for
/// frontend consumers that need grammar-aware segmentation.
#[tauri::command]
fn tokenize_sentence(
    morph_cache: tauri::State<MorphCacheState>,
    tokenizer_state: tauri::State<TokenizerState>,
    text: String,
) -> Vec<MorphToken> {
    tokenize_cached(&morph_cache.0, &tokenizer_state.0, &text)
}

/// Scans a whole sentence into dictionary/deconjugation spans in one IPC
/// round-trip, using the same longest-match resolution as hover but sharing a
/// single tokenization. Replaces the frontend's per-character lookup loop.
#[tauri::command]
fn scan_sentence(
    dict_state: tauri::State<DictState>,
    decon_state: tauri::State<DeconjRulesState>,
    morph_cache: tauri::State<MorphCacheState>,
    tokenizer_state: tauri::State<TokenizerState>,
    text: String,
) -> Vec<MatchSpan> {
    let tokens = tokenize_cached(&morph_cache.0, &tokenizer_state.0, &text);
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut pos = 0usize;
    while pos < chars.len() {
        if let Some(span) = lookup_from_position(&text, pos, 0, &dict_state.0, &decon_state.0, &tokens) {
            if !span.entries.is_empty() {
                let end = span.end;
                spans.push(span);
                pos = end.max(pos + 1);
                continue;
            }
        }
        // No useful span at this position (punctuation, or a no-match
        // placeholder) — jump to the end of the token covering `pos` so a
        // skipped function word never leaves a dangling mid-token cursor.
        let next = tokens
            .iter()
            .find(|t| t.start <= pos && pos < t.end)
            .or_else(|| tokens.iter().find(|t| t.start >= pos))
            .map(|t| t.end);
        pos = next.map(|e| e.max(pos + 1)).unwrap_or(pos + 1);
    }
    spans
}

#[tauri::command]
fn tokenize_text(state: tauri::State<TokenizerState>, text: String) -> Vec<TokenOut> {
    let tokenizer = state.0.lock().unwrap();
    let mut worker = tokenizer.new_worker();
    worker.reset_sentence(&text);
    worker.tokenize();

    worker
        .token_iter()
        .map(|t| {
            let feature = t.feature(); // comma-separated MeCab features
            let fields: Vec<&str> = feature.split(',').collect();
            TokenOut {
                surface: t.surface().to_string(),
                reading: fields.get(7).unwrap_or(&"").to_string(), // reading field position varies by dict
                pos: fields.get(0).unwrap_or(&"").to_string(),
                base_form: fields.get(6).unwrap_or(&t.surface()).to_string(),
            }
        })
        .collect()
}

#[tauri::command]
fn export_database(app: tauri::AppHandle, dest: String) -> Result<(), String> {
    let db_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("immersion.db");
    std::fs::copy(&db_path, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn import_database(app: tauri::AppHandle, source: String) -> Result<(), String> {
    let header = std::fs::read(&source).map_err(|e| e.to_string())?;
    if header.len() < 16 || &header[..16] != b"SQLite format 3\0" {
        return Err("Selected file is not a valid SQLite database".into());
    }

    let db_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("immersion.db");

    // Remove sidecar files left behind by a previous connection so the
    // fresh copy starts clean (especially after a crash).
    for suffix in ["-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }

    std::fs::copy(&source, &db_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![
        Migration {
            version: 1,
            description: "create_media_table",
            sql: include_str!("../migrations/0001_media.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "words_and_sentences",
            sql: include_str!("../migrations/0002_words_and_sentences.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "events_and_sessions",
            sql: include_str!("../migrations/0003_events_and_sessions.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 4,
            description: "sessions_last_updated",
            sql: include_str!("../migrations/0004_sessions_last_updated.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 5,
            description: "lookup_events_new",
            sql: include_str!("../migrations/0005_lookup_events_new.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 6,
            description: "word_status",
            sql: include_str!("../migrations/0006_word_status.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 7,
            description: "dismissed_unknown_words",
            sql: include_str!("../migrations/0007_dismissed_unknown_words.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 8,
            description: "only_media_tag",
            sql: include_str!("../migrations/0008_only_media_tag.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 9,
            description: "tag_rewrite",
            sql: include_str!("../migrations/0009_tag_rewrite.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 10,
            description: "reviews",
            sql: include_str!("../migrations/0010_reviews.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 11,
            description: "sentences_read_events",
            sql: include_str!("../migrations/0011_sentences_read_events.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 12,
            description: "vndb_id",
            sql: include_str!("../migrations/0012_vndb_id.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 13,
            description: "session_links",
            sql: include_str!("../migrations/0013_session_links.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 14,
            description: "words_image",
            sql: include_str!("../migrations/0014_words_image.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 15,
            description: "names",
            sql: include_str!("../migrations/0015_names.sql"),
            kind: MigrationKind::Up,
        },
    ];

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:immersion.db", migrations)
                .build(),
        )
        .manage(DiscordState::new())
        .manage(MorphCacheState(Mutex::new(HashMap::new())))
        .invoke_handler(tauri::generate_handler![
            discord_rpc::connect_discord,
            discord_rpc::update_discord_presence,
            discord_rpc::disconnect_discord,
            tokenize_text, tokenize_sentence, scan_sentence, lookup_at_position,
            get_settings, save_settings,
            export_database, import_database, restart_app,
        ])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            #[cfg(target_os = "windows")]
            window.set_decorations(true)?;

            #[cfg(target_os = "linux")]
            window.set_decorations(false)?;

            let main_window = app.get_webview_window("main").unwrap();
                let app_handle = app.handle().clone();
            
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if let Some(discord_state) = app_handle.try_state::<discord_rpc::DiscordState>() {
                            let _ = discord_state.disconnect();
                        }
                        
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        app_handle.exit(0);
                    }
                });

            // ── Tokenizer (Vibrato) — used by tokenize_text/tokenize_sentence
            // and (via the cached tokens + base-form/context-reading info)
            // by lookup_at_position and scan_sentence. lookup_at_position
            // still resolves spans from the dictionary index, but morphology
            // informs the reading and base-form candidates. ──
            let resource_path = app
                .path()
                .resolve("resources/ipadic-mecab.dic.zst", tauri::path::BaseDirectory::Resource)?;

            let reader = Decoder::new(std::fs::File::open(resource_path)?)?;
            let dict = Dictionary::read(reader)?;
            let tokenizer = Tokenizer::new(dict);
            app.manage(TokenizerState(Mutex::new(tokenizer)));

            // ── Dictionary index (JMdict) ──
            let jmdict_path = app
                .path()
                .resolve("resources/jmdict.json", tauri::path::BaseDirectory::Resource)?;

            let jmdict_json = std::fs::read_to_string(jmdict_path)?;
            let entries: Vec<DictEntry> = serde_json::from_str(&jmdict_json)?;
            let dictionary_index = DictionaryIndex::build(entries);
            app.manage(DictState(dictionary_index));
            app.manage(DeconjRulesState(Deconjugator::build(include_str!(
                "../resources/deconjugation_rules.json"
            ))));

            let initial_settings = settings::load_settings_from_disk(&app.handle());
            app.manage(SettingsState(Mutex::new(initial_settings)));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod lookup_tests {
    use super::*;
    use std::io::Read;
    use vibrato::{Dictionary, Tokenizer};
    use zstd::Decoder;

    pub(crate) struct Harness {
        pub(crate) index: DictionaryIndex,
        pub(crate) decon: Deconjugator,
        pub(crate) tokenizer: Tokenizer,
    }

    impl Harness {
        pub(crate) fn new() -> Self {
            let file = std::fs::File::open("resources/ipadic-mecab.dic.zst").unwrap();
            let mut reader = Decoder::new(file).unwrap();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).unwrap();
            let dict = Dictionary::read(&buf[..]).unwrap();
            let tokenizer = Tokenizer::new(dict);

            let jmdict_json = std::fs::read_to_string("resources/jmdict.json").unwrap();
            let entries: Vec<DictEntry> = serde_json::from_str(&jmdict_json).unwrap();
            let index = DictionaryIndex::build(entries);
            let decon = Deconjugator::build(include_str!("../resources/deconjugation_rules.json"));
            Self { index, decon, tokenizer }
        }

        pub(crate) fn tokens(&self, text: &str) -> Vec<MorphToken> {
            let mut worker = self.tokenizer.new_worker();
            worker.reset_sentence(text);
            worker.tokenize();
            worker
                .token_iter()
                .map(|t| {
                    let range = t.range_char();
                    let fields: Vec<&str> = t.feature().split(',').collect();
                    MorphToken {
                        start: range.start,
                        end: range.end,
                        surface: t.surface().to_string(),
                        base_form: fields.get(6).map(|s| s.to_string()).unwrap_or_else(|| t.surface().to_string()),
                        pos: fields.get(0).unwrap_or(&"").to_string(),
                        reading: normalize::normalize_text(fields.get(7).unwrap_or(&"")),
                    }
                })
                .collect()
        }

        fn lookup(&self, text: &str, position: usize) -> MatchSpan {
            let tokens = self.tokens(text);
            lookup_from_position(text, position, 0, &self.index, &self.decon, &tokens).unwrap()
        }
    }

    fn top_reading(span: &MatchSpan) -> String {
        span.entries[0].readings[0].clone()
    }

    #[test]
    fn adverb_topic_wa_splits() {
        let h = Harness::new();
        // そうは must split into そう + は: the merged surface only matches
        // coincidental reading homophones (走破/争覇), while the real
        // adverb+は words (まずは/または) are single tokens.
        let span = h.lookup("そうは言われてもなぁ", 0);
        assert_eq!(span.surface, "そう");
        assert_eq!(top_reading(&span), "そう");
        let span = h.lookup("そうは言われてもなぁ", 2);
        assert_eq!(span.surface, "は");
        // Particles that form real words keep merging.
        let span = h.lookup("なぜか", 0);
        assert_eq!(span.surface, "なぜか");
        assert_eq!(top_reading(&span), "なぜか");
        let span = h.lookup("まずは", 0);
        assert_eq!(span.surface, "まずは");
        assert_eq!(top_reading(&span), "まずは");
    }

    #[test]
    fn shredded_sokuon_does_not_hijack_morphology() {
        let h = Harness::new();
        // ったく tokenizes as っ|たく; the lone っ is a fragment, never a
        // verb stem, so its く base must not promote 九 ahead of the ったく
        // interjection (with 全く still discoverable as related).
        let span = h.lookup("ったく......はいはい、穹にはかなわないよ。", 0);
        assert_eq!(span.surface, "ったく");
        assert_eq!(top_reading(&span), "ったく");
        assert!(
            span.related_entries.iter().any(|e| e.readings.iter().any(|r| r == "まったく")),
            "全く should stay discoverable, got {:?}",
            span.related_entries.iter().map(|e| &e.readings).collect::<Vec<_>>()
        );
    }

    #[test]
    fn interjection_cursor_prefers_interjection_entry() {
        let h = Harness::new();
        // はいはい tokenizes as interjection tokens; both 這い這い and the
        // "yeah yeah" interjection match identically (Reading, same context),
        // so the cursor's 感動詞 tag must break the tie — not JMdict order.
        let span = h.lookup("ったく......はいはい、穹にはかなわないよ。", 9);
        assert_eq!(span.surface, "はいはい");
        assert!(
            span.entries[0].pos.iter().any(|p| p.contains("interjection")),
            "first entry should be the yeah-yeah interjection, got {:?}",
            span.entries.iter().take(3).map(|e| (&e.spellings, &e.readings, &e.pos)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn shimasu_resolves_to_suru_not_shiru() {
        let h = Harness::new();
        let span = h.lookup("連絡します", 2); // position of し
        assert_eq!(span.surface, "します");
        assert_eq!(top_reading(&span), "する",
            "first entry should read する (suru), got {:?}",
            span.entries.iter().map(|e| &e.readings[0]).collect::<Vec<_>>());
    }

    #[test]
    fn mae_context_orders_mae_before_zen() {
        let h = Harness::new();
        // 前 in 前にある is read まえ; 前(ぜん) is earlier in the dict, so
        // only the context reading can put 前(まえ) first.
        let span = h.lookup("前にある", 0);
        assert_eq!(span.surface, "前");
        assert_eq!(top_reading(&span), "まえ");
    }

    #[test]
    fn overlong_candidate_absorbs_auxiliaries_to_verb() {
        let h = Harness::new();
        // JL parity: 待ってみてください deconjugates through the ～てみる/～
        // ください auxiliaries all the way to 待つ (JL does the same), so the
        // whole phrase resolves instead of stopping at 待って.
        let span = h.lookup("待ってみてください", 0);
        assert_eq!(span.surface, "待ってみてください");
        assert_eq!(top_reading(&span), "まつ");
    }

    #[test]
    fn jl_style_auxiliary_resolution() {
        let h = Harness::new();
        // ～てくる: 入ってこない -> 入る.
        let span = h.lookup("入ってこない", 0);
        assert_eq!(span.surface, "入ってこない");
        assert_eq!(top_reading(&span), "はいる");

        // causative-passive: 知らされなかった -> 知る.
        let span = h.lookup("知らされなかった", 0);
        assert_eq!(span.surface, "知らされなかった");
        assert_eq!(top_reading(&span), "しる");

        // ～てしまう: 食べてしまった -> 食べる.
        let span = h.lookup("食べてしまった", 0);
        assert_eq!(top_reading(&span), "たべる");

        // stacked causative + passive + negative + past: 行かせられなかった -> 行く.
        let span = h.lookup("行かせられなかった", 0);
        assert_eq!(top_reading(&span), "いく");

        // contracted ～ちゃう: 言っちゃった -> 言う.
        let span = h.lookup("言っちゃった", 0);
        assert_eq!(top_reading(&span), "いう");
    }

    #[test]
    fn supplementary_must_and_copula_rules() {
        let h = Harness::new();
        // なければならない (JL's rules don't reduce this to the verb):
        // 食べなければならない -> 食べる.
        let span = h.lookup("食べなければならない", 0);
        assert_eq!(span.surface, "食べなければならない");
        assert_eq!(top_reading(&span), "たべる");

        // noun + だった -> the noun: 学生だった -> 学生.
        let span = h.lookup("学生だった", 0);
        assert_eq!(span.surface, "学生だった");
        assert_eq!(top_reading(&span), "がくせい");
    }

    #[test]
    fn taberareru_resolves_to_taberu() {
        let h = Harness::new();
        let span = h.lookup("食べられます", 0);
        assert_eq!(span.surface, "食べられます");
        assert_eq!(top_reading(&span), "たべる");
    }

    #[test]
    fn kana_kuru_negative_masu_still_resolves() {
        let h = Harness::new();
        // きませんでした: no kana deconjugation rule, but the き token's base
        // form (くる) lets morphology resolve it instead of く/きる guesses.
        let span = h.lookup("きませんでした", 0);
        assert_eq!(span.surface, "きませんでした");
        assert_eq!(top_reading(&span), "くる");
    }

    #[test]
    fn plain_kana_lookup_unaffected() {
        let h = Harness::new();
        // No verb morphology involved; should resolve via reading priority.
        let span = h.lookup("こんにちは", 0);
        assert_eq!(span.surface, "こんにちは");
        assert!(span.entries[0].readings[0] == "こんにちは");
    }

    #[test]
    fn particles_match_as_single_tokens() {
        let h = Harness::new();
        // が/を/も/に/は/の are dictionary entries (蛾, を, 藻, 二, 歯, 野)
        // and stay lookup-able, but only as their own token — never merged
        // into the next word.
        let cases = [
            ("可能性があります", 3, "が"),
            ("考え方をしている", 3, "を"),
            ("意図もない", 2, "も"),
            ("にさせたい", 0, "に"),
            ("はしません", 0, "は"),
            ("のことは", 0, "の"),
            ("はよほどいい", 0, "は"),
        ];
        for (text, pos, want) in cases {
            let span = h.lookup(text, pos);
            assert_eq!(span.surface, want, "particle at {pos} in {text}");
        }
    }

    #[test]
    fn auxiliaries_match_as_single_tokens() {
        let h = Harness::new();
        // なんだ segments as な/ん/だ — な (助動詞) is limited to its own
        // token, so なんだ never forms and 涙 (reading なんだ) is unreachable.
        let span = h.lookup("そういう相手なんだ", 6); // な
        assert_eq!(span.surface, "な");
        assert_ne!(top_reading(&span), "なみだ");

        let span = h.lookup("そういう相手なんだ", 8); // だ
        assert_eq!(span.surface, "だ");

        // ます as its own token still resolves (鱒/増す).
        let span = h.lookup("可能性があります", 6); // ます
        assert_eq!(span.surface, "ます");
    }

    #[test]
    fn verbs_after_particles_resolve_correctly() {
        let h = Harness::new();
        // はしません -> する (not 走る via ichidan ません->る).
        let span = h.lookup("はしません", 1); // し
        assert_eq!(span.surface, "しません");
        assert_eq!(top_reading(&span), "する");

        // にさせたい -> する (not にる via causative recursion).
        let span = h.lookup("にさせたい", 1); // さ
        assert_eq!(span.surface, "させたい");
        assert_eq!(top_reading(&span), "する");

        // できる限り -> 出来る限り (real phrase), not 模する from もできる.
        let span = h.lookup("できる限りのことはします", 0); // できる
        assert_eq!(span.surface, "できる限り");
        assert_eq!(top_reading(&span), "できるかぎり");

        // 意図もない -> ない is the adjective 無い, not 盛る.
        let span = h.lookup("意図もない", 3); // ない
        assert_eq!(span.surface, "ない");
        assert_eq!(top_reading(&span), "ない");

        // はよほど -> よほど is the adverb 余程, not 早よ.
        let span = h.lookup("はよほどいい", 1); // よほど
        assert_eq!(span.surface, "よほど");
        assert_eq!(top_reading(&span), "よほど");

        for text in ["食べてたんじゃなかった", "いいじゃないか"] {
            let spans = scan(&h, text);
            println!("  SENT {text}");
            for (i, s) in spans.iter().enumerate() {
                let entries: Vec<String> = s
                    .entries
                    .iter()
                    .take(3)
                    .map(|e| format!("{}({})", e.spellings.join("/"), e.readings.join("/")))
                    .collect();
                println!(
                    "  span[{i}] {:?}..{:?} {:?} decon={:?} entries={:?}",
                    s.start, s.end, s.surface, s.deconjugated_from, entries
                );
            }
        }
        for text in ["食べてたんじゃなかった", "いいじゃないか"] {
            let spans = scan(&h, text);
            println!("  SENT {text}");
            for (i, s) in spans.iter().enumerate() {
                let entries: Vec<String> = s
                    .entries
                    .iter()
                    .take(3)
                    .map(|e| format!("{}({})", e.spellings.join("/"), e.readings.join("/")))
                    .collect();
                println!(
                    "  span[{i}] {:?}..{:?} {:?} decon={:?} entries={:?}",
                    s.start, s.end, s.surface, s.deconjugated_from, entries
                );
            }
        }
        let span = h.lookup("のことは", 1); // こと
        assert_eq!(span.surface, "こと");
        assert_eq!(top_reading(&span), "こと");
    }

    fn scan(h: &Harness, text: &str) -> Vec<MatchSpan> {
        let tokens = h.tokens(text);
        let chars: Vec<char> = text.chars().collect();
        let mut spans = Vec::new();
        let mut pos = 0usize;
        while pos < chars.len() {
            if let Some(span) = lookup_from_position(text, pos, 0, &h.index, &h.decon, &tokens) {
                if !span.entries.is_empty() {
                    let end = span.end;
                    spans.push(span);
                    pos = end.max(pos + 1);
                    continue;
                }
            }
            let next = tokens
                .iter()
                .find(|t| t.start <= pos && pos < t.end)
                .or_else(|| tokens.iter().find(|t| t.start >= pos))
                .map(|t| t.end);
            pos = next.map(|e| e.max(pos + 1)).unwrap_or(pos + 1);
        }
        spans
    }

    #[test]
    fn scan_respects_particle_boundaries() {
        let h = Harness::new();
        // A function-word token must never appear inside a longer span: every
        // span that starts at a 助詞/助動詞 is exactly that one token.
        for text in [
            "可能性があります",
            "考え方をしている",
            "意図もない",
            "にさせたい",
            "はしません",
            "のことは",
            "はよほどいい",
            "そういう相手なんだ",
            "できる限りのことはします",
        ] {
            let tokens = h.tokens(text);
            for span in scan(&h, text) {
                let token = tokens
                    .iter()
                    .find(|t| span.start >= t.start && span.start < t.end)
                    .unwrap();
                if matches!(token.pos.as_str(), "助詞" | "助動詞") {
                    assert_eq!(
                        span.surface.chars().count(),
                        1,
                        "function word {} merged into {:?} in {text}",
                        token.surface,
                        span.surface
                    );
                }
            }
        }
    }

    #[test]
    fn reading_deconj_beats_priority_for_homographs() {
        let h = Harness::new();
        // ひかれております is 惹かれる's te-form; JL ranks fewer deconj steps
        // before frequency, so 惹かれる (3 steps) beats 光る/引く (4 steps)
        // even though both are ichi1-frequency while 惹かれる is only spec1.
        let span = h.lookup("あなたに惹かれております", 4);
        assert_eq!(span.surface, "惹かれております");
        assert_eq!(top_reading(&span), "ひかれる");
    }

    #[test]
    fn mid_token_span_falls_back_to_surface_deconj() {
        let h = Harness::new();
        // The き of とき sits inside the single token とき (とく verb), so the
        // span きました has no honest token reading (its tokens read ました)
        // and must deconjugate the surface instead: きました -> 来る, not ます.
        let span = h.lookup("ときましたかぁ", 1);
        assert_eq!(span.surface, "きました");
        assert_eq!(top_reading(&span), "くる");
    }

    #[test]
    fn suru_contraction_beats_reading_homophones() {
        let h = Harness::new();
        // してん (する+て+ん) must resolve to する via the tokenizer's base
        // form, not to the してん reading-homophones 支店/視点.
        let span = h.lookup("顔してんの!", 1);
        assert_eq!(span.surface, "してん");
        assert_eq!(top_reading(&span), "する");
    }

    #[test]
    fn leading_and_trailing_punctuation_never_enters_span() {
        let h = Harness::new();
        // Leading dots: the cursor lands on "......" (an unknown 名詞 token),
        // but the span must snap forward to the actual word 苦労. 苦労 is a
        // suru-verb noun followed by している, so the whole economy resolves to
        // 苦労 (teiru) — exactly like 調査している -> 調査.
        let span = h.lookup("......苦労しているのも", 0);
        assert_eq!(span.surface, "苦労している");
        assert_eq!(span.start, 6);
        assert_eq!(top_reading(&span), "くろう");
        assert_eq!(span.deconjugated_from.as_deref(), Some("teiru"));

        // Trailing 記号: the span stops before 〜.
        let span = h.lookup("苦労〜", 0);
        assert_eq!(span.surface, "苦労");
        assert_eq!(top_reading(&span), "くろう");
    }

    #[test]
    fn unknown_tokens_do_not_extend_spans() {
        let h = Harness::new();
        // ぅ is an unknown token with an empty reading; without the break it
        // extended 疲れるぅ whose reading つかれる deconjugated to つく. The
        // span must stop at 疲れる so the literal spelling wins.
        let span = h.lookup("こっちの人格疲れるぅ〜......", 6);
        assert_eq!(span.surface, "疲れる");
        assert_eq!(top_reading(&span), "つかれる");
    }

    #[test]
    fn kire_resolves_to_kireru_in_context() {
        let h = Harness::new();
        // キレてる -> 切れる (the キレる slang), with 切る as the secondary
        // candidate rather than a coincidental deconj homophone outranking it.
        let span = h.lookup("キレてる", 0);
        assert_eq!(span.surface, "キレてる");
        assert_eq!(top_reading(&span), "きれる");
    }

    #[test]
    fn na_adjective_na_stays_with_the_noun() {
        let h = Harness::new();
        // The imperative な rule deconjugates 好きな -> 好く (すく), but a
        // verb-class result with no verb token in the span is coincidental.
        let span = h.lookup("好きな", 0);
        assert_eq!(span.surface, "好き");
        assert_eq!(top_reading(&span), "すき");
        let span = h.lookup("真面目な", 0);
        assert_eq!(span.surface, "真面目");
        assert_eq!(top_reading(&span), "まじめ");
    }

    #[test]
    fn deconj_labels_name_the_surface_conjugation() {
        let h = Harness::new();
        for (text, expected) in [
            ("した", "past"),
            ("高かった", "past"),
            ("面白くない", "negative"),
            ("呼んでいる", "teiru"),
            ("しなければならない", "must"),
            ("来てください", "polite request"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn nai_to_ikemasen_detects_must() {
        let h = Harness::new();
        for (text, base) in [
            ("洗わないといけません", "あらう"),
            ("洗わなければなりません", "あらう"),
            ("食べないといけない", "たべる"),
            ("食べなくてはいけません", "たべる"),
            ("しないといけません", "する"),
            ("しなければいけない", "する"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some("must"),
                "{text} should be labeled must, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn toire_sentence_spans() {
        let h = Harness::new();
        let spans = scan(&h, "トイレを使ってから、手を洗わないといけません。");
        let mut pairs: Vec<(String, String)> = spans
            .iter()
            .map(|s| (s.surface.clone(), s.deconjugated_from.clone().unwrap_or_default()))
            .collect();
        pairs.retain(|(s, _)| !s.is_empty());
        assert_eq!(
            pairs,
            vec![
                ("トイレ".to_string(), String::new()),
                ("を".to_string(), String::new()),
                ("使ってから".to_string(), "after doing".to_string()),
                ("手".to_string(), String::new()),
                ("を".to_string(), String::new()),
                ("洗わないといけません".to_string(), "must".to_string()),
            ]
        );
    }

    #[test]
    fn morph_labels_name_conjugations_for_kanji_bases() {
        let h = Harness::new();
        // The label comes from the deconjugation of the span reading, matched
        // to the base by dictionary entry (のむ == 飲む), never the bare base
        // form. から is a trailing particle, so 飲んでから still reaches the
        // te-form; 始めます dead-ends at the unrecordable ます-stem but is
        // still named by the first rule applied.
        for (text, base, expected) in [
            ("飲んで", "のむ", "te"),
            ("飲んでから", "のむ", "after doing"),
            ("食べてから", "たべる", "after doing"),
            ("飲んでいる", "のむ", "teiru"),
            ("食べています", "たべる", "polite + teiru"),
            ("始めます", "はじめる", "polite"),
            ("始めませんでした", "はじめる", "polite past negative"),
            ("言いませんでした", "いう", "polite past negative"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn combined_labels_name_all_meaningful_rules() {
        let h = Harness::new();
        // Labels combine every meaningful rule applied to the surface,
        // outermost first, skipping JL's intermediate-stem jargon — so
        // 住んでいた reads "past + teiru" rather than "past" and
        // 忘れてしまった reads "past + ended up" rather than "past".
        for (text, base, expected) in [
            ("住んでいた", "すむ", "past + teiru"),
            ("食べています", "たべる", "polite + teiru"),
            ("忘れてしまった", "わすれる", "past + ended up"),
            ("食べてしまった", "たべる", "past + ended up"),
            ("食べさせられてしまった", "たべる", "past + ended up + passive/potential + causative"),
            ("食べられます", "たべる", "polite + passive/potential"),
            ("しておきました", "する", "polite past + for now"),
            ("しておく", "する", "for now"),
            ("してくれました", "する", "polite past"),
            ("知らされなかった", "しる", "past + negative + causative passive"),
            ("言っちゃった", "いう", "past + ended up"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn suru_noun_phrases_resolve_to_the_noun() {
        let h = Harness::new();
        // 調査 + している is the suru verb 調査 doing teiru; the dictionary
        // headword is the noun 調査, not a literal 調査する or the coincidental
        // homophone 弔する (ちょうする) that whole-reading deconjugation finds.
        for (text, want_label) in [
            ("調査している", "teiru"),
            ("調査していた", "past + teiru"),
            ("調査して", "te"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(span.surface, text);
            assert_eq!(top_reading(&span), "ちょうさ", "{text} should resolve to 調査");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(want_label),
                "{text} should be labeled {want_label}, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn unknown_leading_token_deconjugates_surface_not_truncated_reading() {
        let h = Harness::new();
        // One-liner: ジイさんじゃない. The ジイ token is unknown (empty reading),
        // so the concatenated reading "さんじゃない" would misfire to さん. The
        // full surface じいさんじゃない deconjugates to じいさん (爺さん) instead.
        let span = h.lookup("ジイさんじゃない。", 0);
        assert_eq!(span.surface, "ジイさんじゃない");
        assert_eq!(top_reading(&span), "じいさん");
        assert_eq!(span.deconjugated_from.as_deref(), Some("copula"));

        // マズいんだ: the unknown katakana マズ must not collapse to the tail's
        // deconjugation (いんだ -> いぬ); instead the false maz/boundary is
        // crossed to form the real word マズい (= 不味い, まずい).
        let span = h.lookup("マズいんだ", 0);
        assert_eq!(span.surface, "マズい");
        assert_eq!(top_reading(&span), "まずい");
    }

    #[test]
    fn even_if_rules_name_the_condition() {
        let h = Harness::new();
        // JL has no rules for the も-ending te-form, so 急がなくても used to
        // fall back to naming an intermediate stem ("adverbial stem").
        for (text, base, expected) in [
            ("急がなくても", "いそぐ", "even if not"),
            ("食べなくても", "たべる", "even if not"),
            ("行かなくても", "いく", "even if not"),
            ("しなくても", "する", "even if not"),
            ("食べても", "たべる", "even if"),
            ("行っても", "いく", "even if"),
            ("飲んでも", "のむ", "even if"),
            ("高くても", "たかい", "even if"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn filler_token_allows_subtoken_span() {
        let h = Harness::new();
        // MeCab mis-tokenizes ともう as a single adjective token; the フィラー
        // cursor あ must be allowed to extend character-by-character so あと
        // still resolves instead of falling apart into あ/とも/う.
        let spans = scan(&h, "あともう一つ。");
        assert_eq!(spans[0].surface, "あと");
        assert_eq!(top_reading(&spans[0]), "あと");
        assert_eq!(spans[1].surface, "もう一つ");
        assert_eq!(top_reading(&spans[1]), "もうひとつ");
    }

    #[test]
    fn suru_noun_does_not_absorb_particle_phrase() {
        let h = Harness::new();
        // 除外するとして: として is an independent particle and must not extend
        // the suru-noun span — it used to collapse the whole thing to 除外.
        let spans = scan(&h, "除外するとして。");
        assert_eq!(spans[0].surface, "除外する");
        assert_eq!(top_reading(&spans[0]), "じょがい");
        assert_eq!(spans[1].surface, "として");
    }

    #[test]
    fn suru_noun_does_not_absorb_following_verb() {
        let h = Harness::new();
        // 徹底して伏せた: the independent verb 伏せた must not be absorbed into
        // the suru-noun span — it used to collapse to 徹底 entirely.
        let spans = scan(&h, "周りの大人が徹底して伏せた。");
        let hitoshi = spans.iter().find(|s| s.surface.contains("徹底")).unwrap();
        assert_eq!(hitoshi.surface, "徹底して");
        assert_eq!(top_reading(hitoshi), "てってい");
        let fuse = spans.iter().find(|s| s.surface.contains("伏せ")).unwrap();
        assert_eq!(fuse.surface, "伏せた");
        assert_eq!(top_reading(fuse), "ふせる");
    }

    #[test]
    fn verb_span_does_not_swallow_following_noun() {
        let h = Harness::new();
        // 引いたそう is 引く + the hearsay noun そう; the span must stop at
        // 引いた so 引く is found, instead of deconjugating the swallow 引いたそう
        // to ひる (干る).
        let spans = scan(&h, "すぐに引いたそうだ。");
        let hii = spans.iter().find(|s| s.surface.starts_with("引")).unwrap();
        assert_eq!(hii.surface, "引いた");
        assert_eq!(top_reading(hii), "ひく");
    }

#[test]
    fn deconjunction_prefers_kanji_sharing_entries() {
        let h = Harness::new();
        // 書けない must resolve to 書く (to write) first, not the coincidental
        // homophone 掛ける whose reading かける matches the potential form;
        // the shared leading kanji 書 outranks deeper deconjugation steps.
        let spans = scan(&h, "いや先輩はラテン語は書けない。");
        let kakenai = spans.iter().find(|s| s.surface == "書けない").unwrap();
        assert_eq!(top_reading(kakenai), "かく");
        assert_eq!(kakenai.entries.first().unwrap().spellings[0], "書く");

        // 引けない keeps the same-kanji potential 引ける ahead of 引く, while
        // unrelated homophones (弾く etc.) fall behind.
        let spans = scan(&h, "そんな人はいないから引けない。");
        let hikenai = spans.iter().find(|s| s.surface == "引けない").unwrap();
        let readings: Vec<String> = hikenai
            .entries
            .iter()
            .map(|e| e.spellings.first().unwrap().clone())
            .collect();
        assert!(readings.iter().take(2).all(|s| s.contains("引")), "{readings:?}");
    }

    #[test]
    fn explanatory_nda_never_merges_with_preceding_verb() {
        let h = Harness::new();
        // The ん of んだ is the explanatory copula, not a noun: したんだ must
        // split as した (past of する) + んだ, never resolve the whole
        // surface as a single rare verb (湑む したむ).
        let spans =
            scan(&h, "「もしもし。......お前、なにしたんだ? ジイさんバアさんたちパニック状態だぞ」");
        let shita = spans.iter().find(|s| s.surface == "した").unwrap();
        assert_eq!(top_reading(shita), "する");
        assert!(
            spans.iter().all(|s| s.surface != "したんだ"),
            "したんだ must not collapse into one span"
        );
        let nda = spans.iter().find(|s| s.surface == "んだ").unwrap();
        assert!(!nda.entries.is_empty(), "んだ should still be lookup-able");
    }

    #[test]
    fn merged_teru_stays_within_suru_noun_phrase() {
        let h = Harness::new();
        // MeCab tokenizes 会議してる as 会議 + し + てる (て+いる merged into
        // one verb token), so the suru construction must still absorb the full
        // span and resolve to the noun 会議 rather than stopping at 会議し.
        let spans =
            scan(&h, "「いま団地の管理会社と、お寺で除霊するからどっちが費用を出すって会議してるよ」");
        let kaigi = spans.iter().find(|s| s.surface == "会議してる").unwrap();
        assert_eq!(top_reading(kaigi), "かいぎ");
        assert_eq!(kaigi.entries.first().unwrap().spellings[0], "会議");
    }

    #[test]
    fn past_aux_merged_tan_merges_with_copula_tail() {
        let h = Harness::new();
        // MeCab merges the past た with the explanatory ん into one 名詞 token
        // (たん): 寝てたんじゃなかったのか resolves as ONE span to 寝る, with
        // the copula tail named in the label — while lookups starting AT たん
        // itself still never collapse into たんじゃなかった -> 肝.
        let spans = scan(&h, "なんか用か。寝てたんじゃなかったのか");
        let whole = spans
            .iter()
            .find(|s| s.surface == "寝てたんじゃなかった")
            .unwrap();
        assert_eq!(top_reading(whole), "ねる");
        let label = whole.deconjugated_from.as_deref().unwrap();
        assert!(label.contains("explanatory"), "label names the tail, got {label:?}");
        // Skip-cycling still reaches the shorter 寝てた.
        let span = h.lookup("なんか用か。寝てたんじゃなかったのか", 6);
        assert_eq!(span.surface, "寝てたんじゃなかった");
        // A lookup starting at たん itself must not become 肝.
        let tan = h.lookup("なんか用か。寝てたんじゃなかったのか", 8);
        assert_ne!(tan.surface, "たんじゃなかった");
    }

    #[test]
    fn suru_noun_absorbs_passive_sareru() {
        let h = Harness::new();
        // 解除された is 解除 + される (passive): the suru construction must absorb
        // the passive れ (and the past た) and resolve to 解除, not stop at the
        // truncated 解除さ.
        let spans = scan(&h, "待てと言おうとしたが、容赦なく時間停止は解除された。");
        let kaijo = spans
            .iter()
            .find(|s| s.surface == "解除された")
            .unwrap();
        assert_eq!(top_reading(kaijo), "かいじょ");
        assert_eq!(kaijo.entries.first().unwrap().spellings[0], "解除");
        assert_eq!(kaijo.deconjugated_from.as_deref(), Some("past + passive"));
    }

    #[test]
    fn noun_particle_content_verb_split_into_three_spans() {
        let h = Harness::new();
        // 風船でも割れる is three phrases: the noun 風船, the inclusive particle
        // でも, and the verb 割れる. It must not deconjugate the whole string
        // into 諷する (ふうする).
        let spans = scan(&h, "風船でも割れるみたいな消え方。");
        assert!(
            spans.iter().all(|s| s.surface != "風船でも割れる"),
            "風船でも割れる must not collapse into one span"
        );
        let fusen = spans.iter().find(|s| s.surface == "風船").unwrap();
        assert_eq!(top_reading(fusen), "ふうせん");
        let demo = spans.iter().find(|s| s.surface == "でも").unwrap();
        assert!(!demo.entries.is_empty(), "でも should still be lookup-able");
        let wareru = spans.iter().find(|s| s.surface == "割れる").unwrap();
        assert_eq!(top_reading(wareru), "われる");
    }

    #[test]
    fn prohibitive_njanai_conjugates_to_the_verb() {
        let h = Harness::new();
        // 買うんじゃない (ん = の, じゃ = では) is the prohibitive "don't":
        // looking up 買う shows the whole tail as a conjugation of the verb.
        let spans = scan(&h, "野菜を買うんじゃない");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃない").unwrap();
        assert_eq!(top_reading(kau), "かう");
        assert_eq!(kau.entries.first().unwrap().spellings[0], "買う");
        assert_eq!(kau.deconjugated_from.as_deref(), Some("don't"));
        // The past んじゃなかった names the missed obligation "shouldn't have".
        let spans = scan(&h, "わざわざ買うんじゃなかった");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃなかった").unwrap();
        assert_eq!(top_reading(kau), "かう");
        assert_eq!(kau.entries.first().unwrap().spellings[0], "買う");
        assert_eq!(kau.deconjugated_from.as_deref(), Some("shouldn't have"));
    }

    #[test]
    fn prohibitive_njanai_works_across_verb_classes() {
        let h = Harness::new();
        // Godan (帰る), ichidan (食べる), suru (する), and kuru (来る) all
        // attach the prohibitive tail to their dictionary form.
        let spans = scan(&h, "そろそろ帰るんじゃない。");
        let kaeru = spans.iter().find(|s| s.surface == "帰るんじゃない").unwrap();
        assert_eq!(top_reading(kaeru), "かえる");
        assert_eq!(kaeru.deconjugated_from.as_deref(), Some("don't"));
        let spans = scan(&h, "食べるんじゃない");
        let taberu = spans.iter().find(|s| s.surface == "食べるんじゃない").unwrap();
        assert_eq!(top_reading(taberu), "たべる");
        assert_eq!(taberu.deconjugated_from.as_deref(), Some("don't"));
        // The suru of そうする merges with the adverb into one token, so the
        // prohibitive construction is verified through a standalone suru and
        // its んじゃなかった tail at a fresh span start instead.
        let spans = scan(&h, "まさか、するんじゃなかった。そろそろ帰るんじゃない。");
        let suru = spans.iter().find(|s| s.surface == "するんじゃなかった").unwrap();
        assert_eq!(suru.deconjugated_from.as_deref(), Some("shouldn't have"));
        assert_eq!(suru.entries.first().unwrap().spellings[0], "為る");
    }

    #[test]
    fn prolonging_kana_reading_split_across_unknown_token() {
        let h = Harness::new();
        // MeCab splits くしゃっと into くし + ゃっと (the latter an unknown
        // token). Such an unknown token normally stops a span, but here the
        // kana crossing including it is a real dictionary reading, so the
        // word must be found whole.
        let spans = scan(&h, "嬉しそうにくしゃっと顔をゆがめた。");
        let kushatto = spans.iter().find(|s| s.surface == "くしゃっと").unwrap();
        assert!(
            kushatto.entries.iter().any(|e| e.readings.contains(&"くしゃっと".to_string())),
            "くしゃっと should be found"
        );
        // A non-word crossing (疲れるぅ) must still not extend.
        let span = h.lookup("疲れるぅ", 0);
        assert_eq!(span.surface, "疲れる");
        assert_eq!(top_reading(&span), "つかれる");
    }

    #[test]
    fn referential_ko_split_prefers_parts_over_reading_entry() {
        let h = Harness::new();
        // もうこ (もう + こ) merges the referential こ into one token; when it
        // is followed by の (この), the parts もう + この must win over the
        // reading entry 蒙古, and both parts stay lookup-able.
        let spans = scan(&h, "俺はもうこの世界には――。");
        let mou = spans.iter().find(|s| s.surface == "もう").unwrap();
        assert!(mou.entries.iter().any(|e| e.spellings.contains(&"蒙".to_string())));
        let kono = spans.iter().find(|s| s.surface == "この").unwrap();
        assert!(kono.entries.iter().any(|e| e.spellings.contains(&"此の".to_string())));
        assert_eq!(spans.iter().find(|s| s.surface == "世界").unwrap().entries[0].spellings[0], "世界");
        assert!(spans.iter().all(|s| s.surface != "もうこ"), "蒙古 must not win");
        // Correlative ここ/そこ/どこ themselves never split.
        let span = h.lookup("ここ", 0);
        assert_eq!(span.surface, "ここ");
        let span = h.lookup("そこ", 0);
        assert_eq!(span.surface, "そこ");
    }

    #[test]
    fn suru_noun_potential_badable_forms_resolve_to_the_noun() {
        let h = Harness::new();
        // 交信できて (potential できる) directly continues the suru verb from
        // the noun 交信 — the construction must resolve to 交信, not collapse
        // into a coincidental reading (航する).
        // MeCab merges できて + ない into a single token (交信できてない), so
        // the suru continuation resolves 交信 as the head of that whole span —
        // the noun must surface, never collapse into 航する.
        let spans = scan(&h, "すぐ崩れるから、交信できてない");
        let koushin = spans.iter().find(|s| s.surface == "交信できてない").unwrap();
        assert_eq!(top_reading(koushin), "こうしん");
        assert_eq!(koushin.entries.first().unwrap().spellings[0], "交信");
        assert!(koushin.deconjugated_from.as_deref().unwrap().contains("potential"));
        // The bare potential (交信できて, no ない tail) continues the suru verb
        // all the way from the noun — it must resolve to 交信 as well.
        let spans = scan(&h, "すぐ崩れるから、交信できて");
        let koushin = spans.iter().find(|s| s.surface == "交信できて").unwrap();
        assert_eq!(top_reading(koushin), "こうしん");
        assert_eq!(koushin.entries.first().unwrap().spellings[0], "交信");
        assert!(koushin.deconjugated_from.as_deref().unwrap().contains("potential"));
    }

    #[test]
    fn nande_merges_across_copula_token_split() {
        let h = Harness::new();
        // After ...... MeCab tags な as a copula auxiliary (助動詞) + んで,
        // which would lock な to a single token — but なんで (何で, "why") is
        // a real adverb entry and must win whole...
        let spans = scan(&h, "30歳にもなって......なんで今泣いてるんだ。");
        let nande = spans.iter().find(|s| s.surface == "なんで").unwrap();
        assert!(nande.entries.iter().any(|e| e.spellings.contains(&"何で".to_string())));
        // ...while なんだ keeps な single (な+ん is never merged).
        let spans = scan(&h, "そういう相手なんだ");
        assert!(spans.iter().any(|s| s.surface == "な"));
        assert!(spans.iter().all(|s| s.surface != "なん"));
    }

    #[test]
    fn njanai_label_depends_on_stem_tense() {
        let h = Harness::new();
        // Dictionary-form stem + んじゃない/んじゃなかった is the prohibitive
        // ("don't" / "shouldn't have")...
        let spans = scan(&h, "野菜を買うんじゃない");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃない").unwrap();
        assert_eq!(kau.deconjugated_from.as_deref(), Some("don't"));
        let spans = scan(&h, "わざわざ買うんじゃなかった");
        let kau = spans.iter().find(|s| s.surface == "買うんじゃなかった").unwrap();
        assert_eq!(kau.deconjugated_from.as_deref(), Some("shouldn't have"));
        // ...but past/negative stems carrying the same tail are explanatory —
        // 寝てたんじゃなかった is "wasn't sleeping", never "shouldn't have".
        let spans = scan(&h, "なんか用か。寝てたんじゃなかったのか");
        let nete = spans
            .iter()
            .find(|s| s.surface == "寝てたんじゃなかった")
            .unwrap();
        let label = nete.deconjugated_from.as_deref().unwrap();
        assert!(label.contains("explanatory"), "got {label:?}");
        assert!(!label.contains("shouldn't have"), "got {label:?}");
        let spans = scan(&h, "食べないんじゃない");
        let tabe = spans.iter().find(|s| s.surface == "食べないんじゃない").unwrap();
        assert_eq!(top_reading(tabe), "たべる");
        let label = tabe.deconjugated_from.as_deref().unwrap();
        assert!(label.contains("explanatory"), "got {label:?}");
    }

    #[test]
    fn suru_tari_construction_stays_one_span() {
        let h = Harness::new();
        // たり links suru clauses (保存したりしていた): the second し is the
        // suru verb continuing, not a new phrase — the whole span resolves to
        // 保存, never cut at 保存し.
        let spans = scan(
            &h,
            "どこかの掲示板を読んだり、流行の服の画像をいろいろ保存したりしていた。",
        );
        let hozon = spans.iter().find(|s| s.surface == "保存したりしていた").unwrap();
        assert_eq!(top_reading(hozon), "ほぞん");
        assert_eq!(hozon.entries.first().unwrap().spellings[0], "保存");
        // The たり listing is named instead of collapsing to bare "suru".
        assert_eq!(
            hozon.deconjugated_from.as_deref(),
            Some("past + teiru + tari")
        );
        assert!(spans.iter().all(|s| s.surface != "保存し"));
    }

    #[test]
    fn bare_datta_desita_resolve_to_copula() {
        let h = Harness::new();
        // だった after a particle has no noun stem to absorb into (穹 is a
        // name, から intervenes): it resolves to the copula だ itself, past.
        let spans = scan(&h, "その着信とメールは全て穹からだった。");
        let datta = spans.iter().find(|s| s.surface == "だった").unwrap();
        assert_eq!(top_reading(datta), "だ");
        assert_eq!(datta.deconjugated_from.as_deref(), Some("past"));
        // Same for polite でした after a particle (JL's rewrite rules map
        // でした -> です, labeled "past"; the entry itself carries politeness).
        let spans = scan(&h, "彼からでした。");
        let deshita = spans.iter().find(|s| s.surface == "でした").unwrap();
        assert_eq!(top_reading(deshita), "です");
        assert_eq!(deshita.deconjugated_from.as_deref(), Some("past"));
    }

    #[test]
    fn must_construction_variants_stay_whole() {
        let h = Harness::new();
        // Polite past, colloquial stems, ねば, では, and kuru: the いけない/
        // ならない tail must not split into its own span.
        for (text, pos, surface, base) in [
            ("食べなければいけませんでした", 0, "食べなければいけませんでした", "たべる"),
            ("宿題をしなければいけない", 3, "しなければいけない", "する"),
            ("行かなきゃいけない", 0, "行かなきゃいけない", "いく"),
            ("しなくちゃいけない", 0, "しなくちゃいけない", "する"),
            ("来なければいけない", 0, "来なければいけない", "くる"),
            ("行かねばならない", 0, "行かねばならない", "いく"),
            ("食べなくっちゃいけない", 0, "食べなくっちゃいけない", "たべる"),
            ("行かないといけませんでした", 0, "行かないといけませんでした", "いく"),
        ] {
            let span = h.lookup(text, pos);
            assert_eq!(span.surface, surface, "{text} should stay one span");
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some("must"),
                "{text} should be labeled must, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn must_tail_does_not_swallow_following_verb() {
        let h = Harness::new();
        // A conditional followed by an unrelated verb is not a must
        // construction: なければ opens the chain, but 食べる is not a
        // must-auxiliary (いく/いける/なる), so it must still split.
        let spans = scan(&h, "行かなければ食べる。");
        assert!(spans.iter().all(|s| s.surface != "行かなければ食べる"));
        let nake = spans.iter().find(|s| s.surface == "行かなければ").unwrap();
        assert_eq!(top_reading(nake), "いく");
    }

    #[test]
    fn all_span_entries_are_jmdict_entries() {
        // Item 1: the splitter (IPADIC/UniDic) never creates entries — every
        // id in every span must resolve in the JMdict-built by_id map.
        let h = Harness::new();
        for text in [
            "調査している",
            "食べられます",
            "風船でも割れるみたいな消え方。",
            "トイレを使ってから、手を洗わないといけません。",
        ] {
            for s in scan(&h, text) {
                for e in s.entries.iter().chain(s.related_entries.iter()) {
                    assert!(
                        h.index.by_id.contains_key(&e.id),
                        "span {:?} entry {} is not a JMdict entry",
                        s.surface,
                        e.id
                    );
                }
            }
        }
    }

    #[test]
    fn chouonpu_lookup_is_symmetric() {
        // Item 2: queries with or without ー reach the same index keys.
        let h = Harness::new();
        let with = normalize::normalize_variants("セーソー");
        let without = normalize::normalize_variants("せいそう");
        assert!(with.contains(&"せいそう".to_string()));
        // Every variant of a spelling is indexed, so either query form hits.
        for key in with.iter().chain(without.iter()) {
            let _ = key;
        }
        let related_a = find_containing("せいそう", &h.index, 5);
        let related_b = find_containing("せーそー", &h.index, 5);
        // Both directions query without error (exact hits depend on dict
        // contents, but neither direction may panic or diverge in handling).
        assert!(related_a.len() <= 5 && related_b.len() <= 5);
    }

    #[test]
    fn volitional_hairu_resolves_despite_homograph() {
        let h = Harness::new();
        // 本題に入ろう: volitional of 入る — MeCab mistags 入ろう as a noun
        // (reading にゅうろう), but surface deconjugation still reaches 入る.
        // (JMdict splits 入る into いる/はいる entries sharing the spelling;
        // either headword is correct here.)
        let spans = scan(&h, "助かる。さっそく本題に入ろう。");
        let hairu = spans.iter().find(|s| s.surface == "入ろう").unwrap();
        assert_eq!(hairu.entries.first().unwrap().spellings[0], "入る");
        assert_eq!(hairu.deconjugated_from.as_deref(), Some("volitional"));
    }

    #[test]
    fn suru_noun_with_wo_particle_stays_one_span() {
        let h = Harness::new();
        // 質問をする is a JMdict expression headword, so 質問をしている is one
        // span resolving to 質問 (teiru) — not 質問 / を / している.
        let spans = scan(&h, "何度も同じ質問をしている。");
        let shitsumon = spans.iter().find(|s| s.surface == "質問をしている").unwrap();
        assert_eq!(top_reading(shitsumon), "しつもん");
        assert_eq!(shitsumon.deconjugated_from.as_deref(), Some("teiru"));
    }

    #[test]
    fn suru_noun_causative_stays_one_span() {
        let h = Harness::new();
        // 消失させる (causative of 消失する) resolves to 消失 as one span.
        let spans = scan(&h, "先輩を消失させるのは、俺だった。");
        let shoshitsu = spans.iter().find(|s| s.surface == "消失させる").unwrap();
        assert_eq!(top_reading(shoshitsu), "しょうしつ");
        // Bare conditional negation of the causative also stays whole.
        let spans = scan(&h, "儀式を完了させなければ");
        let kanryo = spans.iter().find(|s| s.surface == "完了させなければ").unwrap();
        assert_eq!(top_reading(kanryo), "かんりょう");
    }

    #[test]
    fn teiku_past_compound_stays_one_span() {
        let h = Harness::new();
        // 盗んでいった is 盗む + ていく (teiku) + past, one span (the
        // explanatory んです tail rides along via copula-tail stripping).
        let spans = scan(&h, "うちから大事な物を盗んでいったんです");
        let nusumu = spans.iter().find(|s| s.surface == "盗んでいったんです").unwrap();
        assert_eq!(top_reading(nusumu), "ぬすむ");
        assert_eq!(nusumu.entries.first().unwrap().spellings[0], "盗む");
    }

    #[test]
    fn nande_and_ima_split_correctly() {
        let h = Harness::new();
        // なんで is the single 何で (why) entry; 今 must not absorb the
        // following verb (今泣いてる -> 今 + 泣いてる, never 忌む).
        let spans = scan(&h, "なんで今泣いてるんだ。");
        let nande = spans.iter().find(|s| s.surface == "なんで").unwrap();
        assert!(nande.entries.iter().any(|e| e.spellings.contains(&"何で".to_string())));
        let ima = spans.iter().find(|s| s.surface == "今").unwrap();
        assert!(!ima.entries.is_empty());
        let naiteru = spans.iter().find(|s| s.surface == "泣いてる").unwrap();
        assert_eq!(top_reading(naiteru), "なく");
        assert!(
            spans.iter().all(|s| !s.surface.contains("忌")),
            "must not resolve to 忌む"
        );
    }

    #[test]
    fn kanji_surface_prefers_kanji_sharing_entry() {
        let h = Harness::new();
        // 引かれた is written with 引: 引く must beat 惹かれる even though
        // 惹かれる deconjugates in fewer steps.
        let spans = scan(&h, "ナタが引かれた。");
        let hikareta = spans.iter().find(|s| s.surface == "引かれた").unwrap();
        assert_eq!(hikareta.entries.first().unwrap().spellings[0], "引く");
    }

    #[test]
    fn stem_sugiru_compound_resolves_to_front_verb() {
        let h = Harness::new();
        // 取り過ぎた is 取る + すぎる ("too much" + past), not the noun 取り過ぎ.
        let spans = scan(&h, "だが俺はもう歳を取り過ぎた。");
        let torisugi = spans.iter().find(|s| s.surface == "取り過ぎた").unwrap();
        assert_eq!(torisugi.entries.first().unwrap().spellings[0], "取る");
        assert_eq!(
            torisugi.deconjugated_from.as_deref(),
            Some("past + too much")
        );
    }

    fn cpos(text: &str, sub: &str) -> usize {
        let b = text.find(sub).unwrap_or_else(|| panic!("{sub:?} not in {text:?}"));
        text[..b].chars().count()
    }

    #[test]
    fn kiku_and_tedasuke_regions_resolve() {
        // No backend bug here (reported as showing only する): every hover
        // position already resolves correctly — 聞いてる -> 聞く,
        // 手助けをしてくれる -> 手助け, verb-start してくれる -> する.
        let h = Harness::new();
        let text = "豹馬(聞いてるぞ。5人で手助けをしてくれるんだよな)";
        let span = h.lookup(text, cpos(text, "聞いて"));
        assert_eq!(span.surface, "聞いてる");
        assert_eq!(top_reading(&span), "きく");
        let span = h.lookup(text, cpos(text, "手助け"));
        assert_eq!(span.surface, "手助けをしてくれる");
        assert_eq!(top_reading(&span), "てだすけ");
    }

    #[test]
    fn adjective_causative_saseru_stays_one_span() {
        // 気を悪くさせて: the ku-stem adjective continues into させる.
        let h = Harness::new();
        let text = "気を悪くさせてごめんね、本当に。";
        let span = h.lookup(text, cpos(text, "悪く"));
        assert_eq!(span.surface, "悪くさせて");
        assert_eq!(top_reading(&span), "わるい");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("causative + te")
        );
    }

    #[test]
    fn suru_noun_absorbs_chau_contraction() {
        // 遅刻しちゃう must stay one span (遅刻), not cut at し.
        let h = Harness::new();
        let text = "っと、そろそろ私、行かなきゃ遅刻しちゃう。";
        let span = h.lookup(text, cpos(text, "遅刻"));
        assert_eq!(span.surface, "遅刻しちゃう");
        assert_eq!(top_reading(&span), "ちこく");
    }

    #[test]
    fn koto_ni_natta_resolves_to_koto_ni_naru() {
        // 通うことになった: formal-noun こと + なる across the に particle.
        let h = Harness::new();
        let text = "学校の方からも連絡があり、いよいよ今日から通うことになった。";
        let span = h.lookup(text, cpos(text, "ことにな"));
        assert_eq!(span.surface, "ことになった");
        assert_eq!(top_reading(&span), "ことになる");
        assert_eq!(span.deconjugated_from.as_deref(), Some("past"));
    }

    #[test]
    fn shredded_warui_shi_merges_okurigana() {
        // MeCab shreds 悪いし as 悪|いし: the single-hiragana literal merge
        // must still reach 悪い, not the 悪 prefix noun.
        let h = Harness::new();
        let text = "あ、だったら、立ちっぱなしも悪いし中に入って待っててよ。";
        let span = h.lookup(text, cpos(text, "悪いし"));
        assert_eq!(span.surface, "悪い");
        assert_eq!(top_reading(&span), "わるい");
    }

    #[test]
    fn suru_noun_absorbs_nagara() {
        // 案内しながら行く: ながら continues the suru-verb chain.
        let h = Harness::new();
        let text = "学校まではるちゃん達を案内しながら行くつもりだったから。";
        let span = h.lookup(text, cpos(text, "案内し"));
        assert_eq!(span.surface, "案内しながら");
        assert_eq!(top_reading(&span), "あんない");
        assert_eq!(span.deconjugated_from.as_deref(), Some("while"));
    }

    #[test]
    fn unknown_kanji_okurigana_verb_stays_one_span() {
        // 淹れてもらう tokenized as 淹|れ|て|もらう: the unknown kanji's
        // following verb is the okurigana continuation, resolving to 淹れる.
        let h = Harness::new();
        let text = "そんな、淹れてもらうのにわざわざ注文なんてつけないよ。";
        let span = h.lookup(text, cpos(text, "淹れて"));
        assert_eq!(span.surface, "淹れてもらう");
        assert_eq!(span.entries[0].spellings[0], "淹れる");
        assert_eq!(top_reading(&span), "いれる");
    }

    #[test]
    fn suru_noun_absorbs_chau_contraction_imperative() {
        // 準備しちゃいな: ちゃい + the いなさい-contraction な stay in the
        // suru-verb span via the ちゃいな supplemental rule.
        let h = Harness::new();
        let text =
            "片付けと食器洗いは私がしておいてあげるから、はるちゃんは早く準備しちゃいなよ。";
        let span = h.lookup(text, cpos(text, "準備しちゃ"));
        assert_eq!(span.surface, "準備しちゃいな");
        assert_eq!(top_reading(&span), "じゅんび");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("contracted + casual imperative + ended up")
        );
    }

    #[test]
    fn adjective_njanai_takes_explanatory_negative() {
        // 良いんじゃないか: adjective stems take the copula tails too —
        // must not fall back to bare 良い or the coincidental 余韻.
        let h = Harness::new();
        let text = "逆に目立てて良いんじゃないかなぁ。";
        let span = h.lookup(text, cpos(text, "良いん"));
        assert_eq!(span.surface, "良いんじゃないか");
        assert_eq!(top_reading(&span), "よい");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("explanatory + negative + question")
        );
    }

    #[test]
    fn dattara_conditional_merges_across_aux_split() {
        // 帰るぐらいだったら: MeCab splits だっ|たら, both auxiliaries.
        let h = Harness::new();
        let text = "いや、帰るぐらいだったら一人でもなんとかなるよ。";
        let span = h.lookup(text, cpos(text, "だったら"));
        assert_eq!(span.surface, "だったら");
        assert!(
            span.entries.iter().any(|e| e.readings.first()
                .map_or(false, |r| r == "だ")
                && e.pos.iter().any(|p| p == "copula")),
            "だったら should resolve to the copula だ, got {:?}",
            span.entries
                .iter()
                .map(|e| e.readings.first())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shredded_oozappa_with_na_resolves() {
        // おおざっぱ tokenized as お|お|ざっぱな (unknown + adnominal な):
        // must reach 大雑把, not the おおい/多い misread of the bare おお.
        let h = Harness::new();
        let text = "その見た目通りにおおざっぱな性格なのか。";
        let span = h.lookup(text, cpos(text, "おおざっ"));
        assert_eq!(span.surface, "おおざっぱ");
        assert_eq!(top_reading(&span), "おおざっぱ");
        assert_eq!(span.entries[0].spellings[0], "大雑把");
    }

    #[test]
    fn ki_hover_covers_split_causative_expression() {
        // 気を悪くさせて tokenized 気|を|悪く|さ|せ|て: the せ continues the
        // さ (する-stem), and the whole span resolves to 気を悪くする —
        // not bare 悪い, which the longest-prefix tiebreak outranks.
        let h = Harness::new();
        let text = "わかんないけど。気を悪くさせてごめんね、本当に。";
        let span = h.lookup(text, cpos(text, "気を悪く"));
        assert_eq!(span.surface, "気を悪くさせて");
        assert_eq!(top_reading(&span), "きをわるくする");
        assert_eq!(span.entries[0].spellings[0], "気を悪くする");
        assert_eq!(span.deconjugated_from.as_deref(), Some("causative"));
    }

    #[test]
    fn suru_noun_tekureru_labels_benefactive() {
        // 手助けをしてくれる must read "suru + do for someone", not bare "suru".
        let h = Harness::new();
        let text = "豹馬(聞いてるぞ。5人で手助けをしてくれるんだよな)";
        let span = h.lookup(text, cpos(text, "手助け"));
        assert_eq!(span.surface, "手助けをしてくれる");
        assert_eq!(top_reading(&span), "てだすけ");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("suru + do for someone")
        );
    }
    #[test]
    fn souieba_expression_merges_across_split() {
        // そう|いえ|ば must reach the そういえば expression, not 相違.
        let h = Harness::new();
        let text = "そういえば、穹とは別のクラスになってしまった。";
        let span = h.lookup(text, cpos(text, "そうい"));
        assert_eq!(span.surface, "そういえば");
        assert_eq!(top_reading(&span), "そういえば");
    }

    #[test]
    fn kono_splits_from_koto() {
        // この|こと must stay この + こと, not このこ/海鼠子 — and この
        // must prefer 此の over the same-reading 九 via rentaishi agreement.
        let h = Harness::new();
        let text = "穹がこのことを知ったら、ますます学校に出てこないんじゃないかと、ひどく心配になる。";
        let span = h.lookup(text, cpos(text, "このこ"));
        assert_eq!(span.surface, "この");
        assert_eq!(span.entries[0].spellings[0], "此の");
    }

    #[test]
    fn chakuzushita_compound_verb_stays_one_span() {
        // 着|くず|し|た must reach 着崩す via kanji-sharing deconjugation.
        let h = Harness::new();
        let text = "少し着くずした制服に、あごヒゲがなんか少し学生離れしてる印象があるけど。";
        let span = h.lookup(text, cpos(text, "着くず"));
        assert_eq!(span.surface, "着くずした");
        assert_eq!(span.entries[0].spellings[0], "着崩す");
        assert_eq!(top_reading(&span), "きくずす");
    }

    #[test]
    fn fukai_demo_prefers_literal_stem() {
        // 不快でも must be 不快, not 深い + even-if.
        let h = Harness::new();
        let text = "でも、呼ばれ方に何かしらこだわりがあるわけじゃないし、彼女の愛嬌さもあって、特に不快でもなかった。";
        let span = h.lookup(text, cpos(text, "不快でも"));
        assert_eq!(span.surface, "不快");
        assert_eq!(top_reading(&span), "ふかい");
    }

    #[test]
    fn kangae_tenasasou_resolves_to_kangaeru() {
        // 考えてなさそう (tokens 考え|て|な|さ|そう) must reach 考える with
        // negative + hearsay labeling.
        let h = Harness::new();
        let text = "その無邪気そうな笑顔を見ていると実は何も考えてなさそうな雰囲気もなきにしもあらず。";
        let span = h.lookup(text, cpos(text, "考えてな"));
        assert_eq!(span.surface, "考えてなさそうな");
        assert_eq!(top_reading(&span), "かんがえる");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("negative + hearsay")
        );
    }

    #[test]
    fn katakana_hen_na_resolves_to_hen() {
        // ヘンな must be ヘン(変) + rentaikei な, not the ヘンナ plant.
        let h = Harness::new();
        let text = "ヘンな噂になると困るじゃないかぁ。";
        let span = h.lookup(text, cpos(text, "ヘンな"));
        assert_eq!(span.surface, "ヘン");
        assert_eq!(span.entries[0].spellings[0], "変");
    }

    #[test]
    fn nishinahyoron_merges_to_ni_naru_tono() {
        // に|なる|と must reach the になると expression.
        let h = Harness::new();
        let text = "ヘンな噂になると困るじゃないかぁ。";
        let span = h.lookup(text, cpos(text, "になると"));
        assert_eq!(span.surface, "になると");
        assert_eq!(top_reading(&span), "になると");
    }

    #[test]
    fn san_splits_from_ii() {
        // さん|いい|の must stay split, not さんい/三位 + いの.
        let h = Harness::new();
        let text = "渚さんいいの？迷惑じゃない？";
        let span = h.lookup(text, cpos(text, "さんいい"));
        assert_eq!(span.surface, "さん");
        let span = h.lookup(text, cpos(text, "さんいいの") + 2);
        assert_eq!(span.surface, "いい");
    }

    #[test]
    fn nan_darou_resolves_with_conjecture() {
        // 何|だろ|う must span 何だろう -> 何だ + conjecture, not bare 何だ.
        let h = Harness::new();
        let text = "何だろう、さっきもそうだったけど。";
        let span = h.lookup(text, cpos(text, "何だろう"));
        assert_eq!(span.surface, "何だろう");
        assert_eq!(span.entries[0].spellings[0], "何");
        assert_eq!(
            span.deconjugated_from.as_deref(),
            Some("conjecture")
        );
    }

    #[test]
    fn maji_ka_splits_to_maji() {
        // マジ|かっ must be マジ, not マジか/間近.
        let h = Harness::new();
        let text = "マジかっ！ちきしょー！俺より先に、何いい関係築いてんだよ！";
        let span = h.lookup(text, cpos(text, "マジか"));
        assert_eq!(span.surface, "マジ");
        assert_eq!(top_reading(&span), "まじ");
    }

    #[test]
    fn chikishoo_with_chouonpu_resolves_to_chikushou() {
        // ちきしょー (official 畜生 reading ちきしょう) must reach 畜生.
        let h = Harness::new();
        let text = "マジかっ！ちきしょー！俺より先に、何いい関係築いてんだよ！";
        let span = h.lookup(text, cpos(text, "ちきしょ"));
        assert_eq!(span.surface, "ちきしょー");
        assert_eq!(span.entries[0].spellings[0], "畜生");
    }

    #[test]
    fn sou_nanda_strips_to_sou() {
        // そう|な|ん|だ must span whole and resolve to そう + explanatory,
        // not cut at なん (遭難).
        let h = Harness::new();
        let text = "そ、そうなんだ。";
        let span = h.lookup(text, cpos(text, "そうなん"));
        assert_eq!(span.surface, "そうなんだ");
        assert_eq!(top_reading(&span), "そう");
        assert_eq!(span.deconjugated_from.as_deref(), Some("explanatory"));
    }

    #[test]
    fn youna_prefers_samana_over_you() {
        // 感じたような: 様な (rentaishi) must outrank 酔う, and the span
        // must not shorten to よう (様 lacks keiyodoshi, so rentaikei
        // shortening stays off).
        let h = Harness::new();
        let text = "感じたような。";
        let span = h.lookup(text, cpos(text, "ような"));
        assert_eq!(span.surface, "ような");
        assert_eq!(span.entries[0].spellings[0], "様な");
    }

    #[test]
    fn yokatta_with_emphatic_tsu_resolves_to_yoi() {
        // よかっ|たっ (past た + emphatic っ misread as 立つ) must reach
        // よかった -> 良い, not よか/余暇.
        let h = Harness::new();
        let text = "よかったっ、ハル君。";
        let span = h.lookup(text, cpos(text, "よかった"));
        assert_eq!(span.surface, "よかった");
        assert_eq!(top_reading(&span), "よい");
    }

    #[test]
    fn gomen_nasai_merges_across_split() {
        // ゴメン|な|さ|いね must reach ゴメンなさい -> 御免なさい.
        let h = Harness::new();
        let text = "みんな、ゴメンなさいね。";
        let span = h.lookup(text, cpos(text, "ゴメンな"));
        assert_eq!(span.surface, "ゴメンなさい");
        assert_eq!(span.entries[0].spellings[0], "御免なさい");
    }

    #[test]
    fn sou_hoihoi_splits_adverb_and_mimetic() {
        // そう + ほいほい must stay split (adverbs take no continuations),
        // not merge to そうほ/相補; ほいほい resolves whole.
        let h = Harness::new();
        let text = "はは、そうほいほい面白い。";
        let span = h.lookup(text, cpos(text, "そうほい"));
        assert_eq!(span.surface, "そう");
        let span = h.lookup(text, cpos(text, "ほいほい"));
        assert_eq!(span.surface, "ほいほい");
        assert_eq!(top_reading(&span), "ほいほい");
    }

    #[test]
    fn kaerimashou_volitional_reaches_kaeru() {
        // 帰り|ましょ|うかっ (う glued into mis-split adjective) must span
        // 帰りましょう -> 帰る, not cut before う.
        let h = Harness::new();
        let text = "お兄さんさあ、帰りましょうかっ!";
        let span = h.lookup(text, cpos(text, "帰りましょ"));
        assert_eq!(span.surface, "帰りましょう");
        assert_eq!(top_reading(&span), "かえる");
    }

    #[test]
    fn kiri_single_kanji_drops_coincidental_verbs() {
        // 霧がかかった: 霧 stays, but 切る/斬る/剪る (matched only through
        // the kana reading きり) must not clutter entries.
        let h = Harness::new();
        let text = "頭の中が霧がかかったようになっていて。";
        let span = h.lookup(text, cpos(text, "霧がかか"));
        assert_eq!(span.surface, "霧");
        assert_eq!(span.entries[0].spellings[0], "霧");
        assert!(!span.entries.iter().any(|e| e.spellings.first()
            .map_or(false, |s| s == "切る" || s == "着る" || s == "来る")),
            "coincidental kiru-verbs should be filtered, got {:?}",
            span.entries.iter().map(|e| e.spellings.first()).collect::<Vec<_>>());
    }

    #[test]
    fn sou_saseru_splits_to_sou() {
        // そうさせる must not resolve as one span to archaic 奏する;
        // hovering そう gives そう (させる -> する reachable separately).
        let h = Harness::new();
        let text = "そうさせる。";
        let span = h.lookup(text, cpos(text, "そうさせ"));
        assert_eq!(span.surface, "そう");
    }

    #[test]
    fn wakannai_slurred_negative_resolves() {
        // わかんない (slurred わからない) must reach 分かる.
        let h = Harness::new();
        let text = "わかんないけど。";
        let span = h.lookup(text, cpos(text, "わかんな"));
        assert_eq!(span.surface, "わかんないけど");
        assert_eq!(top_reading(&span), "わかる");
    }

    #[test]
    fn bonus_locks_koto_ni_suru_furisou_ayamannakute() {
        // Loose ends that already work: ことにする, 降りそう, あやまんなくて.
        let h = Harness::new();
        let span = h.lookup("ことにする。", cpos("ことにする。", "ことにする"));
        assert_eq!(span.surface, "ことにする");
        assert_eq!(span.entries[0].spellings[0], "事にする");
        let span = h.lookup("雨が降りそう。", cpos("雨が降りそう。", "降りそう"));
        assert_eq!(span.surface, "降りそう");
        assert_eq!(top_reading(&span), "おりる");
        let span = h.lookup("あやまんなくて良いよ。", cpos("あやまんなくて良いよ。", "あやまんな"));
        assert_eq!(top_reading(&span), "あやまる");
    }

    #[test]
    fn sou_nanda_etc_strip_universally() {
        // なんだ-family tails attach to any stem.
        let h = Harness::new();
        let span = h.lookup("そ、そうなんだ。", cpos("そ、そうなんだ。", "そうなん"));
        assert_eq!(span.surface, "そうなんだ");
        assert_eq!(top_reading(&span), "そう");
        // Verb + explanatory ん keeps the deliberate split (したんだ ->
        // した + んだ, never merged): verbs never absorb ん.
        let span = h.lookup("行くんですか。", cpos("行くんですか。", "行くんです"));
        assert_eq!(span.surface, "行く");
        // Noun stems do resolve whole through the tail.
        let span = h.lookup("本なんですか。", cpos("本なんですか。", "本なんで"));
        assert_eq!(span.surface, "本なんですか");
        assert_eq!(top_reading(&span), "ほん");
    }

    #[test]
    fn tto_particle_maps_to_to() {
        // Single-token っと (quotative/emphatic と) must resolve, not vanish.
        let h = Harness::new();
        let text = "ちりっと走った。";
        let pos = cpos(text, "っと");
        let tokens = h.tokens(text);
        let span = lookup_from_position(text, pos, 0, &h.index, &h.decon, &tokens).unwrap();
        assert_eq!(span.surface, "っと");
        assert!(span.entries.iter().any(|e| e.readings.first().map_or(false, |r| r == "と")),
            "っと should resolve to the と particle");
    }

    #[test]
    fn obscure_literal_prefers_common_stem() {
        // Orphan literals swallowing common stems split back.
        let h = Harness::new();
        let span = h.lookup("そこにいる。", cpos("そこにいる。", "そこに"));
        assert_eq!(span.surface, "そこ");
        let span = h.lookup("渚さんと行く。", cpos("渚さんと行く。", "さんと"));
        assert_eq!(span.surface, "さん");
    }

    #[test]
    fn bound_volitional_u_reaches_verb() {
        let h = Harness::new();
        let text = "お兄さんさあ、帰りましょうかっ!";
        let span = h.lookup(text, cpos(text, "帰りましょ"));
        assert_eq!(span.surface, "帰りましょう");
        assert_eq!(top_reading(&span), "かえる");
    }

    #[test]
    fn tatsu_past_emphasis_splits_ta() {
        // たっ (past た + emphatic っ) must end at た.
        let h = Harness::new();
        let span = h.lookup("よかったっ。", cpos("よかったっ。", "よかった"));
        assert_eq!(span.surface, "よかった");
        assert_eq!(top_reading(&span), "よい");
    }

    #[test]
    fn nasai_after_noun_resolves() {
        // ゴメン|な|さ|いね must reach ゴメンなさい -> 御免なさい.
        let h = Harness::new();
        let text = "みんな、ゴメンなさいね。";
        let span = h.lookup(text, cpos(text, "ゴメンな"));
        assert_eq!(span.surface, "ゴメンなさい");
        assert_eq!(span.entries[0].spellings[0], "御免なさい");
    }

    #[test]
    fn causative_prefers_complete_stem_word() {
        // そうさせる must not resolve as one span to archaic 奏する.
        let h = Harness::new();
        let span = h.lookup("そうさせる。", cpos("そうさせる。", "そうさせ"));
        assert_eq!(span.surface, "そう");
    }

    #[test]
    fn sou_ieba_kanji_reaches_expression() {
        // そう|言え|ば (kanji tail) must merge via readings to そう言えば.
        let h = Harness::new();
        let text = "あー、そう言えば初めて。";
        let span = h.lookup(text, cpos(text, "そう言え"));
        assert_eq!(span.surface, "そう言えば");
        assert_eq!(span.entries[0].spellings[0], "そう言えば");
    }

    #[test]
    fn mono_ha_splits_to_mono() {
        // ものは must not resolve as one span to もの派.
        let h = Harness::new();
        let text = "足りないものはスーパーに。";
        let span = h.lookup(text, cpos(text, "ものは"));
        assert_eq!(span.surface, "もの");
    }

    #[test]
    fn naishi_single_token_prefers_nai() {
        // Conjunction-tagged ないし in dialogue position is ない + し.
        let h = Harness::new();
        let text = "体を鍛えてないし。";
        let span = h.lookup(text, cpos(text, "ないし"));
        assert_eq!(span.surface, "ない");
        assert_eq!(span.entries[0].spellings[0], "無い");
    }

    #[test]
    fn katakana_shai_outranks_homophones() {
        // Exact-script シャイ beats normalized-only 謝意/社医.
        let h = Harness::new();
        let text = "シャイなのか。";
        let span = h.lookup(text, cpos(text, "シャイ"));
        assert_eq!(span.surface, "シャイなのか");
        assert!(span.entries[0].readings.iter().any(|r| r == "シャイ"),
            "シャイ loanword should top, got {:?}", span.entries[0].readings.first());
    }

    #[test]
    fn suguni_prefix_shredding_resolves() {
        // す|ぐにどっか… (prefix + giant unknown) must reach すぐに/直ぐに.
        let h = Harness::new();
        let text = "すぐにどっか。";
        let span = h.lookup(text, cpos(text, "すぐに"));
        assert_eq!(span.surface, "すぐに");
        assert_eq!(span.entries[0].spellings[0], "直ぐに");
    }

    #[test]
    fn tto_particle_resolves_to_to() {
        // Single-token quotative っと must show と.
        let h = Harness::new();
        let text = "ちりっと走った。";
        let pos = cpos(text, "っと");
        let tokens = h.tokens(text);
        let span = lookup_from_position(text, pos, 0, &h.index, &h.decon, &tokens).unwrap();
        assert_eq!(span.surface, "っと");
        assert!(span.entries.iter().any(|e| e.readings.first().map_or(false, |r| r == "と")));
    }
}
