//! Candidate lookup for a single span: literal matches, the
//! tokenizer's morphological base form, suru-noun and compound-verb
//! constructions, rule-based deconjugation, copula/conjecture/nanda tail
//! stripping, honorific/emphatic/chouonpu fallbacks, and final ranking.

use crate::deconjugate::{
    combined_label, deconj_tag_matches_entry, is_verb_class, DeconjugatedForm, Deconjugator,
};
use crate::index::DictionaryIndex;
use crate::normalize::{self, normalize_variants};
use crate::rank::{is_bound_only, match_kind, priority_score, reading_matches_context};
use crate::rules::{
    CONJECTURE_TAILS, CONTRACTION_AUX_VERBS, COPULA_TAILS, NANDA_ADVERBIAL_TAILS, NANDA_TAILS,
    SHIYAGARU_FORMS, TAIL_AUX_LABELS, TE_AUX_VERBS,
};
use crate::types::{DictEntry, MatchKind, MorphToken};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;


/// Tries every normalized variant of `candidate` (there can be more than
/// one due to chouonpu ambiguity — see normalize::chouonpu_variants)
/// against the dictionary index: literal match first, then deconjugation.
/// Deconjugation uses the JL/Nazeka engine, which records each resolved
/// form with the fewest proper rule steps per (text, word class), and the
/// resulting word class is validated against each entry's POS (JL's
/// GetValidDeconjugatedResults) so a coincidental conjugation can't surface
/// a wrong homograph.

