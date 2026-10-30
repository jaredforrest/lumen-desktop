//! Bounded deterministic peer for actual FilesState job/list transport ordering.
use std::io::{self, BufRead, Write};
fn main() {
    let mut listing = 0;
    for line in io::stdin().lock().lines() {
        let line = line.expect("fixture stdin");
        assert!(line.len() < 4096, "bounded fixture request");
        let output = if line.contains("\"action\":\"fixture-warning\"") {
            "{\"jobs\":[{\"id\":\"live-copy\",\"state\":\"complete\",\"operation\":\"copy\",\"started\":9999999999999,\"errors\":[],\"undoWarning\":\"This copy has no Undo; earlier named history remains\"}],\"canUndo\":false}".to_string()
        } else if line.contains("\"action\":\"list\"") {
            let id = line
                .split("\"id\":")
                .nth(1)
                .expect("request ID")
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .expect("numeric request ID");
            listing += 1;
            format!(
                "{{\"id\":{id},\"action\":\"list\",\"path\":\"/fixture\",\"parent\":\"/\",\"rows\":[],\"breadcrumbs\":[],\"writable\":true,\"limited\":{},\"unreadable\":false}}",
                listing == 2
            )
        } else {
            continue;
        };
        println!("{output}");
        io::stdout().flush().expect("fixture output");
    }
}
