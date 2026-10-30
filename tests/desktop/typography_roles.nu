#!/usr/bin/env nu
let shell = ($env.FILE_PWD | path join .. .. res lumen-desktop shell | path expand)
let theme = (open --raw ($shell | path join Theme.qml))
let matches = ($theme | parse --regex 'typography:\s*\(\{(?<values>[^}]+)\}\)')
if ($matches | length) != 1 { error make {msg: 'The named typography roles are missing'} }
let actual = ($matches | first | get values | parse --regex '(?<key>\w+):(?<size>\d+)' | reduce --fold {} {|row, acc| $acc | upsert $row.key ($row.size | into int)})
let expected = {caption: 10, metadata: 11, label: 12, body: 13, subheading: 15, title: 16, dialogHeading: 17, heading: 18, display: 29}
if $actual != $expected { error make {msg: 'The default typography role sizes changed'} }
if not ($theme | str contains 'return textSize(typography[role]') { error make {msg: 'Role scaling is missing'} }
if not ((open --raw ($shell | path join PanelLabel.qml)) | str contains 'font.pixelSize: Theme.textRole(textRole)') {
    error make {msg: 'Panel labels bypass the shared text roles'}
}
for name in [FilesWindow.qml CalendarWindow.qml ColumnView.qml] {
    let source = (open --raw ($shell | path join $name) | str replace 'font.pixelSize:10/* fixed mark symbol */' '' | str replace 'text:FileLinks.badge(modelData);font.pixelSize:16' 'text:FileLinks.badge(modelData);')
    if (($source | parse --regex 'font\.pixelSize:\s*\d') | length) > 0 {
        error make {msg: $'Ordinary text in ($name) bypasses named roles'}
    }
}
let calendar = (open --raw ($shell | path join CalendarWindow.qml))
for requirement in ['contentWidth:main.width;contentHeight:main.height' 'Math.max(calendarViewport.availableWidth,(root.view==="Day"?160:420)*Theme.textScale)' 'onActiveFocusItemChanged()'] {
    if not ($calendar | str contains $requirement) { error make {msg: $'Calendar scrolling requirement missing: ($requirement)'} }
}
for name in [FileInfoPanel TextToolsWindow RemoteDesktopWindow] {
    let source = (open --raw ($shell | path join $'($name).qml'))
    if (($source | parse --regex 'font\.pixelSize\s*:\s*\d') | length) > 0 {
        error make {msg: $'Literal font size remains in ($name)'}
    }
}
print 'PASS named typography roles, scaling, scrolling, and utility consumers'
