use std::io::{self, BufRead};

fn main() {
    for line in io::stdin().lock().lines() {
        if line.is_err() {
            break;
        }
    }
}
