def main [binary: path, icons: path, catalogue: path, license: path, bridge: path, palette: path, fontconfig: path, font: path, font_family: string, nu_binary: path, runtime_path: string] {
    let shared = ($env.out | path join share character-viewer)
    let bin = ($env.out | path join bin)
    mkdir $shared
    mkdir $bin
    ^cp -r $icons ($shared | path join icons)
    ^cp $catalogue ($shared | path join emoji-test.txt)
    ^cp $license ($shared | path join UNICODE-LICENSE.txt)
    for name in [character-viewer-runtime lumen-character-viewer] {
        let catalogue_arg = if $name == 'lumen-character-viewer' { $'"($shared | path join emoji-test.txt)"' } else { '' }
        let path_line = '$env.PATH = (' + (($runtime_path | split row ':' | to nuon)) + ' ++ $env.PATH)'
        let family_literal = ($font_family | to nuon)
        let script = $'#!($nu_binary)
def --wrapped main [...args] {
  ($path_line)
  $env.GDK_BACKEND = "wayland"
  $env.CHARACTER_VIEWER_BRIDGE = "($bridge)"
  $env.CHARACTER_VIEWER_ICONS = "($shared | path join icons)"
  $env.CHARACTER_VIEWER_PALETTE = "($palette)"
  $env.CHARACTER_VIEWER_FONTCONFIG = "($fontconfig)"
  $env.CHARACTER_VIEWER_FONT = "($font)"
  $env.LUMEN_EMOJI_FONT_FAMILY = ($family_literal)
  exec ($binary) ($catalogue_arg) ...$args
}
'
        let destination = ($bin | path join $name)
        $script | save --raw $destination
        ^chmod +x $destination
    }
}
