# Automatic local time and appearance

Import `modules/location.nix` and enable `services.lumen-location.enable` on a
travelling host. It makes the system timezone mutable and refreshes approximate
network location after boot and every fifteen minutes, including after resume.
NTP keeps the underlying clock in UTC; only the displayed timezone changes.

The HTTPS provider is https://ipwhois.io/documentation (`https://ipwho.is/`).
It receives the public IP associated with the request. Only the latest city,
coordinates, timezone and refresh time are retained in
`/var/lib/lumen-location/current.json`; no IP address or history is stored.
Failed lookups retain the last successful location. Routes through WireGuard,
TUN, Mullvad or Tailscale retain the last location instead of adopting an exit
server's location. IP geolocation is approximate; an undetected proxy or VPN
can still report the wrong place.

The existing appearance timer checks every minute. Auto mode uses the cached
location for sunrise/sunset, including polar day/night. Explicit light, dark and
black choices remain explicit. The menu clock consumes the current UTC offset
from appearance state so travel does not require restarting applications.
Without a cached location, solar appearance retains its London fallback.

Check `systemctl status lumen-location.timer`,
`journalctl -u lumen-location.service` and `timedatectl status`.
