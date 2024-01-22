use trie_rs::Trie;

pub trait TrieUtils {
    type Word;
    fn collect_matches(&self, prefix: &Self::Word) -> Vec<Self::Word>;
}

impl TrieUtils for Trie<u8> {
    type Word = String;

    fn collect_matches(&self, prefix: &Self::Word) -> Vec<Self::Word> {
        let results_u8: Vec<Vec<u8>> = self.predictive_search(prefix.as_str());
        let results: Vec<String> = results_u8
            .iter()
            .map(|u8s| String::from_utf8(u8s.clone()).unwrap())
            .collect();
        results
    }
}
