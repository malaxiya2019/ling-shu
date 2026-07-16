//! Tokenizer — 中文友好的文本分词器。
//!
//! 使用字符 n-gram 来处理中英文混合文本。
//! 英文按词分割，中文按字符 + bi-gram 分割。

use std::collections::HashSet;

/// 对文本进行分词，返回词项及其权重。
/// - 英文单词保持原样
/// - 数字保持
/// - 中文拆为单字 + 双字组合（bi-gram）
///
/// 参考信息检索中的"Chinese Segmentation"标准做法：
/// 对于没有专用分词器的场景，字符 n-gram 是最实用的折中方案。
pub fn tokenize(text: &str) -> Vec<String> {
    let text = text.to_lowercase();
    let mut tokens = Vec::new();

    // 1. 提取英文单词和数字
    let mut current_word = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            current_word.push(ch);
        } else {
            if !current_word.is_empty() {
                tokens.push(current_word.clone());
                current_word.clear();
            }
        }
    }
    if !current_word.is_empty() {
        tokens.push(current_word);
    }

    // 2. 提取中文字符
    let chinese_chars: Vec<char> = text.chars().filter(|c| is_chinese_char(*c)).collect();

    // 2a. 单字
    for &ch in &chinese_chars {
        tokens.push(ch.to_string());
    }

    // 2b. 双字组合 (bi-gram)
    for window in chinese_chars.windows(2) {
        let bigram: String = window.iter().collect();
        tokens.push(bigram);
    }

    tokens
}

/// 判断是否为中文字符（CJK统一表意文字区间）。
pub fn is_chinese_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |     // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |     // CJK Extension A
        '\u{F900}'..='\u{FAFF}' |     // Compatibility Ideographs
        '\u{2F800}'..='\u{2FA1F}'     // CJK Extension C
    )
}

/// 对词项进行去重和频率统计。
pub fn tokenize_with_freq(text: &str) -> Vec<(String, usize)> {
    let tokens = tokenize(text);
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for token in tokens {
        *freq.entry(token).or_insert(0) += 1;
    }
    let mut result: Vec<(String, usize)> = freq.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1)); // 按频率降序
    result
}

/// 获取唯一的词项集合。
pub fn unique_tokens(text: &str) -> HashSet<String> {
    tokenize(text).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_english_words() {
        let tokens = tokenize("hello world");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_chinese_chars() {
        let tokens = tokenize("项目A暂停");
        // "项目A暂停" → 英文 A, 中文: 项,目,暂,停, 项目, 目暂, 暂停
        assert!(tokens.contains(&"a".to_string()), "should extract 'a', got {:?}", tokens);
        assert!(tokens.contains(&"项".to_string()), "should have 项");
        assert!(tokens.contains(&"暂停".to_string()), "should have 暂停 bigram");
    }

    #[test]
    fn test_mixed_text() {
        let tokens = tokenize("项目A为什么暂停");
        assert!(tokens.contains(&"a".to_string()));
        assert!(tokens.contains(&"项目".to_string()), "should contain 项目 bigram");
    }

    #[test]
    fn test_tokenize_with_freq() {
        let result = tokenize_with_freq("test test foo");
        let test_freq = result.iter().find(|(t, _)| t == "test").map(|(_, f)| *f);
        assert_eq!(test_freq, Some(2));
    }

    #[test]
    fn test_unique_tokens() {
        let tokens = unique_tokens("abc abc def");
        assert_eq!(tokens.len(), 2);
        assert!(tokens.contains("abc"));
        assert!(tokens.contains("def"));
    }

    #[test]
    fn test_is_chinese_char() {
        assert!(is_chinese_char('中'));
        assert!(is_chinese_char('文'));
        assert!(!is_chinese_char('a'));
        assert!(!is_chinese_char('1'));
    }

    #[test]
    fn test_empty_text() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_numbers() {
        let tokens = tokenize("v1.0.0");
        // v100 should be extracted as "v100"... wait
        // Actually with the current implementation, "." is a separator
        // so "v", "1", "0", "0" are separate tokens
        assert!(tokens.contains(&"v1".to_string()));
        assert!(tokens.contains(&"0".to_string()));
    }

    #[test]
    fn test_chinese_english_mix() {
        // "RAG技术" → RAG, 技,术, 技术
        let tokens = tokenize("RAG技术");
        assert!(tokens.contains(&"rag".to_string()));
        assert!(tokens.contains(&"技术".to_string()));
    }
}
