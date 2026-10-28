def main [source: path] {
    let destination = ($env.out | path join share fonts truetype)
    mkdir $destination
    ^cp ($source | path join fonts InterludeVariable.ttf) ($destination | path join InterludeVariable.ttf)
}
