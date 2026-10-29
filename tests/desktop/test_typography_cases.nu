#!/usr/bin/env nu
use typography_cases.nu *

let batches = (segments)
if ($batches | length) != 8 { error make {msg: 'Typography corpus must have eight bounded segments'} }
mut rows = []
for batch in $batches {
    let segment = (cases $batch.consumer $batch.width $batch.mode)
    if ($segment | length) != 16 { error make {msg: $'Typography segment has ($segment | length) cases'} }
    $rows = ($rows | append $segment)
}
let actual = ($rows | each {|row| identity $row })
if ($actual | length) != 128 or ($actual | uniq | length) != 128 { error make {msg: 'Typography corpus has omitted or duplicate cases'} }
let selected = (segments Calendar 1060 dark)
if $selected != [{consumer: Calendar, width: 1060, mode: dark}] { error make {msg: 'Selected typography segment is wrong'} }
for filter in [
    {consumer: Other, width: 480, mode: light}
    {consumer: Files, width: 999, mode: light}
    {consumer: Files, width: 480, mode: other}
] {
    let result = (try { segments $filter.consumer $filter.width $filter.mode; false } catch { true })
    if not $result { error make {msg: 'Invalid typography filter was accepted'} }
}
print 'PASS eight bounded segments preserve all 128 unique accepted typography cases and reject invalid filters'