pub(crate) fn lookup_candidate(
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
                // Fall back to surface deconjugation instead. The same holds
                // for a trailing partial token: its reading is only half
                // covered (今帰 inside 今|帰り reads こん, which deconjugates
                // to こる -> 凝る), so the reading is only honest when the span
                // ends exactly on a token boundary.
                let ends_on_token = in_span
                    .last()
                    .map_or(false, |t| t.end == position + span_len);
                if in_span.is_empty()
                    || !ends_on_token
                    || in_span.iter().any(|t| t.reading.is_empty())
                {
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
        // Trailing emphatic sokuon (お待たせっ -> 待たせる): deconjugate
        // the surface/reading minus the final っ as well, so sokuon-final
        // inflections resolve instead of dying (the sokuon-shorten
        // post-rule then lands the span on the stem). Deconjugation only,
        // never literals (マジかっ must not reach 間近), and skipped
        // outright when the stripped form is itself a literal word (its
        // own paths already cover it: いいよ, マジか, てめ).
        {
            let mut stripped_inputs: Vec<String> = Vec::new();
            if let Some(reading) = &span_reading {
                let cs: Vec<char> = reading.chars().collect();
                if matches!(cs.last(), Some('っ') | Some('ッ')) {
                    stripped_inputs.push(cs[..cs.len() - 1].iter().collect());
                }
            }
            {
                let cs: Vec<char> = candidate.chars().collect();
                if matches!(cs.last(), Some('っ') | Some('ッ')) {
                    stripped_inputs.push(cs[..cs.len() - 1].iter().collect());
                }
            }
            stripped_inputs.retain(|s| !s.is_empty());
            let literal_hit = stripped_inputs
                .iter()
                .flat_map(|s| normalize::normalize_variants(s))
                .any(|k| index.by_text.contains_key(&k));
            if !literal_hit {
                for s in &stripped_inputs {
                    for key in normalize::normalize_variants(s) {
                        all.extend(decon.deconjugate(&key));
                    }
                }
            }
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
    // A whole-candidate っちゃ is the topical/contrastive は with sokuon
    // (世話焼いてるっちゃ = 焼いてる + とは/ては). Look it up as は.
    // Longer words containing it (抹茶) never match whole-candidate here.
    if candidate == "っちゃ" && !literal_keys.iter().any(|k| k == "は") {
        literal_keys.push("は".to_string());
    }
    // A whole-candidate てめっ is てめえ ("you", vulgar) with the final
    // え slurred to っ — normalization never maps those, and てめっ has
    // no entry of its own (unlike あっ, so no literal confusion).
    if candidate == "てめっ" && !literal_keys.iter().any(|k| k == "てめえ") {
        literal_keys.push("てめえ".to_string());
    }
    // A whole-candidate しやがった-family is する + the vulgar auxiliary
    // やがる (MeCab shreds it し|や|がっ|た, so it never resolves
    // normally). Look it up as する as well. The same auxiliary on a
    // longer surface (バカにしやがった) is バカにする + やがった: strip the
    // しやがる-family suffix and restore the suru verb, so the dictionary
    // compound (馬鹿にする) resolves whole. Other verbs + やがる
    // (食べやがった) degrade to front-verb + junk — future work.
    if SHIYAGARU_FORMS.contains(&candidate)
        && !literal_keys.iter().any(|k| k == "する")
    {
        literal_keys.push("する".to_string());
    }
    if let Some(tail) = SHIYAGARU_FORMS
        .iter()
        .filter(|s| candidate.ends_with(**s) && candidate.len() > s.chars().count())
        .max_by_key(|s| s.chars().count())
    {
        let stem: String = candidate
            .chars()
            .take(candidate.chars().count() - tail.chars().count())
            .collect();
        let with_suru = format!("{stem}する");
        if !literal_keys.iter().any(|k| k == &with_suru) {
            literal_keys.push(with_suru);
        }
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
    // A trailing chouonpu is often emphatic lengthening, not a different
    // word (メシメシー -> メシー -> 盲, while メシ -> 飯). When the full form
    // reached nothing — or only priority-less orphans like 盲 — also try
    // without the final ー and let frequency decide. Real ー-words
    // (セーラー, ちきしょー -> 畜生) resolve to common entries, so the
    // fallback never fires for them.
    if candidate.ends_with('ー') {
        let all_orphan = candidates
            .iter()
            .all(|(e, _, _, _, _)| priority_score(e) == 0);
        if all_orphan {
            let stripped: String = candidate
                .chars()
                .take(candidate.chars().count().saturating_sub(1))
                .collect();
            if !stripped.is_empty() {
                for key in normalize::normalize_variants(&stripped) {
                    if let Some(entries) = index.by_text.get(&key) {
                        for e in entries {
                            if seen_ids.insert(e.id) {
                                let kind = match_kind(e, &key);
                                let ctx = context_reading
                                    .map_or(false, |r| reading_matches_context(e, r));
                                candidates.push((Arc::clone(e), 0, None, kind, ctx));
                            }
                        }
                    }
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
    //      The tail must be non-empty: `all` on zero tokens is vacuously
    //      true, which used to let single-token misanalyses (いいよっ as
    //      いいよる) hijack morphology ahead of the honest reading match.
    if let Some(base) = morph_base {
        let base_norm = normalize::normalize_text(base);
        let base_ids: HashSet<u32> = index
            .by_text
            .get(&base_norm)
            .map(|es| es.iter().map(|e| e.id).collect())
            .unwrap_or_default();
        // A kana reading deconjugates to kana text while the base form may
        // be kanji (あり -> ある vs 有る), so a reading form also counts
        // when it resolves to the base's dictionary entry — ったく -> 九
        // and いいよっ -> いいよる stay gated since no reading form
        // reaches those entries.
        let via_deconj = reading_forms.iter().any(|f| {
            normalize::normalize_text(&f.text) == base_norm
                || index
                    .by_text
                    .get(&normalize::normalize_text(&f.text))
                    .map_or(false, |es| es.iter().any(|e| base_ids.contains(&e.id)))
        });
        let tail_tokens: Vec<&MorphToken> = tokens
            .iter()
            .filter(|t| t.start >= position && t.start < position + span_len && t.start != position)
            .collect();
        let via_aux_tail = !tail_tokens.is_empty()
            && tail_tokens
                .iter()
                .all(|t| matches!(t.pos.as_str(), "助動詞" | "助詞" | "記号" | "接頭辞" | "接尾辞"));
        if via_deconj || via_aux_tail {
            // Name the deconjugation (e.g. した -> "past") rather than the bare
            // base form in the tooltip. The deconjugation forms are kana while
            // the base form may be kanji (のむ vs 飲む), so they are matched by
            // the dictionary entry they resolve to, not by raw text.
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
        // Shredded okurigana verbs (連んでんだ -> 連|ん|でん) are exempt too:
        // the cursor is a lone kanji and the span ends mid-token in a te-form
        // (て/で/って/んで), which only happens when MeCab shredded a real
        // inflection — trust it like JL does. Token-aligned spans (目で,
        // 木たい) stay gated, so particles and auxiliaries can't hijack.
        let span_end = position + span_len;
        let ends_mid_token = tokens
            .iter()
            .any(|t| t.start < span_end && span_end < t.end);
        let cursor_single_kanji = tokens
            .iter()
            .find(|t| t.start == position)
            .map_or(false, |t| {
                let cs: Vec<char> = t.surface.chars().collect();
                cs.len() == 1 && {
                    let cp = cs[0] as u32;
                    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
                }
            });
        let surface_te_form = candidate.ends_with("て")
            || candidate.ends_with("で")
            || candidate.ends_with("って")
            || candidate.ends_with("んで");
        // Contracted/slurred verbs (なめんな -> なめる via the ん<-る slurred
        // rule) carry no verb token either, but they are genuine colloquial
        // inflections, not phonetic lookalikes like 好きな -> 好く (single
        // step). The chain must contain an actual slurred step; な-imperative
        // lookalikes (好きな/へんな, one step) stay gated.
        let chain_has_slurred = form.rule_chain.as_deref().map_or(false, |c| {
            c.split('→').any(|seg| seg == "slurred")
        });
        // ても/でも/っても conditionals (出来ても -> できる "even if"):
        // the rule itself only ever fires on a te-stem, so a verb-class
        // result through it is genuine even when the tokenizer mistagged
        // the stem as a noun (出来 as 名詞). Noun + ても with no verb
        // continuation (子供でも -> 子供で) reaches nothing verb-classed,
        // so the exemption has nothing to admit there.
        let chain_has_even_if = form.rule_chain.as_deref().map_or(false, |c| {
            c.split('→').any(|seg| seg.contains("even if"))
        });
        if starts_at_position
            && !single_token_span
            && is_verb_class(&form.tag)
            && !span_has_verb_token
            && !(cursor_single_kanji && ends_mid_token && surface_te_form)
            && !chain_has_slurred
            && !chain_has_even_if
        {
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
            // Copula-absorbed stems (noun + だった/たり/じゃない...: 休みだったり
            // -> 休み) name only the stem, while the bare stem also lists its
            // own readings (休み -> 休み + 休む). Continue one explicit step
            // from the absorbed stem so both stay listed: the bare stem span
            // is single-token (verb-gate exempt) and already shows exactly
            // these entries, so this only restores parity — never new words.
            // seen_ids dedupes when the engine already chained this far.
            let absorbed = form.rule_chain.as_deref().map_or(false, |c| {
                c.split('→')
                    .any(|seg| seg == "copula" || seg == "copula + tari" || seg == "past + tari")
            });
            if absorbed {
                let absorption_label = chain_desc.clone();
                for cont in decon.deconjugate(&form.text) {
                    let cont_na_imperative = cont.rule_chain.as_deref().map_or(false, |c| {
                        c.split('→').any(|seg| seg == "casual polite imperative")
                    });
                    if cont_na_imperative {
                        let stem_is_verb = tokens
                            .iter()
                            .find(|t| t.start == position)
                            .map_or(false, |t| t.pos == "動詞");
                        if !stem_is_verb {
                            continue;
                        }
                    }
                    let cont_key = normalize::normalize_text(&cont.text);
                    if let Some(cont_entries) = index.by_text.get(&cont_key) {
                        let cont_desc = cont.rule_chain.as_deref().and_then(combined_label);
                        let desc = match (cont_desc, absorption_label.clone()) {
                            (Some(s), Some(a)) => Some(format!("{s} + {a}")),
                            (None, a) => a,
                            (s, None) => s,
                        };
                        for e in cont_entries {
                            if deconj_tag_matches_entry(&e.pos, &cont.tag)
                                && seen_ids.insert(e.id)
                            {
                                let ctx = context_reading
                                    .map_or(false, |r| reading_matches_context(e, r));
                                candidates.push((
                                    Arc::clone(e),
                                    cont.proper_steps + form.proper_steps + 2,
                                    desc.clone(),
                                    MatchKind::Deconjugated,
                                    ctx,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    // Char-level honorific strip for fused politeness (お待たせっ arriving
    // as one token, so no お-token exists for the token wrapper): strip
    // the first character and deconjugate the rest. Deconjugation-only
    // with a literal-skip gate — common お-words are literal keys (お茶,
    // お金, おこ, おは…) so the feed never fires for them — and skipped
    // outright when an お/ご/御 token sits at the cursor (the token
    // wrapper owns those, labels intact). Labeled honorific like the
    // wrapper, so the sokuon-shorten rule can land the span on the stem.
    // Runs unconditionally (not only when empty): the full span must get
    // its chance before shorter spans win by fallback.
    {
        let first = candidate.chars().next();
        let has_prefix_token = tokens
            .iter()
            .any(|t| t.start == position && matches!(t.surface.as_str(), "お" | "ご" | "御"));
        if matches!(first, Some('お') | Some('ご') | Some('御')) && !has_prefix_token {
            let stripped: String = candidate.chars().skip(1).collect();
            if !stripped.is_empty() {
                let literal_hit = normalize::normalize_variants(&stripped)
                    .iter()
                    .any(|k| index.by_text.contains_key(k));
                if !literal_hit {
                    for key in normalize::normalize_variants(&stripped) {
                        for form in decon.deconjugate(&key) {
                            let fkey = normalize::normalize_text(&form.text);
                            if let Some(fentries) = index.by_text.get(&fkey) {
                                let chain_desc =
                                    form.rule_chain.as_deref().and_then(combined_label);
                                let label = match chain_desc {
                                    Some(l) => Some(format!("honorific + {l}")),
                                    None => Some("honorific".to_string()),
                                };
                                for e in fentries {
                                    if deconj_tag_matches_entry(&e.pos, &form.tag)
                                        && seen_ids.insert(e.id)
                                    {
                                        let ctx = context_reading.map_or(false, |r| {
                                            reading_matches_context(e, r)
                                        });
                                        candidates.push((
                                            Arc::clone(e),
                                            form.proper_steps + 1,
                                            label.clone(),
                                            MatchKind::Deconjugated,
                                            ctx,
                                        ));
                                    }
                                }
                            }
                        }
                    }
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
            // A lone explanatory ん never names the stem (どうなんだろう ->
            // ん + だろう, not んだろう -> ん + conjecture): the ん of the
            // copula belongs to what precedes it, and the tail keeps its
            // own span (だろう) at the next cursor instead.
            if stem == "ん" {
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
            // A literal stem ending in an unstripped copula (日だ from
            // 日だっけ) is usually not a word — it only matches
            // coincidental readings (襞/ひだ, 操舵/そうだ). Require such
            // stems to be spelling-reached, or reading-reached by a
            // genuinely common word (未だ/まだ, 只/ただ and 肌/はだ at
            // 950 absorb っけ; そうだ-hearsay at 800 does not, so
            // そうだっけ splits to そう like そうは does). The explanatory
            // のだ/んだ are grammatical infrastructure, exempt, as is
            // single-char だ (so だっけ still reaches だ).
            let stem_contaminated = {
                let n = stem.chars().count();
                n > 1
                    && (stem.ends_with("だ") || stem.ends_with("です"))
                    && stem != "のだ"
                    && stem != "んだ"
                    && {
                        let rest: String = if stem.ends_with("です") {
                            stem.chars().take(n - 2).collect()
                        } else {
                            stem.chars().take(n - 1).collect()
                        };
                        normalize::normalize_variants(&rest)
                            .iter()
                            .any(|k| index.by_text.contains_key(k))
                    }
            };
            for key in normalize::normalize_variants(stem) {
                if let Some(entries) = index.by_text.get(&key) {
                    for e in entries {
                        if stem_contaminated
                            && match_kind(e, &key) == MatchKind::Reading
                            && priority_score(e) < 900
                        {
                            continue;
                        }
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

    // Structural wrappers resolve through what they wrap: when the whole
    // span came up empty, retry after peeling the piece the tokenizer
    // already marked as structure — an honorific お/ご prefix (おじい ->
    // じい -> 爺, おまたせ -> またせ -> 待つ) or a trailing small-vowel coda
    // (おばぁ -> おば -> 祖母: the ぁ spells the previous mora's vowel, it
    // adds no mora of its own). Both run only when nothing else resolved,
    // so they can only replace "no answer" with an answer. Recursion
    // terminates: the prefix token is consumed by the sub-span's own
    // cursor, and the coda strip removes one trailing small kana per call.
    if candidates.is_empty() {
        // A wrapper strip never justifies crossing a particle: おじい + と
        // must stay おじい | と, since the reduced じいと resolves as
        // 凝乎と and would swallow the と into the span.
        let span_end = position + span_len;
        let crosses_particle = tokens.iter().any(|t| {
            t.start >= position && t.end <= span_end && t.pos == "助詞"
        });
        if !crosses_particle {
            // An honorific お/ご/御 prefix (おじい -> 爺, おまたせ -> 待つ):
            // usually tagged 接頭詞, but sentence-initial お often comes
            // out as 感動詞 (お待たせっ), a noun reading (尾), or worse —
            // either way the prefix adds no meaning of its own, so resolve
            // through the stem. Runs only when nothing else resolved, so it
            // can only replace "no answer" with an answer (おはよう resolves
            // literally and never reaches here). Particles, auxiliaries and
            // bound/unknown pieces are excluded; everything else is tried.
            if let Some(pre) = tokens
                .iter()
                .find(|t| {
                    t.start == position
                        && !matches!(
                            t.pos.as_str(),
                            "助詞" | "助動詞" | "記号" | "接頭辞" | "接尾辞"
                        )
                })
                .filter(|t| matches!(t.surface.as_str(), "お" | "ご" | "御"))
            {
                let plen = pre.surface.chars().count();
                let stem: String = candidate.chars().skip(plen).collect();
                // The prefix's own reading says nothing about the stem, so
                // the sub-span is looked up without reading context.
                if !stem.is_empty() {
                    if let Some((mut sub, sub_label)) =
                        lookup_candidate(&stem, index, decon, None, None, tokens, position + plen)
                    {
                        let label = match sub_label {
                            Some(l) => Some(format!("honorific + {l}")),
                            None => Some("honorific".to_string()),
                        };
                        let key = normalize::normalize_text(&stem);
                        for e in sub.drain(..) {
                            let kind = match_kind(&e, &key);
                            if !candidates.iter().any(|(p, _, _, _, _)| p.id == e.id) {
                                candidates.push((e, 1, label.clone(), kind, false));
                            }
                        }
                    }
                }
            }
            if candidates.is_empty() {
                const SMALL_VOWELS: &[char] = &[
                    'ぁ', 'ぃ', 'ぅ', 'ぇ', 'ぉ', 'ゃ', 'ゅ', 'ょ', 'ゎ', 'ァ', 'ィ', 'ゥ', 'ェ',
                    'ォ', 'ャ', 'ュ', 'ョ', 'ヮ',
                ];
                if candidate.chars().last().is_some_and(|c| SMALL_VOWELS.contains(&c)) {
                    let stem: String = candidate
                        .chars()
                        .take(candidate.chars().count() - 1)
                        .collect();
                    if !stem.is_empty() {
                        if let Some((mut sub, sub_label)) = lookup_candidate(
                            &stem,
                            index,
                            decon,
                            context_reading,
                            morph_base,
                            tokens,
                            position,
                        ) {
                            let label = match sub_label {
                                Some(l) => Some(format!("emphatic + {l}")),
                                None => Some("emphatic".to_string()),
                            };
                            for e in sub.drain(..) {
                                let kind = match_kind(&e, &normalize::normalize_text(&stem));
                                if !candidates.iter().any(|(p, _, _, _, _)| p.id == e.id) {
                                    candidates.push((e, 1, label.clone(), kind, false));
                                }
                            }
                        }
                    }
                }
            }
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
    // For kana surfaces priority decides first. The tokenizer's base form
    // outranks same-kind homophones right after reading context (あり ->
    // 有る over 蟻 when the cursor is the verb stem, 行かせられなかった
    // -> 行く over 生かす), then non-bound entries.
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
        // Freshly-finished たて (したて -> する): the derived noun is a
        // plain reading of its own (したて -> 下手, 仕立て), so kind alone
        // would pick the homophone. Prefer the deconjugated verb — but only
        // over reading/deconjugation matches, never over a real spelling or
        // morphological answer (hovering 仕立て itself must stay 仕立て).
        let is_tate = |c: &(Arc<DictEntry>, usize, Option<String>, MatchKind, bool)| {
            c.2.as_deref().is_some_and(|l| l.contains("right after doing"))
        };
        let spellingish =
            |k: &MatchKind| matches!(k, MatchKind::PrimarySpelling | MatchKind::Spelling | MatchKind::Morphological);
        let a_tate = is_tate(a) && !spellingish(&b.3);
        let b_tate = is_tate(b) && !spellingish(&a.3);
        let ord = b_tate
            .cmp(&a_tate)
            .then(a.3.cmp(&b.3))
            .then(b.4.cmp(&a.4)) // context-match: true first
            .then({
                // Single-kana surfaces are almost always their particle
                // reading (と -> と-particle, not 十): prefer particle
                // entries once kind and reading context tie. Multi-char and
                // kanji surfaces keep existing behavior (どうか/なんで
                // untouched). Word-split, not substring, so participle
                // entries (noun or verb acting prenominally) never match.
                let single_kana = {
                    let ns: Vec<char> =
                        normalize::normalize_text(&candidate).chars().collect();
                    ns.len() == 1 && matches!(ns[0], 'ぁ'..='ん' | 'ァ'..='ン')
                };
                let is_particle = |e: &Arc<DictEntry>| {
                    e.pos.iter().any(|p| {
                        p.split(|c: char| !c.is_alphabetic()).any(|w| w == "particle")
                    })
                };
                let ap = single_kana && is_particle(&a.0);
                let bp = single_kana && is_particle(&b.0);
                bp.cmp(&ap)
            })
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
            })
            .then({
                // Morphological trust (あり -> 有る, not 蟻): when the cursor
                // is a verb token, an entry spelling the tokenizer's base
                // form outranks same-kind homophones — MeCab's analysis
                // beats frequency there. Later tiebreaks (orphans,
                // priority, steps) still order everything else, and other
                // kinds still decide first, so this only ever settles ties
                // like Reading-vs-Reading where one side is the actual
                // inflection (き -> くる over 木, likewise verb-only).
                b_base_match.cmp(&a_base_match)
            });
        if has_kanji {
            ord.then(b_kanji_share.cmp(&a_kanji_share)) // kanji match: true first
                .then(b_prefix.cmp(&a_prefix)) // longest-prefix entry first
                .then(a_orphan.cmp(&b_orphan)) // common word first
                .then(a.1.cmp(&b.1)) // fewest deconj steps first
                .then(b_prio.cmp(&a_prio))
                .then(is_bound_only(&a.0).cmp(&is_bound_only(&b.0))) // false (not bound) sorts before true
        } else {
            // Pure-kana surface: prefer usually-kana words (せい -> 所為,
            // not 性) before falling back to frequency — but only among
            // attested words. Orphans sort last first, or a coincidental
            // kana-only orphan would outrank the common word and trip the
            // obscure-margin splitter below (どうか must stay whole).
            // Kanji surfaces skip this entirely — kanji evidence dominates
            // there, and a kana match against a kanji surface is
            // coincidental by definition.
            let a_kana = a.0.kana_only;
            let b_kana = b.0.kana_only;
            // Pure-kana surface: no kanji evidence, most common word wins.
            ord.then(a_orphan.cmp(&b_orphan)) // common word first
                .then(b_kana.cmp(&a_kana)) // usually-kana entries first
                .then(b_prefix.cmp(&a_prefix)) // longest-prefix entry first
                .then(b_prio.cmp(&a_prio))
                .then(a.1.cmp(&b.1)) // fewest deconj steps first
                .then(is_bound_only(&a.0).cmp(&is_bound_only(&b.0)))
        }
    });

    let deconjugated_from = candidates[0].2.clone();
    let entries: Vec<Arc<DictEntry>> = candidates.into_iter().map(|(e, _, _, _, _)| e).collect();

    Some((entries, deconjugated_from))
}
