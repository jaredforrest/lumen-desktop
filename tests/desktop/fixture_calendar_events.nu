def main [root: path] {
    let calendars = [
        {name: Personal, events: [
            {uid: all-day, fields: "DTSTART;VALUE=DATE:20261031\nDTEND;VALUE=DATE:20261102\nSUMMARY:Two-day all-day"}
            {uid: old-day, fields: "DTSTART:20261031T220000Z\nDTEND:20261031T230000Z\nSUMMARY:Previous day only"}
        ]}
        {name: Travel, events: [
            {uid: west, fields: "DTSTART;TZID=America/Los_Angeles:20261031T183000\nDTEND;TZID=America/Los_Angeles:20261031T200000\nSUMMARY:West overlap"}
            {uid: east, fields: "DTSTART;TZID=America/New_York:20261031T220000\nDTEND;TZID=America/New_York:20261031T230000\nSUMMARY:East overlap"}
            {uid: utc, fields: "DTSTART:20261101T030000Z\nDTEND:20261101T040000Z\nSUMMARY:Later UTC"}
        ]}
    ]
    for calendar in $calendars {
        let folder = ($root | path join icloud $calendar.name)
        mkdir $folder
        $calendar.name | save --raw ($folder | path join displayname)
        for event in $calendar.events {
            $"BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Fixture//EN\nBEGIN:VEVENT\nUID:($event.uid)\n($event.fields)\nEND:VEVENT\nEND:VCALENDAR\n"
                | save --raw ($folder | path join $"($event.uid).ics")
        }
    }
    let settings = ($root | path join icloud-settings)
    mkdir $settings
    'fixture only' | save --raw ($settings | path join vdirsyncer.conf)
    '{"lastSuccess":"2026-10-31T12:00:00Z"}' | save --raw ($settings | path join status.json)
}
