use std::io;

fn main() -> io::Result<()> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    println!(
        "{{\"configured\":false,\"events\":[],\"monthEvents\":[],\"days\":[],\"total\":0,\"providers\":[]}}"
    );
    Ok(())
}
