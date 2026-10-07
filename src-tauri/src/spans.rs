//! Span building: candidate end enumeration (token-aligned extensions,
//! completions, okurigana carve-outs, separator guards) plus the post-rules
//! that shorten or re-split the longest-first winner.

use crate::deconjugate::Deconjugator;
use crate::index::{find_containing, DictionaryIndex};
use crate::lookup::lookup_candidate;
use crate::normalize;
use crate::rank::{match_kind, pos_class, priority_score};
use crate::rules::{
    COMPLETIONS, CONTINUATIVE, CONTRACTION_AUX_VERBS, CONTRACTION_SURFACES, MAX_CHARS_COMBINED,
    SOKUON_TAIL_PARTICLES, TE_AUX_VERBS,
};
use crate::types::{MatchKind, MatchSpan, MorphToken};
use std::collections::HashSet;


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
pub(crate) fn lookup_from_position(
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
            // match (the ったく interjection). Same for contraction
            // fragments (ちゃっ -> ちゃう): the shredded piece is not a
            // ちゃう-stem, so morphology must not promote the auxiliary
            // over the adverb it shredded from (ちゃっちゃと -> ちゃう
            // over the adverb).
            if t.pos == "動詞" && t.base_form != t.surface && t.surface != "っ"
                && !CONTRACTION_AUX_VERBS.contains(&t.base_form.as_str())
            {
                Some(t.base_form.as_str())
            // あり is a verb stem under IPAdic (base 有る) but a plain
            // noun under UniDic (base == surface): without this the
            // morph-base mechanism strands and 蟻 outranks 有る.
            // Surface+reading anchored, so kanji 蟻 never matches.
            } else if t.surface == "あり" && t.reading == "あり" {
                Some("有る")
            // Continuative ある/おる come out tagged as auxiliaries (ありね):
            // the existence verbs are the only auxiliaries ever trusted this
            // way — ください/なさい keep their literal readings (their bases
            // くださる/なさる are excluded by the allowlist below).
            } else if t.pos == "助動詞"
                && t.base_form != t.surface
                && matches!(t.base_form.as_str(), "ある" | "おる")
            {
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
    // Ends contributed by fixed-expression completions (not by token
    // extension): the obscure-literal splitter must not re-fragment the
    // very constructions the COMPLETIONS table merges (まずは -> まず).
    let mut completion_ends: HashSet<usize> = HashSet::new();
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
                            completion_ends.insert(nend);
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
                                completion_ends.insert(nend);
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
                            completion_ends.insert(nend);
                        }
                        // The rest of that token continues the same
                        // construction (だった|り -> だったり): the completion
                        // landed mid-token, so the remainder belongs to the
                        // merged word rather than starting a new span. The
                        // shorter end above still forms the head on its own
                        // when the longer one doesn't resolve.
                        let full = tok.end.min(len);
                        if full > position {
                            ends.push(full);
                            completion_ends.insert(full);
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
        // な + ん -> なん (何) across the explanatory ん (どうなんだろう,
        // 事なんだけど): な is function-locked to its own token, so the pair
        // never forms from a な cursor. Only when the ん is followed by the
        // copula だ which itself continues (だろう, けど, よ...): a
        // sentence-final なんだ (相手なんだ, そういう相手なんだ) keeps the
        // deliberate な|ん|だ split, and なんで/なんです keep their own
        // paths (で follows the ん there, not だ).
        if position == t.start
            && t.surface == "な"
            && matches!(t.pos.as_str(), "助詞" | "助動詞")
        {
            let nn = tokens.iter().find(|tok| tok.start == t.end);
            let continues = nn
                .filter(|n| {
                    n.surface == "ん" && matches!(n.pos.as_str(), "名詞" | "助詞")
                })
                .and_then(|n| tokens.iter().find(|tok| tok.start == n.end))
                .map_or(false, |d| {
                    d.pos == "助動詞"
                        && matches!(d.base_form.as_str(), "だ" | "じゃ" | "です")
                        // A bare sentence-final だ keeps the deliberate
                        // な|ん|だ split (相手なんだ); an already-continued
                        // copula (だろう, です, じゃ, だった) merges even at
                        // end of text (どうなんだろう). UniDic keeps だろう
                        // whole where IPAdic split だろ|う, so the old
                        // "followed by more" test alone strands it.
                        && (d.surface != "だ"
                            || tokens
                                .iter()
                                .any(|tok| tok.start == d.end && tok.pos != "記号"))
                });
            if continues {
                if let Some(n) = nn {
                    let nend = n.end.min(len);
                    if nend > position {
                        ends.push(nend);
                    }
                }
            }
        }
        // If the cursor is on an unknown token (empty reading — katakana slang
        // like マズ), a filler token (フィラー — often mis-analyzed tokens
        // like ともう being one token), or a prefix (接頭詞 like す in すぐ,
        // which only ever forms words together with what follows), allow the
        // span to continue character-by-character into the next token so a
        // real literal word can still form across the false boundary. Two
        // tokens deep: an honorific prefix plus stem plus inflection tail
        // (お|待た|せ) needs the second token too, or お待たせ can never
        // form and falls back to お待た (お股). Ends still have to resolve
        // to win, so longest-first fallback keeps this safe.
        if t.reading.is_empty() || t.pos == "フィラー" || t.pos == "接頭辞" || t.pos == "接頭詞" {
            if let Some(next) = tokens.iter().find(|tok| tok.start > position) {
                for e in (t.end + 1)..=next.end.min(len) {
                    ends.push(e);
                }
                if let Some(following) = tokens.iter().find(|tok| tok.start >= next.end && tok.start > t.end) {
                    for e in (next.end + 1)..=following.end.min(len) {
                        ends.push(e);
                    }
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
        // hiragana or kanji char completes a literal dictionary word, allow
        // ending there — the kanji half covers numeral-counter splits
        // (三|食分 -> 三食) the same way. Literal-only (never a
        // deconjugated end), function words stay locked, referential splits
        // keep priority — so のこ/はよ-style false positives can't form.
        // Kana-led cursors never merge: okurigana attaches to kanji stems,
        // and kana words (さん, そう, この, ヘン, マジ, ち) plus the next
        // kana would form coincidental words (さんい/三位, そうい/相違,
        // このこ/海鼠子, へんな/ヘナ, マジか/間近).
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
                        // Okurigana is never a particle: 日|とか must stay
                        // 日 + とか instead of merging into 日と -> 日ト/日土.
                        if next.pos != "助詞" {
                            let e = t.end + 1;
                            if e <= next.end.min(len) && e <= position + MAX_CHARS_COMBINED {
                                if let Some(c) = chars.get(t.end) {
                                    let cp = *c as u32;
                                    let is_kana = matches!(c, 'ぁ'..='ん');
                                    let is_kanji = (0x4E00..=0x9FFF).contains(&cp)
                                        || (0x3400..=0x4DBF).contains(&cp);
                                    if is_kana || is_kanji {
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
                        // Shredded te-form continuation (連んでんだ ->
                        // 連|ん|でん): allow ending two chars in when the
                        // merge ends in a te-form, so the stranded
                        // inflection can still resolve (the verb-gate
                        // exemption covers ranking). Single-char auxiliaries
                        // (たい/ない) never qualify. Failed lookups simply
                        // fall through longest-first.
                        let e2 = t.end + 2;
                        if e2 <= len && e2 <= position + MAX_CHARS_COMBINED {
                            if let (Some(c1), Some(c2)) =
                                (chars.get(t.end), chars.get(t.end + 1))
                            {
                                if matches!(c1, 'ぁ'..='ん')
                                    && matches!(c2, 'ぁ'..='ん')
                                {
                                    let merged2: String =
                                        chars[position..e2].iter().collect();
                                    if merged2.ends_with("て")
                                        || merged2.ends_with("で")
                                        || merged2.ends_with("って")
                                        || merged2.ends_with("んで")
                                    {
                                        ends.push(e2);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Honorific-suffix completion across MeCab shredding (カズちゃん
        // tokenized カズ|ち|ゃんとらぶらぶ): when the cursor starts a
        // kana-led token and the following characters complete a
        // closed-class suffix, allow ending there. Token-start only, so
        // ぼくんち's く (mid-token) never completes to くん; the closed
        // list keeps coincidental words out; longest-first still prefers
        // longer real spans (チャンネル wins over ちゃん).
        const SUFFIX_COMPLETIONS: &[&str] =
            &["ちゃん", "くん", "さま", "たち", "ども"];
        if let Some(t) = token_at_pos {
            if position == t.start
                && t.surface.chars().next().map_or(false, |c| {
                    matches!(c, 'ぁ'..='ん' | 'ァ'..='ン')
                })
            {
                let rest: String = chars[position..].iter().collect();
                for suffix in SUFFIX_COMPLETIONS {
                    if rest.starts_with(*suffix) {
                        ends.push(position + suffix.chars().count());
                    }
                }
            }
        }
        // Particle partition inside a fragmented kana token (カズちゃんとらぶらぶ
        // -> ち|ゃんとらぶらぶ, the latter unknown): when the cursor kana alone
        // is a genuine single-kana particle AND the remainder of the token still
        // resolves as its own word (らぶらぶ -> ラブラブ), keep the particle as
        // its own span instead of swallowing the homophone (とら -> 虎, とらぶら
        // as one run). Unknown tokens only — real dictionary words that happen
        // to start with a particle kana (かばん -> 鞄) stay atomic because their
        // token is known.
        if let Some(t) = token_at_pos {
            // Mid-token only: a cursor on an unknown run's leading character
            // is the run's own first syllable (マズいんだ must not split the
            // マズ token, while ゃんとらぶらぶ's inner と is a stray particle).
            if (t.base_form == "*" || t.reading.is_empty())
                && t.end > position + 1
                && position > t.start
            {
                let c = chars[position];
                let single = c.to_string();
                let is_particle = index
                    .by_text
                    .get(&normalize::normalize_text(&single))
                    .map_or(false, |es| {
                        es.iter().any(|e| {
                            e.pos.iter().any(|p| {
                                p.split(|ch: char| !ch.is_alphabetic())
                                    .any(|w| w == "particle")
                            })
                        })
                    });
                if is_particle {
                    let rest: String = chars[position + 1..t.end].iter().collect();
                    if !rest.is_empty() {
                        let empty_tokens: Vec<MorphToken> = Vec::new();
                        let resolves = match lookup_candidate(
                            &rest,
                            &index,
                            &decon,
                            None,
                            None,
                            &empty_tokens,
                            0,
                        ) {
                            Some((entries, _)) => !entries.is_empty(),
                            None => false,
                        };
                        if resolves {
                            ends.retain(|e| *e == position + 1);
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
            // Noun + を/に + verb dictionary compounds (身につける,
            // 皮をむく): the particle trips the separator guard below and
            // the inflected compound is never literal-known, so the verb's
            // end is exempted from both verb guards when noun + particle +
            // verb-base is itself a dictionary entry — the span then
            // resolves via normal lookup/deconjugation (literal for
            // dictionary forms, deconjugation for inflections like
            // 身につけた). A verb continuing right after (取り|過ぎ,
            // 食べ|ちゃ) belongs to a verb compound instead, so the
            // exemption yields there. Longest-first falls back when nothing
            // resolves, so ordinary phrases stay split (本を読む/トイレを
            // 使う/費用を出す are not dictionary entries).
            let pp_compound_end: Option<usize> = match token_at_pos {
                Some(t) if t.pos == "名詞" && position == t.start => {
                    let p = tokens.iter().find(|tok| tok.start == t.end);
                    let v = p
                        .filter(|p| {
                            p.pos == "助詞" && matches!(p.surface.as_str(), "を" | "に")
                        })
                        .and_then(|p| tokens.iter().find(|tok| tok.start == p.end))
                        .filter(|v| v.pos == "動詞" && v.base_form != "*");
                    match (p, v) {
                        (Some(p), Some(v)) => {
                            let continued = tokens
                                .iter()
                                .find(|tok| tok.start == v.end)
                                .map_or(false, |tok| tok.pos == "動詞");
                            let compound: String =
                                format!("{}{}{}", t.surface, p.surface, v.base_form);
                            let known = normalize::normalize_variants(&compound)
                                .iter()
                                .any(|k| index.by_text.contains_key(k));
                            if known && !continued {
                                Some(v.end)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            let is_pp_compound = |end: usize| pp_compound_end == Some(end);
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
                // of マジ.) Only after a verb or a volitional stem: without
                // that gate the rule fires on every う-initial token and
                // swallowed the first syllable of the next word (見つめて|う|
                // つむいていた -> 見つめてう).
                if let Some(&c) = chars.get(tok.start) {
                    let prev_volitional = tokens
                        .iter()
                        .filter(|t| t.end <= tok.start)
                        .last()
                        .map_or(false, |p| {
                            p.pos == "動詞"
                                || matches!(p.surface.as_str(), "ましょ" | "でしょ" | "だろ")
                        });
                    if c == 'う'
                        && tok.start + 1 > position
                        && tok.start + 1 < tok.end.min(len)
                        && prev_volitional
                    {
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
                // のか before a かもしれない construction (あるのかもしれない):
                // the の belongs to のかもしれない, not to an explanatory-のか
                // merge — without this the verb absorbs のか and かもしれない
                // never forms. Contiguous も|しれ|ない (or polite ませ) only,
                // with しれ verifiably the potential stem (base しれる);
                // anything else merges as before.
                if tok.pos == "助詞" && tok.surface == "か" {
                    let mut rest = tokens.iter().filter(|t| t.start >= tok.end);
                    let is_kamoshirenai = match (rest.next(), rest.next(), rest.next()) {
                        (Some(m), Some(s), Some(n))
                            if m.start == tok.end
                                && m.surface == "も"
                                && s.start == m.end
                                && s.surface == "しれ"
                                && s.base_form == "しれる"
                                && n.start == s.end
                                && (n.surface == "ない" || n.surface == "ませ") =>
                        {
                            true
                        }
                        _ => false,
                    };
                    if is_kamoshirenai {
                        break;
                    }
                }
                // Adverbs never take continuations (そうほいほい -> そう +
                // ほいほい, not そうほ/相補; とても親切 -> とても + 親切;
                // よく書く -> よく + 書く): content words always start new
                // phrases after an adverb — only particles, auxiliaries,
                // adnominals, and completions continue the span. Four
                // exemptions: the explanatory ん (そうなんだ still reaches
                // だ); split-causative せ/させる (そうさせる still reaches
                // せ for the causative-shorten rule); contracted
                // てしまう/でしまう verbs (そうしちゃい still reaches ちゃい
                // for the contraction-stem rule); fixed adverbial
                // compounds with noun continuations (もう一つ, もう一度),
                // lexicalized adverb+する units (ことにする,
                // ちゃんとする), and adverb+adjective units that are real
                // headwords (なんともない, よくない) — longest-first falls
                // back otherwise.
                let tok_is_contraction = tok.pos == "動詞"
                    && CONTRACTION_AUX_VERBS.contains(&tok.base_form.as_str());
                if token_at_pos.map_or(false, |t| t.pos == "副詞")
                    && matches!(
                        tok.pos.as_str(),
                        "名詞" | "動詞" | "形容詞" | "副詞"
                    )
                    && !(tok.surface == "ん" && matches!(tok.pos.as_str(), "名詞" | "助詞"))
                    && !tok_is_split_cause
                    && !tok_is_contraction
                {
                    // Adverb+adjective units that are real headwords
                    // (なんともない, よくない) also continue; とても親切
                    // and よく書く still split via longest-first fallback.
                    // A whole dictionary word may continue regardless of POS
                    // (どうしてる needs the てる continuation — どうしてる is
                    // a headword); an inflected verb fragment may not, since
                    // a key ending on one is a coincidence of the split
                    // (そうほ -> 相補 inside そうほいほい).
                    let compound_key = {
                        let compound: String =
                            chars[position..tok.end].iter().collect();
                        normalize::normalize_variants(&compound)
                            .iter()
                            .any(|k| index.by_text.contains_key(k))
                    };
                    let compound_known = compound_key
                        && (matches!(tok.pos.as_str(), "名詞" | "形容詞")
                            || (tok.pos == "動詞" && tok.base_form == "する")
                            || tok.base_form == tok.surface);
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
                                // Emphatic small-vowel coda glued onto an
                                // unknown token (おばぁ tokenized as おば|ぁが):
                                // when the span through the unknown token's
                                // first character resolves, allow ending
                                // there (one character in) as well — the coda
                                // spells the previous mora's vowel rather than
                                // adding one, so おばぁ -> おば -> 祖母 can
                                // form. Longest-first still falls through when
                                // the prefix resolves to nothing.
                                let head_ok = tok.start > position
                                    && tok.end > tok.start + 1
                                    && {
                                        let head: String =
                                            chars[position..tok.start + 1].iter().collect();
                                        lookup_candidate(
                                            &head,
                                            index,
                                            decon,
                                            context_reading,
                                            None,
                                            tokens,
                                            position,
                                        )
                                        .is_some()
                                    };
                                if head_ok {
                                    let sub = (tok.start + 1).min(len);
                                    if sub > position {
                                        ends.push(sub);
                                    }
                                }
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
                // a phrase, so it always extends. Contracted てしまう/
                // でしまう verbs (ちゃう/じゃう/ちまう/じまう) behave the
                // same: they continue the construction (食べちゃう,
                // そうしちゃい), mirroring the CONTRACTION_SURFACES
                // handling for unknown tokens below.
                // The vulgar auxiliary やがる binds its verb too:
                // バカにし|やがっ|た is する+やがる, resolved via the
                // literal しやがる-strip rule below. Only a verb's 連用形
                // takes it, so 怖がる/恥ずかしがる noun-bases (怖, 恥ずかし
                // are 名詞/形容動詞) stay phrase-initial.
                let tok_is_vulg_aux = tok.base_form == "がる"
                    && tokens
                        .iter()
                        .filter(|t| t.end <= tok.start && t.start >= position)
                        .rev()
                        .find(|t| t.pos != "助詞" && t.pos != "助動詞")
                        .map_or(false, |p| p.pos == "動詞");
                let tok_is_bound =
                    tok.base_form == "てる" || tok_is_contraction || tok_is_vulg_aux;
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
                // False た-form token (妬いちゃってた|し|ね, MeCab reads たし
                // as the masu-stem of 足す): the text is the past auxiliary た
                // plus the listing/reason し, so the span must be able to end
                // right after the た — otherwise たし|ね resolves to 足す and
                // the past is lost. Conjugated た-base only (base != surface),
                // so a genuine たす/楽し stem stays intact and longest-first
                // falls back when the shorter end doesn't resolve.
                if cursor_is_verb
                    && tok.pos == "動詞"
                    && tok.surface.chars().next() == Some('た')
                    && tok.base_form.starts_with('た')
                    && tok.base_form != tok.surface
                    && tok.surface.chars().count() > 1
                {
                    let sub_end = tok.start + 1;
                    if sub_end > position {
                        ends.push(sub_end);
                    }
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
                    && !is_pp_compound(tok.end)
                    && !tok_is_split_cause
                {
                    break;
                }
                // Noun-start spans never absorb a following content verb
                // directly (今泣いてる -> 今 + 泣いてる): verbs that continue
                // a te-auxiliary chain, a must construction (宿題をしなければ
                // いけない), the suru verb, a suru-noun inflection,
                // the bound てる, or a real verb-headed dictionary compound
                // (気になる) still extend. A known compound without any verb
                // reading (訳あり) must not swallow the existential verb
                // (わけありません -> わけ + ありません) — otherwise the
                // prefix boost promotes the orphan compound over ある.
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
                    && !is_pp_compound(tok.end)
                    && !(cursor_unknown && token_at_pos.map_or(false, |t| t.start == position))
                {
                    let compound: String = chars[position..tok.end].iter().collect();
                    let compound_known = normalize::normalize_variants(&compound)
                        .iter()
                        .any(|k| index.by_text.contains_key(k));
                    // Word-split, not substring, so "adverb" never counts.
                    let compound_has_verb = normalize::normalize_variants(&compound)
                        .iter()
                        .flat_map(|k| index.by_text.get(k).into_iter().flatten())
                        .any(|e| {
                            e.pos.iter().any(|p| {
                                p.split(|c: char| !c.is_alphabetic())
                                    .any(|w| w == "verb")
                            })
                        });
                    if !(compound_known && compound_has_verb)
                        && !compound_shares_deconj_kanji(tok.end)
                    {
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
                // A verb stem feeding a nominal suffix (もやし|炒め): when
                // the cursor verb's own surface is a dictionary noun, the
                // nominal reading is intended — extending swallows the
                // compound into the verb (もやし炒めも -> 燃やす). Real
                // verb+suffix units (食べっぷり) never have noun-known
                // stems, and noun-cursor compounds (野菜炒め) never take
                // this branch, so both still form.
                if cursor_is_verb
                    && tok.pos == "接尾辞"
                    && token_at_pos.map_or(false, |t| {
                        normalize::normalize_variants(&t.surface)
                            .iter()
                            .flat_map(|k| index.by_text.get(k).into_iter().flatten())
                            .any(|e| {
                                e.pos.iter().any(|p| {
                                    p.split(|c: char| !c.is_alphabetic())
                                        .any(|w| w.starts_with("noun"))
                                })
                            })
                    })
                {
                    break;
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
                    // Contracted てしまう/でしまう (炒めちゃう -> 炒める):
                    // after a verb, ちゃ/じゃ (and their ちま/じま cousins,
                    // with or without emphatic sokuon) is the contraction,
                    // never a new word — continue instead of breaking, like
                    // てる does. Longest-first still falls back when the
                    // merged span doesn't resolve.
                    {
                        let bare: String = tok
                            .surface
                            .chars()
                            .take(
                                tok.surface.chars().count()
                                    - usize::from(tok.surface.ends_with('っ')),
                            )
                            .collect();
                        if matches!(bare.as_str(), "ちゃ" | "じゃ" | "ちま" | "じま") {
                            let e = tok.end.min(len);
                            if e > position {
                                ends.push(e);
                            }
                            continue;
                        }
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
                        // One kana of the verb glued into the mis-segmented
                        // noun that follows it (話しとく|し MeCab reads as
                        // 話し|と|くし): the first character is the verb's
                        // own continuation, so the span may end one character
                        // into the noun when that spelling resolves
                        // (話しとく -> 話す). Single-kana nouns only: a real
                        // multi-kana noun (した|にんじん) is a new phrase,
                        // not a shredded fragment — without this, any word
                        // the head happens to spell (したに -> 下煮) swallows
                        // the noun. Longest-first still prefers longer spans,
                        // and the noun itself stays reachable from its own
                        // cursor.
                        let sub = tok.start + 1;
                        if sub > position && tok.surface.chars().count() == 1 {
                            let head: String = chars[position..sub].iter().collect();
                            if lookup_candidate(
                                &head,
                                index,
                                decon,
                                context_reading,
                                morph_base,
                                tokens,
                                position,
                            )
                            .is_some()
                            {
                                ends.push(sub);
                            }
                        }
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
                // Wrapper-fallback wins (honorific お/ご prefix, small-vowel
                // coda strip) name how the STEM resolves, not a tail of this
                // candidate — so the label-keyed tail preferences below must
                // not re-strip them (おまたせ's "imperative + short
                // causative" would take the せ off the honorific-prefixed
                // candidate and fall back to おまた -> お股).
                let wrapper_win = deconj_info.as_deref().map_or(false, |d| {
                    d.starts_with("honorific") || d.starts_with("emphatic")
                });
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
                if !wrapper_win
                    && deconj_info
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
                                // A strictly more common winner keeps the
                                // whole span (出来ても -> 出来る 950 over
                                // 出来 910/800): shortening to a rarer stem
                                // would name the wrong word, while a rare
                                // or equally-ranked winner still shortens
                                // (諷する 0 < 風船 950; 不快 ties with
                                // itself). The stem stays reachable by
                                // cycling shorter either way. But the stem
                                // must be a real word (literal spelling or
                                // reading match), never a deconjugation or
                                // morphology fragment: しても shortens to し
                                // only as 為る-via-morphology ("masu stem"),
                                // which would delete the ても construction
                                // instead of naming a word.
                                let winner_score = entries
                                    .first()
                                    .map_or(0, |e| priority_score(e));
                                let stem_score =
                                    se.first().map_or(0, |e| priority_score(e));
                                if winner_score > stem_score {
                                    break;
                                }
                                let stem_key = normalize::normalize_text(&stem);
                                let stem_is_word = se.first().map_or(false, |e| {
                                    matches!(
                                        match_kind(&e, &stem_key),
                                        MatchKind::PrimarySpelling
                                            | MatchKind::Spelling
                                            | MatchKind::Reading
                                    )
                                });
                                if !stem_is_word {
                                    break;
                                }
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
                if !wrapper_win
                    && deconj_info
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
                                // Word-split, not substring: "adverb"
                                // contains "verb", which used to veto
                                // adverb stems (そう) outright.
                                let stem_is_word = se.first().map_or(false, |e| {
                                    !e.pos.iter().any(|p| {
                                        p.split(|c: char| !c.is_alphabetic())
                                            .any(|w| w == "verb")
                                    })
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
                // Contraction-stem preference (そうしちゃい -> そう, not
                // 奏する): the win came through a てしまう/でしまう
                // contraction ("ended up" — the "contracted" chain detail is
                // jargon-swallowed before post-rules run), but the stem
                // alone is a complete non-verb word — resolve as the stem
                // (the contraction tail stays reachable on hover). The し
                // belongs to the stem only for real する-compounds, which
                // the winner's own POS tells apart (奏する is a suru verb,
                // 指す is not — so 指しちゃう keeps 指し and skips as a
                // verb). Verb stems (食べちゃう -> 食べる), sokuon stems
                // (買っちゃった), bare しちゃう, and literal ちゃ-matches
                // (抹茶) never trigger this.
                if !wrapper_win
                    && deconj_info
                        .as_deref()
                        .map_or(false, |d| d.contains("ended up"))
                {
                    const CONTRACTION_SUFFIXES: &[&str] = &[
                        "ちゃって", "ちゃった", "ちゃわない", "ちゃいます", "ちゃおう",
                        "ちゃえる", "ちゃえ", "ちゃお", "ちゃい", "ちゃう",
                        "じゃって", "じゃった", "じゃわない", "じゃいます", "じゃおう",
                        "じゃえる", "じゃえ", "じゃお", "じゃい", "じゃう",
                        "ちまって", "ちまった", "ちまわない", "ちまいます", "ちまおう",
                        "ちまえる", "ちまえ", "ちまお", "ちまい", "ちまう",
                        "じまって", "じまった", "じまわない", "じまいます", "じまおう",
                        "じまえる", "じまえ", "じまお", "じまい", "じまう",
                    ];
                    for suffix in CONTRACTION_SUFFIXES {
                        if !candidate.ends_with(suffix) || candidate.len() <= suffix.len() {
                            continue;
                        }
                        let mut stem: String = candidate
                            .chars()
                            .take(candidate.chars().count() - suffix.chars().count())
                            .collect();
                        // A trailing し is the する-stem only when the win
                        // itself is a suru verb (そうしちゃい -> そう via
                        // 奏する); otherwise it belongs to the stem
                        // (指しちゃう -> 指し, a verb, skips below).
                        let winner_is_suru = entries.first().map_or(false, |e| {
                            e.pos.iter().any(|p| p.contains("suru verb"))
                        });
                        if stem.ends_with('し') && winner_is_suru {
                            stem = stem
                                .chars()
                                .take(stem.chars().count() - 1)
                                .collect();
                        }
                        if stem.chars().count() < 2
                            || stem.ends_with('て')
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
                                // A verb stem keeps its verb (炒めちゃう ->
                                // 炒める): only non-verb stems (そう, お菓子)
                                // shorten here. Otherwise a real verb stem
                                // that happens to be a key (炒め as a dish)
                                // would split the contraction it heads.
                                let cursor_is_verb = token_at_pos.map_or(false, |t| {
                                    t.start == position && t.pos == "動詞"
                                });
                                if cursor_is_verb {
                                    break;
                                }
                                // Word-split, not substring (see causative
                                // rule above): adverb stems count as words.
                                let stem_is_word = se.first().map_or(false, |e| {
                                    !e.pos.iter().any(|p| {
                                        p.split(|c: char| !c.is_alphabetic())
                                            .any(|w| w == "verb")
                                    })
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
                // Trailing emphatic-sokuon preference (ですっ -> です,
                // すっ -> す, いいよっ -> いい): the win came via
                // deconjugation, but an emphatic っ never belongs to a
                // deconjugated word. Re-resolve the stem, then the stem
                // minus one trailing particle (いいよ -> いい + よ): a
                // literal remainder wins outright, otherwise the span dies
                // so longest-first falls back. Literal winners (あっ, って)
                // never reach here. Stems ending in て/で/っ continue a
                // conjugation (行っ is a sokuon stem), like the even-if
                // rule above.
                {
                    let last = candidate.chars().next_back();
                    // Wrapper wins (お待たせっ -> 待たせる via honorific)
                    // shorten the same way so the emphatic っ doesn't stay
                    // in the span — but only onto the bare stem (no
                    // particle-strip fallback) and only when the stem names
                    // the same top entry; otherwise the wrapper answer stays
                    // whole instead of dying like non-wrapper spans do.
                    if deconj_info.is_some()
                        && (last == Some('っ') || last == Some('ッ'))
                    {
                        let stem: String = candidate
                            .chars()
                            .take(candidate.chars().count() - 1)
                            .collect();
                        let stem_continues = stem.is_empty()
                            || stem.ends_with('て')
                            || stem.ends_with('で')
                            || stem.ends_with('っ')
                            || stem.ends_with('ッ');
                        if !stem_continues {
                            // Candidate remainders, longest first: the stem
                            // itself, then the stem minus a trailing
                            // particle (a literal win anywhere adopts).
                            // Wrapper wins only try the bare stem.
                            let mut remainders = vec![stem.clone()];
                            if !wrapper_win {
                                if let Some(p) = stem.chars().next_back() {
                                    if SOKUON_TAIL_PARTICLES.contains(&p) {
                                        remainders.push(
                                            stem.chars()
                                                .take(stem.chars().count() - 1)
                                                .collect(),
                                        );
                                    }
                                }
                            }
                            let mut adopted = false;
                            for remainder in remainders {
                                if remainder.is_empty() {
                                    continue;
                                }
                                if let Some((se, si)) = lookup_candidate(
                                    &remainder,
                                    index,
                                    decon,
                                    context_reading,
                                    morph_base,
                                    tokens,
                                    position,
                                ) {
                                    // Adopt only literal remainders: the
                                    // deconjugation already spoke for the
                                    // longer surface and lost credibility.
                                    // Wrapper wins additionally adopt a
                                    // deconjugated stem when it names the
                                    // same top entry (お待たせっ -> お待たせ,
                                    // still 待たせる).
                                    let same_top = se.first().map_or(false, |e| {
                                        entries.first().map_or(false, |f| e.id == f.id)
                                    });
                                    if si.is_none() || (wrapper_win && same_top) {
                                        eff_end =
                                            position + remainder.chars().count();
                                        candidate = remainder;
                                        entries = se;
                                        deconj_info = si;
                                        adopted = true;
                                        break;
                                    }
                                }
                            }
                            if !adopted && !wrapper_win {
                                // Nothing literal underneath: kill the span
                                // so shorter spans win. (Found is not
                                // incremented — a killed candidate must not
                                // consume a skip.) Wrapper wins stay whole
                                // instead — their answer came from the
                                // wrapped stem, not the っ.
                                continue;
                            }
                        }
                    }
                }
                // Negative-polarity も (少しも食べない vs 少しもらって):
                // 少しも is only a word under negation — without a negative
                // (ない/ず/ぬ/まい/ません) following in the clause, the も
                // belongs to what follows (少し|も|もらって), and keeping the
                // merge strands もらっ into らって garbage. Shorten to 少し
                // when no negation follows; the negative case keeps merging
                // since 少しも + ない is the real construction.
                if candidate == "少しも"
                    && entries.first().map_or(false, |e| {
                        e.spellings.iter().any(|s| s == "少しも")
                    })
                {
                    let negated = tokens
                        .iter()
                        .filter(|t| t.start > position && t.start < position + 12)
                        .any(|t| {
                            t.base_form == "ない"
                                || matches!(t.surface.as_str(), "ず" | "ぬ" | "まい" | "ません")
                                || matches!(t.base_form.as_str(), "ぬ" | "まい")
                        });
                    if !negated {
                        let stem = "少し".to_string();
                        if let Some((se, si)) = lookup_candidate(
                            &stem,
                            index,
                            decon,
                            context_reading,
                            morph_base,
                            tokens,
                            position,
                        ) {
                            if !se.is_empty() {
                                eff_end = position + stem.chars().count();
                                candidate = stem;
                                entries = se;
                                deconj_info = si;
                            }
                        }
                    }
                }
                // Obscure-literal preference (そこに -> そこ, not 底荷;
                // さんと -> さん, not 三都; ものは -> もの, not もの派;
                // あると -> ある, not アルト; さんが -> さん, not 山河):
                // the winner splits at a token boundary into a stem that
                // resolves to a sufficiently more common word of a
                // DIFFERENT entry (margin 50 on priority score). Pure
                // frequency ties (なぜか 950/950, どうか 950/950, そうか
                // within 40 either way) stay whole. Same-identity
                // extensions (くせに -> 癖, ために -> 為, ところで -> 所,
                // 残念ながら -> 残念, 今日, 食べ物) stay whole, as do
                // conjugations (literal winners only) and standalone-な
                // tails (owned by the rentaikei rule below). Continuative
                // particles (ながら/たり/だり/がてら/つつ) never split
                // either — they inflect the verb rather than casing a noun.
                // Spans built by a fixed-expression completion never split:
                // the COMPLETIONS table deliberately merged them (まずは,
                // いいよ, もう一つ), and fragmenting the winner back into
                // the head re-fragments the construction itself.
                if deconj_info.is_none() && !completion_ends.contains(&eff_end)
                {
                    // Fixed completions (な+んで -> なんで, になると):
                    // the head is a locked function word (助詞/助動詞) and the
                    // tail a particle. Splitting such a candidate into the
                    // function-word leaf (なんで -> な) re-fragments the very
                    // construction the COMPLETIONS table merges, so the
                    // obscure-literal split never applies to them.
                    let completion_head = token_at_pos.map_or(false, |t| {
                        matches!(
                            t.pos.as_str(),
                            "助詞" | "助動詞" | "接続詞"
                        )
                    });
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
                        // Copula tails (こうだ -> こう, not 好打): the same
                        // obscure-literal preference applies when the tail is
                        // a だ/です auxiliary — a coincidental copula word
                        // must not swallow a common stem. Same-entry and
                        // margin rules below protect real copula words
                        // (そうだ hearsay, したんだ explanatory): only
                        // obscure winners split. ます/たい/ない/れる tails
                        // never take this arm.
                        let copula_tail = last.pos == "助動詞"
                            && matches!(last.base_form.as_str(), "だ" | "です");
                        if !na_owned
                            && ((last.pos == "助詞" && !continuative) || copula_tail)
                            && !completion_head
                        {
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
                                    let stem_best = se
                                        .iter()
                                        .map(|e| priority_score(e))
                                        .max()
                                        .unwrap_or(0);
                                    let winner_score =
                                        priority_score(&entries[0]);
                                    let same_entry = se
                                        .iter()
                                        .any(|e| e.id == entries[0].id);
                                    // Same-lemma keep: the winner is the stem
                                    // word plus one particle (どうか = 如何
                                    // + か, すぐに = 直ぐ + に) of the same
                                    // coarse POS class and genuinely attested
                                    // — splitting only obscures it. Classes
                                    // are coarse (adverbial covers adverb +
                                    // keiyodoshi) but verb/noun/numeric/suffix
                                    // stay distinct, so orphan winners
                                    // (もの派, 底荷) and mismatched ones
                                    // (アルト noun vs 有る verb, 山河 noun vs
                                    // 三 numeric) still split below.
                                    let tail: String =
                                        chars[last.start..eff_end].iter().collect();
                                    let same_lemma_keep = tail.chars().count() == 1
                                        && winner_score != 0
                                        && entries[0]
                                            .readings
                                            .iter()
                                            .flat_map(|r| {
                                                normalize::normalize_variants(r)
                                            })
                                            .any(|f| f == format!("{stem}{tail}"))
                                        && se.first().map_or(false, |s| {
                                            pos_class(
                                                s.pos.first()
                                                    .map(|x| x.as_str())
                                                    .unwrap_or(""),
                                            ) == pos_class(
                                                entries[0]
                                                    .pos
                                                    .first()
                                                    .map(|x| x.as_str())
                                                    .unwrap_or(""),
                                            )
                                        });
                                    // Copula-attached grammatical expressions stay
                                    // whole (そうだ hearsay, わけだ, はずだ):
                                    // the winner names the construction itself,
                                    // not a coincidental homophone.
                                    let winner_is_expression = copula_tail
                                        && entries[0].pos.iter().any(|p| {
                                            p.contains("expression")
                                        });
                                    if !same_entry
                                        && !same_lemma_keep
                                        && !winner_is_expression
                                        && stem_best.saturating_sub(winner_score)
                                            >= 50
                                    {
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
                            // Na-adjective stems shorten (変, 静か…). So do
                            // noun/adjective/adverb stems with no verb
                            // homograph (たより, 本当, 残念…): the な is the
                            // copula rentaikei, not part of the word — this
                            // is what keeps たよりな from resolving to
                            // 頼りない via the "imperative negative" な rule.
                            // Verb-containing stems (食べ, あり, そう via
                            // 沿う, よう via 酔う, 好き via 梳く is still
                            // covered by the na-adj arm) keep the whole
                            // span: the な may be prohibitive/imperative
                            // (食べな -> 食べる) or an adnominal whose verb
                            // homograph outranks it (ような -> 様な).
                            // Word-split (not substring) so "adverb" never
                            // counts as "verb".
                            let stem_is_na_adj = se.iter().any(|e| {
                                e.pos.iter().any(|p| p.contains("keiyodoshi"))
                            });
                            let stem_has_verb = se.iter().any(|e| {
                                e.pos.iter().any(|p| {
                                    p.split(|c: char| !c.is_alphabetic())
                                        .any(|w| w == "verb")
                                })
                            });
                            // A label-less win reached ONLY by deconjugation
                            // (the rule chain was pure jargon, e.g. the bare
                            // "stem" that turns たよりな into 頼りない) means
                            // the deconjugation added no meaning — the な is
                            // the copula even when the stem has a verb
                            // homograph. Genuine imperatives always name
                            // themselves ("casual polite imperative"), and
                            // literal winners (ような -> 様な) are exempt by
                            // kind.
                            let winner_deconj_only =
                                deconj_info.is_none()
                                    && normalize::normalize_variants(&candidate)
                                        .iter()
                                        .map(|k| match_kind(&entries[0], k))
                                        .min()
                                        == Some(MatchKind::Deconjugated);
                            if !se.is_empty()
                                && (stem_is_na_adj
                                    || !stem_has_verb
                                    || winner_deconj_only)
                            {
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
                // Explanatory-ん split (事なん -> 事, いいん -> いい,
                // どうなん -> どう): the span ends on a standalone
                // explanatory ん (名詞, base ん) whose copula follows, and the
                // stem before the copula run resolves on its own — so the win
                // is a coincidental homophone of stem + な + ん reached by a
                // reading or a deconjugation (ことなん -> 異なる via slurred,
                // いいん -> 委員, どうなん -> 童男), not a word of its own.
                // The obscure-literal margin above never fires here (いい 950
                // vs 委員 990) and its branch requires the last token to be a
                // 助詞, which 名詞-ん is not — so this rule keys on the token
                // shape instead: stem + copula run (な/だ/で/じゃ) + explanatory
                // ん, then the copula that makes the ん explanatory. Spelled
                // winners (a real word the surface actually spells) and
                // same-entry stems keep the whole span, as do function-word
                // stems (a lone particle plus ん is its own construction).
                {
                    let ending = tokens
                        .iter()
                        .filter(|t| t.start > position && t.end == eff_end)
                        .last();
                    if let Some(n) = ending.filter(|t| {
                        matches!(t.pos.as_str(), "名詞" | "助詞")
                            && t.surface == "ん"
                            && t.base_form == "ん"
                    }) {
                        // Copula family: だ/な/で/だろ/だった (base だ), じゃ
                        // (base じゃ), です/でしょう/でし (base です). The じゃ
                        // after an explanatory ん is tagged 助動詞 with base じゃ,
                        // while じゃ elsewhere is a 助詞 — so the base test alone
                        // decides here.
                        let next_is_copula = tokens
                            .iter()
                            .find(|t| t.start == n.end)
                            .map_or(false, |t| {
                                t.pos == "助動詞"
                                    && matches!(t.base_form.as_str(), "だ" | "じゃ" | "です")
                            });
                        let stem_is_content = tokens.iter().any(|t| {
                            t.start == position
                                && !matches!(t.pos.as_str(), "助詞" | "助動詞" | "接続詞")
                        });
                        if next_is_copula && stem_is_content {
                            // Walk back over the copula run (事|な|ん -> 事,
                            // いい|ん -> いい): only the copula だ-inflations
                            // between the stem and the ん are absorbed.
                            let mut stem_start = n.start;
                            while let Some(t) = tokens
                                .iter()
                                .find(|t| t.start >= position && t.end == stem_start)
                            {
                                if t.pos == "助動詞" && t.base_form == "だ" {
                                    stem_start = t.start;
                                } else {
                                    break;
                                }
                            }
                            if stem_start > position {
                                let stem: String = chars[position..stem_start].iter().collect();
                                if let Some((se, si)) = lookup_candidate(
                                    &stem,
                                    index,
                                    decon,
                                    context_reading,
                                    morph_base,
                                    tokens,
                                    position,
                                ) {
                                    let same_entry = se.iter().any(|e| e.id == entries[0].id);
                                    // Reading/deconjugated winners only: a
                                    // surface the dictionary spells outright
                                    // is the word, whatever the tokens say.
                                    let winner_kind = normalize::normalize_variants(&candidate)
                                        .iter()
                                        .map(|k| match_kind(&entries[0], k))
                                        .min();
                                    if !se.is_empty()
                                        && !same_entry
                                        && winner_kind.map_or(false, |k| k >= MatchKind::Reading)
                                    {
                                        eff_end = stem_start;
                                        candidate = stem;
                                        entries = se;
                                        deconj_info = si;
                                    }
                                }
                            }
                        }
                    }
                }
                // Verb + explanatory ん (もうそんな時間 -> 申す): MeCab reads
                // もうそ|ん|な and the whole run slurred-deconjugates back to
                // the verb (もうそんな -> もうそ -> 申す), so a phrase that is
                // really もう + そんな resolves as one nonsense word. The
                // compound is a slurred reading of the verb, not a word of
                // its own: shorten to the longest SUB-END of the verb token
                // that names a different word (もう), so the scan resumes
                // mid-token and そんな can form. Sub-ends under two
                // characters are the verb's own head (言う|ん -> 言, both read
                // いう) and keep the compound, as do same-reading sub-ends
                // and the standard explanatory copula (verb + ん + だ never
                // splits).
                {
                    let ctok = tokens.iter().find(|t| t.start == position);
                    if ctok.map_or(false, |t| {
                        t.pos == "動詞" && t.base_form != t.surface
                    }) {
                        let ctok = ctok.unwrap();
                        if let Some(n) = tokens.iter().find(|t| {
                            t.start > position
                                && t.start < eff_end
                                && t.surface == "ん"
                                && t.base_form == "ん"
                        }) {
                            let after_is_copula = tokens
                                .iter()
                                .find(|t| t.start == n.end)
                                .map_or(false, |t| {
                                    t.pos == "助動詞"
                                        && matches!(
                                            t.base_form.as_str(),
                                            "だ" | "じゃ" | "です"
                                        )
                                });
                            if !after_is_copula {
                                for e in ((position + 1)..ctok.end).rev() {
                                    let stem: String = chars[position..e].iter().collect();
                                    if stem.chars().count() < 2 {
                                        break;
                                    }
                                    let Some((se, si)) = lookup_candidate(
                                        &stem,
                                        index,
                                        decon,
                                        context_reading,
                                        morph_base,
                                        tokens,
                                        position,
                                    ) else {
                                        continue;
                                    };
                                    let same_reading = ctok.reading.is_empty()
                                        || se.iter().any(|e2| {
                                            e2.readings.iter().any(|r| {
                                                normalize::normalize_variants(r)
                                                    .iter()
                                                    .any(|v| v == &ctok.reading)
                                            })
                                        });
                                    if !same_reading {
                                        eff_end = e;
                                        candidate = stem;
                                        entries = se;
                                        deconj_info = si;
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                // Stem + explanatory ん + conjecture (良いんだろうか -> 良い,
                // いいんだろうけど -> いい): the whole run resolves to a
                // coincidental word (良いんだろうか -> 余韻, いいんだろう ->
                // 委員 + conjecture), so shorten to the stem like the
                // explanatory-ん split does — the ん keeps its span and
                // だろう/けど resolve after it. The ん is found in the
                // surface itself (not just as a standalone token, since
                // んだろう/んだ may tokenize fused) — but a token must
                // start there, so mid-word ん (ふんだろう's 分) never
                // matches. The stem walks back over a copula run
                // (相手なんだろう -> 相手 + なん + だろう), must resolve to
                // a different entry, and the winner must be
                // reading-or-deconjugation reached (spelled winners keep the
                // span). The remainder after the ん has to open with the
                // conjecture (んだろ/んでしょ, covering だろう/だろうか/
                // けど…, でしょう…): plain んだ/んです/んじゃない/んだよ
                // tails never match, and spans without an ん (何だろう) or
                // without a stem (bare んだろう) are untouched.
                {
                    let nposs: Vec<usize> = (position + 1..eff_end)
                        .filter(|&i| {
                            chars[i] == 'ん' && tokens.iter().any(|t| t.start == i)
                        })
                        .collect();
                    for i in nposs {
                        let rest: String = chars[i..eff_end].iter().collect();
                        if !(rest.starts_with("んだろ") || rest.starts_with("んでしょ")) {
                            continue;
                        }
                        let mut stem_start = i;
                        while let Some(t) = tokens
                            .iter()
                            .find(|t| t.start >= position && t.end == stem_start)
                        {
                            if t.pos == "助動詞" && t.base_form == "だ" {
                                stem_start = t.start;
                            } else {
                                break;
                            }
                        }
                        if stem_start <= position {
                            continue;
                        }
                        let stem: String = chars[position..stem_start].iter().collect();
                        if let Some((se, si)) = lookup_candidate(
                            &stem,
                            index,
                            decon,
                            context_reading,
                            morph_base,
                            tokens,
                            position,
                        ) {
                            let same_entry = se.iter().any(|e| e.id == entries[0].id);
                            let winner_kind = normalize::normalize_variants(&candidate)
                                .iter()
                                .map(|k| match_kind(&entries[0], k))
                                .min();
                            if !se.is_empty()
                                && !same_entry
                                && winner_kind.map_or(false, |k| k >= MatchKind::Reading)
                            {
                                eff_end = stem_start;
                                candidate = stem;
                                entries = se;
                                deconj_info = si;
                                break;
                            }
                        }
                    }
                }
                // Adverb + したい (どうしたい -> どう + したい): the run
                // resolves to a coincidental noun (どうしたい never wins —
                // どうし -> 動詞/同志 does), hiding the どうする "what to
                // do" construction. Only どう/そう/こう followed by したい
                // (する + want-to): して-forms are excluded on purpose
                // (そうして is そして). The したい remainder is read from
                // the stem end (it usually extends past the winning span)
                // and must resolve to する on its own, or the span stays.
                {
                    let adv = token_at_pos.filter(|t| {
                        t.start == position
                            && t.pos == "副詞"
                            && matches!(t.surface.as_str(), "どう" | "そう" | "こう")
                    });
                    if let Some(a) = adv {
                        let stem_len = a.surface.chars().count();
                        if candidate.chars().count() > stem_len {
                            let rest: String = chars[a.end..len].iter().collect();
                            if rest.starts_with("したい") {
                                if let Some((se, _)) = lookup_candidate(
                                    "したい",
                                    index,
                                    decon,
                                    context_reading,
                                    morph_base,
                                    tokens,
                                    a.end,
                                ) {
                                    let to_suru = se.iter().any(|e| {
                                        e.readings.iter().any(|r| r == "する")
                                            && e.pos.iter().any(|p| p.contains("suru verb"))
                                    });
                                    if to_suru {
                                        if let Some((stem_e, stem_l)) = lookup_candidate(
                                            &a.surface,
                                            index,
                                            decon,
                                            context_reading,
                                            morph_base,
                                            tokens,
                                            position,
                                        ) {
                                            eff_end = a.end;
                                            candidate = a.surface.clone();
                                            entries = stem_e;
                                            deconj_info = stem_l;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Fused sentence particles (部はね -> は + ね, 猫よね, 嫌だよ):
                // MeCab fuses the particle run into one token — often
                // mis-tagged as a verb stem (はね -> 跳ねる via morphology) —
                // while the whole reads as a coincidental word. Shorten to
                // the particle part. Three guards keep real words whole:
                // the fusion must directly follow a noun (a particle/
                // auxiliary before it, a sentence start, or a continuative
                // comma means the stem is real: うさぎが跳ね, 跳ね、止まれ),
                // the ね/よ/わ must be sentence-final (punctuation or EOS —
                // a following auxiliary keeps the stem: かねない, 跳ねた),
                // and the head must itself be a function word (買わ, ほね,
                // 死ね keep their stems). Real fused words never end in
                // ね/よ/わ (では, かな, っけ...), and dictionary-listed
                // particles/interjections (よね, うわ) keep their entries.
                {
                    let tchars: Vec<char> = candidate.chars().collect();
                    let single_fused = token_at_pos.filter(|t| {
                        t.start == position
                            && t.end == eff_end
                            && matches!(
                                t.pos.as_str(),
                                "助詞" | "助動詞" | "接続詞" | "感動詞" | "動詞"
                            )
                    });
                    // Multi-token fusion (カズちゃんとらぶらぶ): UniDic
                    // keeps the suffix and particle separate (ちゃん|と)
                    // where IPAdic fused them (ちゃんと), so the single-token
                    // arm above never fires. The と/ね/よ/わ tail is wider
                    // here on purpose: a suffix head is never a verb stem,
                    // so there is no 跳ねた-style ambiguity to protect.
                    let suffix_fused = token_at_pos.filter(|t| {
                        t.start == position
                            && t.pos == "接尾辞"
                            && eff_end == t.end + 1
                            && matches!(tchars.last(), Some('と' | 'ね' | 'よ' | 'わ'))
                            && tokens.iter().any(|x| {
                                x.start == t.end && x.end == t.end + 1 && x.pos == "助詞"
                            })
                    });
                    let fused = single_fused.or(suffix_fused);
                    // The word the fusion attaches to: a directly-adjacent
                    // noun (pronouns live under 名詞) or auxiliary
                    // (兼ねないよ, だよ, ですね). Anything else before it —
                    // a particle (うさぎが跳ね), a verb/adjective/adverb
                    // (大きく跳ね), a comma, or sentence start — means the
                    // stem is real.
                    let prev_is_host = tokens
                        .iter()
                        .filter(|t| t.end <= position)
                        .last()
                        .map_or(false, |p| {
                            p.end == position && matches!(p.pos.as_str(), "名詞" | "助動詞")
                        });
                    // Sentence-final ね/よ/わ: punctuation or end of text
                    // follows (anything else continues the word).
                    let tail_is_final = tokens
                        .iter()
                        .find(|t| t.start == eff_end)
                        .map_or(true, |t| t.pos == "記号");
                    // Suffix-arm tails split on phrase boundaries, not just
                    // sentence-finality: ちゃんと+verb keeps the adverb whole
                    // (犬ちゃんと遊ぶ, ちゃんとした), while ちゃんと+noun
                    // splits the suffix off (カズちゃんとらぶらぶ). A new
                    // phrase starts at nouns, pronouns, punctuation, and end
                    // of text; verbs, auxiliaries, particles and the rest
                    // continue the adverb's phrase.
                    let suffix_new_phrase = suffix_fused.is_some()
                        && tokens
                            .iter()
                            .find(|t| t.start == eff_end)
                            .map_or(true, |t| {
                                matches!(t.pos.as_str(), "名詞" | "代名詞" | "記号")
                            });
                    let single_tail_ok = single_fused.is_some()
                        && matches!(tchars.last(), Some('ね' | 'よ' | 'わ'))
                        && tail_is_final;
                    let suffix_tail_ok = suffix_fused.is_some()
                        && matches!(tchars.last(), Some('と' | 'ね' | 'よ' | 'わ'))
                        && (tail_is_final || suffix_new_phrase);
                    if fused.is_some()
                        && tchars.len() >= 2
                        && (single_tail_ok || suffix_tail_ok)
                        && prev_is_host
                        && !entries.iter().any(|e| {
                            e.pos.iter().any(|p| {
                                p.contains("particle") || p.contains("interjection")
                            })
                        })
                    {
                        let winner_kind = normalize::normalize_variants(&candidate)
                            .iter()
                            .map(|k| match_kind(&entries[0], k))
                            .min();
                        if matches!(
                            winner_kind,
                            Some(MatchKind::Reading)
                                | Some(MatchKind::Deconjugated)
                                | Some(MatchKind::Morphological)
                        ) {
                            let head: String =
                                tchars[..tchars.len() - 1].iter().collect();
                            if let Some((se, si)) = lookup_candidate(
                                &head,
                                index,
                                decon,
                                context_reading,
                                morph_base,
                                tokens,
                                position,
                            ) {
                                let head_is_function = se.iter().any(|e| {
                                    e.pos.iter().any(|p| {
                                        p.contains("particle")
                                            || p.contains("auxiliary")
                                            || p.contains("copula")
                                            || p.contains("conjunction")
                                            // Split-off suffixes (ちゃん from
                                            // ちゃんと): the head names a
                                            // bound morpheme, not a word.
                                            || p.contains("suffix")
                                    })
                                });
                                if !se.is_empty() && head_is_function {
                                    eff_end -= 1;
                                    candidate = head;
                                    entries = se;
                                    deconj_info = si;
                                }
                            }
                        }
                    }
                }
                // Bare-し suru absorption before a とく-continuation
                // (おまけしといた -> おまけ + しといた): the suru path
                // absorbs a lone し stem plus nothing, swallowing the split
                // the といた/とく/とけ tail needs (おまけし -> おまけ +
                // "suru"). When exactly one し follows the noun and a
                // とく-form continues, keep the noun alone so しといた
                // resolves on its own. Same-entry gated: the noun alone
                // must name the same word.
                {
                    let noun_tok = tokens.iter().find(|t| t.start == position);
                    if let Some(noun) = noun_tok.filter(|t| t.pos == "名詞") {
                        if eff_end == noun.end + 1 && chars.get(noun.end) == Some(&'し') {
                            let after: String = chars
                                .get(eff_end..(eff_end + 2).min(len))
                                .unwrap_or(&[])
                                .iter()
                                .collect();
                            if matches!(after.as_str(), "とい" | "とく" | "とけ") {
                                let stem: String =
                                    chars[position..noun.end].iter().collect();
                                if let Some((se, si)) = lookup_candidate(
                                    &stem,
                                    index,
                                    decon,
                                    context_reading,
                                    None,
                                    tokens,
                                    position,
                                ) {
                                    let same_entry = se.first().map_or(false, |e| {
                                        entries.first().map_or(false, |f| e.id == f.id)
                                    });
                                    if !se.is_empty() && same_entry {
                                        eff_end = noun.end;
                                        candidate = stem;
                                        entries = se;
                                        deconj_info = si;
                                    }
                                }
                            }
                        }
                    }
                }
                // Sentence-final particle after an auxiliary (兼ねない|よ,
                // だ|よ, です|ね): the particle starts its own span instead
                // of gluing to the inflection, so both stay hoverable. Only
                // auxiliaries split: nouns/pronouns/adverbs/adjectives
                // (これ|よ, いい|よ, すごい|よ, ごめん|ね), te-forms
                // (食べて|よ), conditionals (ば|よかった), continuatives
                // (ながら|よ), contractions (ちゃう|よ) and verbs
                // (来い|よ, しろ|よ) all keep whole — as do sentence-final
                // う/か/な/っけ/かな/かしら shapes, which never match.
                {
                    let last_tok = tokens.iter().find(|t| t.end == eff_end);
                    let ends_particle = last_tok.map_or(false, |t| {
                        t.start == eff_end - 1
                            && t.pos == "助詞"
                            && matches!(t.surface.as_str(), "ね" | "よ" | "わ")
                    });
                    if ends_particle {
                        let prev_is_aux = tokens
                            .iter()
                            .find(|t| t.end == eff_end - 1)
                            .map_or(false, |t| t.pos == "助動詞");
                        if prev_is_aux && eff_end - 1 > position {
                            let stem: String =
                                chars[position..eff_end - 1].iter().collect();
                            if let Some((se, si)) = lookup_candidate(
                                &stem,
                                index,
                                decon,
                                context_reading,
                                morph_base,
                                tokens,
                                position,
                            ) {
                                if !se.is_empty() {
                                    eff_end -= 1;
                                    candidate = stem;
                                    entries = se;
                                    deconj_info = si;
                                }
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
