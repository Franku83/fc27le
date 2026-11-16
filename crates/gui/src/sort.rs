use fc27_game::player_store::PlayerRecord;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    PlayerId,
    Overall,
    Potential,
    Height,
    Weight,
}

/// Precomputed, accent-insensitive search material for one player snapshot.
///
/// Building this once avoids decoding names or lowercasing strings during GUI
/// rendering and per-keystroke filtering.
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    normalized: Vec<String>,
    sort_names: Vec<String>,
}

impl SearchIndex {
    pub fn build(records: &[PlayerRecord]) -> Self {
        let mut normalized = Vec::with_capacity(records.len());
        let mut sort_names = Vec::with_capacity(records.len());
        for record in records {
            let mut haystack = String::new();
            for component in [
                record.display_name.as_deref(),
                record.first_name.as_deref(),
                record.last_name.as_deref(),
                record.common_name.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if !haystack.is_empty() {
                    haystack.push(' ');
                }
                haystack.push_str(component);
            }
            normalized.push(normalize_text(&haystack));
            let sort_name = record
                .display_name
                .as_deref()
                .or(record.common_name.as_deref())
                .map(normalize_text)
                .or_else(|| {
                    let mut parts = Vec::new();
                    if let Some(name) = record.first_name.as_deref() {
                        parts.push(name);
                    }
                    if let Some(name) = record.last_name.as_deref() {
                        parts.push(name);
                    }
                    if parts.is_empty() {
                        None
                    } else {
                        Some(normalize_text(&parts.join(" ")))
                    }
                })
                .unwrap_or_default();
            sort_names.push(sort_name);
        }
        Self {
            normalized,
            sort_names,
        }
    }

    pub fn matches(&self, index: usize, record: &PlayerRecord, query: &str) -> bool {
        let tokens = query_tokens(query);
        if tokens.is_empty() {
            return true;
        }
        let Some(haystack) = self.normalized.get(index) else {
            return false;
        };
        tokens.iter().all(|token| {
            if let Ok(id) = token.parse::<u32>() {
                record.player_id == id
            } else {
                haystack.contains(token)
            }
        })
    }

    pub fn sort_name(&self, index: usize) -> &str {
        self.sort_names.get(index).map(String::as_str).unwrap_or("")
    }
}

/// Normalize text for accent-insensitive, case-insensitive matching.
pub fn normalize_text(input: &str) -> String {
    let decomposed: String = input
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
        .collect();
    let lower = decomposed.to_lowercase();
    let mut cleaned = String::with_capacity(lower.len());
    for character in lower.chars() {
        if character.is_alphanumeric() {
            cleaned.push(character);
        } else if character != '\'' && character != '’' {
            cleaned.push(' ');
        }
    }
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split a query into normalized tokens. Empty input produces no tokens.
pub fn query_tokens(query: &str) -> Vec<String> {
    normalize_text(query)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

pub fn filtered_sorted(
    records: &[PlayerRecord],
    index: &SearchIndex,
    query: &str,
    key: SortKey,
    descending: bool,
) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..records.len())
        .filter(|record_index| index.matches(*record_index, &records[*record_index], query))
        .collect();
    indices.sort_by(|left, right| {
        let a = &records[*left];
        let b = &records[*right];
        let primary = match key {
            SortKey::Name => index.sort_name(*left).cmp(index.sort_name(*right)),
            SortKey::PlayerId => a.player_id.cmp(&b.player_id),
            SortKey::Overall => a.overall_rating.cmp(&b.overall_rating),
            SortKey::Potential => a.potential.cmp(&b.potential),
            SortKey::Height => a.height_cm.cmp(&b.height_cm),
            SortKey::Weight => a.weight_kg.cmp(&b.weight_kg),
        };
        let primary = if descending {
            primary.reverse()
        } else {
            primary
        };
        // Stable, deterministic tie-breaking independent of sort direction.
        primary
            .then_with(|| a.player_id.cmp(&b.player_id))
            .then_with(|| a.record_index.cmp(&b.record_index))
    });
    indices
}

