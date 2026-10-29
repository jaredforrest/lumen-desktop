export def segments [consumer?: string, width?: int, mode?: string] {
    if $consumer != null and $consumer not-in [Files Calendar] { error make {msg: 'Unsupported consumer'} }
    if $width != null and $width not-in [480 1060] { error make {msg: 'Unsupported window size'} }
    if $mode != null and $mode not-in [light dark] { error make {msg: 'Unsupported colour mode'} }
    mut rows = []
    for app in [Files Calendar] {
        if $consumer != null and $app != $consumer { continue }
        for size in [480 1060] {
            if $width != null and $size != $width { continue }
            for color in [light dark] {
                if $mode != null and $color != $mode { continue }
                $rows = ($rows | append {consumer: $app, width: $size, mode: $color})
            }
        }
    }
    $rows
}

export def cases [consumer: string, width: int, mode: string] {
    let selected = (segments $consumer $width $mode)
    if ($selected | length) != 1 { error make {msg: 'A complete segment is required'} }
    let views = (if $consumer == Files { [list grid columns gallery] } else { [Month Year Day Week] })
    let height = (if $width == 480 { 320 } else { 720 })
    mut rows = []
    for scale in [1.0 1.15 1.3 1.5] {
        for view in $views {
            $rows = ($rows | append {app: $consumer, width: $width, height: $height, mode: $mode, scale: $scale, view: $view})
        }
    }
    $rows
}

export def identity [case: record] {
    [$case.app $case.width $case.height $case.mode $case.scale $case.view] | to json --raw
}
