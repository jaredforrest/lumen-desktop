def main [atspi: path, gir: path, glib_gir: path, correction: path] {
    let output = $env.out
    mkdir $output
    ^cp -rs $"($atspi)/." $"($output)/"
    ^find $output -type d -exec chmod u+w '{}' +
    ^rm $"($output)/lib/girepository-1.0/Atspi-2.0.typelib"
    mkdir ($output | path join 'share' 'gir-1.0')
    ^nu $correction $gir ($output | path join 'share' 'gir-1.0' 'Atspi-2.0.gir') ($output | path join 'share' 'action-alias-proof.json')
    ^g-ir-compiler $"--includedir=($glib_gir)" ($output | path join 'share' 'gir-1.0' 'Atspi-2.0.gir') $"--output=($output)/lib/girepository-1.0/Atspi-2.0.typelib"
}