/// Choose the selected row after filtering/sorting.
///
/// The current selection wins if it is still visible. Otherwise prefer the
/// same player ID (for example after a refresh), then fall back to the first
/// filtered row.
pub fn selection_after_filter(
    records: &[PlayerRecord],
    filtered: &[usize],
    current: Option<usize>,
    preferred_player_id: Option<u32>,
) -> Option<usize> {
    if let Some(selected) = current {
        if filtered.contains(&selected) {
            return Some(selected);
        }
    }
    if let Some(player_id) = preferred_player_id {
        if let Some(index) = filtered
            .iter()
            .copied()
            .find(|index| records[*index].player_id == player_id)
        {
            return Some(index);
        }
    }
    filtered.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, name: &str, ovr: u8, pot: u8, height: u8, weight: u8) -> PlayerRecord {
        PlayerRecord {
            record_index: id,
            player_id: id,
            first_name_id: 0,
            last_name_id: 0,
            common_name_id: 0,
            overall_rating: ovr,
            potential: pot,
            height_cm: height,
            weight_kg: weight,
            first_name: None,
            last_name: None,
            common_name: None,
            display_name: Some(name.to_string()),
            name_source: Some("playernames".to_string()),
            name_error: None,
        }
    }

    fn names() -> Vec<PlayerRecord> {
        vec![
            record(20801, "Cristiano Ronaldo", 84, 94, 187, 85),
            record(158023, "Lionel Messi", 88, 94, 169, 67),
            record(231747, "Kylian Mbappé", 91, 94, 182, 81),
            record(215914, "N'Golo Kanté", 82, 90, 168, 70),
        ]
    }

    fn filtered(records: &[PlayerRecord], query: &str) -> Vec<usize> {
        let index = SearchIndex::build(records);
        filtered_sorted(records, &index, query, SortKey::Name, false)
    }

    #[test]
    fn normalizes_accents_and_case() {
        let records = names();
        assert_eq!(filtered(&records, "Kante"), vec![3]);
        assert_eq!(filtered(&records, "kanté"), vec![3]);
        assert_eq!(filtered(&records, "Mbappe"), vec![2]);
        assert_eq!(filtered(&records, "MBAPPÉ"), vec![2]);
        assert_eq!(filtered(&records, "Ngolo Kante"), vec![3]);
    }

    #[test]
    fn supports_multi_token_search_in_any_order() {
        let records = names();
        assert_eq!(filtered(&records, "ronaldo cristiano"), vec![0]);
        assert_eq!(filtered(&records, "cristiano ronaldo"), vec![0]);
        assert_eq!(filtered(&records, "lionel  messi"), vec![1]);
    }

    #[test]
    fn numeric_query_is_exact_player_id() {
        let records = names();
        assert_eq!(filtered(&records, "158023"), vec![1]);
        assert!(filtered(&records, "1580").is_empty());
        assert!(filtered(&records, "not-a-number").is_empty());
    }

    #[test]
    fn empty_query_returns_everyone() {
        let records = names();
        assert_eq!(filtered(&records, "").len(), records.len());
        assert_eq!(filtered(&records, "   ").len(), records.len());
    }

    #[test]
    fn sorting_has_deterministic_tie_breaks() {
        let records = vec![
            record(3, "Same", 80, 90, 180, 75),
            record(1, "Same", 80, 90, 180, 75),
            record(2, "Same", 80, 90, 180, 75),
        ];
        let index = SearchIndex::build(&records);
        assert_eq!(
            filtered_sorted(&records, &index, "", SortKey::Overall, false),
            vec![1, 2, 0]
        );
        assert_eq!(
            filtered_sorted(&records, &index, "", SortKey::Overall, true),
            vec![1, 2, 0]
        );
        assert_eq!(
            filtered_sorted(&records, &index, "", SortKey::Name, true),
            vec![1, 2, 0]
        );
    }

    #[test]
    fn selection_prefers_current_then_player_id_then_first() {
        let records = names();
        let index = SearchIndex::build(&records);
        let filtered = filtered_sorted(&records, &index, "messi", SortKey::Name, false);
        assert_eq!(
            selection_after_filter(&records, &filtered, Some(0), Some(158023)),
            Some(1)
        );
        assert_eq!(
            selection_after_filter(&records, &filtered, Some(1), None),
            Some(1)
        );
        assert_eq!(
            selection_after_filter(&records, &filtered, None, Some(158023)),
            Some(1)
        );
        assert_eq!(
            selection_after_filter(&records, &filtered, None, Some(999_999)),
            Some(1)
        );
    }

    #[test]
    fn name_sort_is_accent_insensitive() {
        let records = vec![
            record(1, "Zidane", 70, 80, 185, 80),
            record(2, "Ábel", 70, 80, 185, 80),
        ];
        let index = SearchIndex::build(&records);
        assert_eq!(
            filtered_sorted(&records, &index, "", SortKey::Name, false),
            vec![1, 0]
        );
    }
}
