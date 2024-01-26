use clap::Command;
use trie_rs::{Trie, TrieBuilder};

/// Trait to add $create related functionally to Trie.
pub trait TrieUtils {
    /// Word is the type of collected characters from Trie<T>
    /// Example: Trie<u8> -> Word=String
    type Word;

    /// Get owned collection of matching words for a given prefix from the Trie
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

/// Return a prefix tree of commands based on Command app created with Clap.
/// One can use the command trie to make command predictions.
pub fn get_command_trie(cmd_app: &Command) -> Trie<u8> {
    let mut builder = TrieBuilder::new();
    for cmd in cmd_app.get_subcommands() {
        let name = cmd.get_name();
        builder.push(name);
        for alias in cmd.get_all_aliases() {
            builder.push(alias);
        }
    }
    builder.build()
}

/// Return a vector of commands based on Command app created with Clap.
/// One can use the command vector to list all possible commands.
pub fn get_visible_command_vector(cmd_app: &Command) -> Vec<String> {
    let mut vec = Vec::new();
    for cmd in cmd_app.get_subcommands() {
        let name = cmd.get_name().to_string();
        vec.push(name);
        for alias in cmd.get_visible_aliases() {
            vec.push(alias.to_string());
        }
    }
    vec
}
