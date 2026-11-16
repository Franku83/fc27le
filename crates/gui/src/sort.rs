use fc27_game::player_store::PlayerRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    PlayerId,
    Overall,
    Potential,
    Height,
    Weight,
}

pub fn matches(record: &PlayerRecord, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    if query.chars().all(|character| character.is_ascii_digit())
        && record.player_id.to_string().contains(&query)
    {
        return true;
    }
    let mut haystack = String::new();
    if let Some(name) = &record.display_name {
        haystack.push_str(name);
    }
    if let Some(name) = &record.first_name {
        haystack.push(' ');
        haystack.push_str(name);
    }
    if let Some(name) = &record.last_name {
        haystack.push(' ');
        haystack.push_str(name);
    }
    if let Some(name) = &record.common_name {
        haystack.push(' ');
        haystack.push_str(name);
    }
    haystack.to_lowercase().contains(&query)
}

pub fn filtered_sorted(
    records: &[PlayerRecord],
    query: &str,
    key: SortKey,
    descending: bool,
) -> Vec<usize> {
    let mut indices: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, record)| matches(record, query))
        .map(|(index, _)| index)
        .collect();
    indices.sort_by(|left, right| {
        let a = &records[*left];
        let b = &records[*right];
        let ordering = match key {
            SortKey::Name => name(a).cmp(&name(b)),
            SortKey::PlayerId => a.player_id.cmp(&b.player_id),
            SortKey::Overall => a.overall_rating.cmp(&b.overall_rating),
            SortKey::Potential => a.potential.cmp(&b.potential),
            SortKey::Height => a.height_cm.cmp(&b.height_cm),
            SortKey::Weight => a.weight_kg.cmp(&b.weight_kg),
        };
        let ordering = ordering.then_with(|| a.player_id.cmp(&b.player_id));
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    indices
}

fn name(record: &PlayerRecord) -> String {
    record
        .display_name
        .as_deref()
        .or(record.common_name.as_deref())
        .unwrap_or("")
        .to_lowercase()
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

    #[test]
    fn filters_by_name_and_id() {
        let records = vec![
            record(20801, "Cristiano Ronaldo", 84, 94, 187, 85),
            record(158023, "Lionel Messi", 88, 94, 169, 67),
            record(231747, "Kylian Mbappé", 91, 94, 182, 81),
        ];
        assert_eq!(
            filtered_sorted(&records, "messi", SortKey::Name, false),
            vec![1]
        );
        assert_eq!(
            filtered_sorted(&records, "158", SortKey::Name, false),
            vec![1]
        );
        assert_eq!(
            filtered_sorted(&records, "ronaldo", SortKey::Name, false),
            vec![0]
        );
        assert_eq!(filtered_sorted(&records, "", SortKey::Name, false).len(), 3);
    }

    #[test]
    fn sorts_numeric_columns_both_directions() {
        let records = vec![
            record(1, "A", 70, 80, 170, 65),
            record(2, "B", 90, 91, 190, 85),
            record(3, "C", 80, 95, 180, 75),
        ];
        assert_eq!(
            filtered_sorted(&records, "", SortKey::Overall, false),
            vec![0, 2, 1]
        );
        assert_eq!(
            filtered_sorted(&records, "", SortKey::Overall, true),
            vec![1, 2, 0]
        );
        assert_eq!(
            filtered_sorted(&records, "", SortKey::Height, true),
            vec![1, 2, 0]
        );
        assert_eq!(
            filtered_sorted(&records, "", SortKey::Potential, true),
            vec![2, 1, 0]
        );
    }
}
