#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub position: i32,
    pub char_start: i32,
    pub char_end: i32,
    pub content: String,
}

pub fn chunk_text(input: &str, target_chars: usize, overlap_chars: usize) -> Vec<Chunk> {
    if input.trim().is_empty() {
        return Vec::new();
    }

    let chars: Vec<char> = input.chars().collect();
    let target = target_chars.max(128);
    let overlap = overlap_chars.min(target / 2);
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut position = 0i32;

    while start < chars.len() {
        let hard_end = (start + target).min(chars.len());
        let mut end = hard_end;

        if hard_end < chars.len() {
            let search_start = start + target / 2;
            if let Some(offset) = chars[search_start..hard_end]
                .iter()
                .rposition(|c| matches!(c, '\n' | '。' | '！' | '？' | '.' | '!' | '?'))
            {
                end = search_start + offset + 1;
            }
        }

        let content: String = chars[start..end]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if !content.is_empty() {
            chunks.push(Chunk {
                position,
                char_start: start as i32,
                char_end: end as i32,
                content,
            });
            position += 1;
        }

        if end >= chars.len() {
            break;
        }
        start = end.saturating_sub(overlap).max(start + 1);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_is_deterministic_and_overlapping() {
        let text = "a".repeat(400);
        let first = chunk_text(&text, 128, 16);
        let second = chunk_text(&text, 128, 16);
        assert_eq!(first, second);
        assert!(first.len() > 1);
        assert!(first[1].char_start < first[0].char_end);
    }

    #[test]
    fn empty_input_has_no_chunks() {
        assert!(chunk_text("   ", 128, 16).is_empty());
    }
}
