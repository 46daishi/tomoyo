use crate::normalize;
use crate::rank::priority_score;
use crate::types::DictEntry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub(crate) struct DictionaryIndex {
    pub(crate) by_text: HashMap<String, Vec<Arc<DictEntry>>>,
    pub(crate) by_id: HashMap<u32, Arc<DictEntry>>,
    // Maps each bigram (and, for single-character text, each unigram) to
    // the set of entry ids that contain it somewhere in a spelling or
    // reading. Used to narrow "contains" searches to a small candidate
    // set instead of scanning every entry.
    pub(crate) by_bigram: HashMap<String, HashSet<u32>>,
}

pub(crate) struct DictState(pub(crate) DictionaryIndex);

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
    pub(crate) fn build(entries: Vec<DictEntry>) -> Self {
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

pub(crate) fn find_containing(query: &str, index: &DictionaryIndex, limit: usize) -> Vec<Arc<DictEntry>> {
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
    
    // Exact matches first, then newspaper priority, then the VN corpus rank
    // (same tiebreak as the main results, so a ranked containing-word like
    // 日陰者 #47163 outranks unlisted ones), then id: the candidate set is a
    // HashSet, so without a total order the related list would shuffle
    // between lookups.
    let rank_or_last = |e: &Arc<DictEntry>| if e.freq_rank == 0 { u32::MAX } else { e.freq_rank };
    results.sort_by(|(a, a_exact), (b, b_exact)| {
        b_exact.cmp(a_exact)
            .then(priority_score(b).cmp(&priority_score(a)))
            .then(rank_or_last(a).cmp(&rank_or_last(b)))
            .then(a.id.cmp(&b.id))
    });
    results.truncate(limit);
    
    results.into_iter().map(|(e, _)| e).collect()
}
