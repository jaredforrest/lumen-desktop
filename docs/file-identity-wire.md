# File identity wire format

Local directory rows carry `identity: {device, inode}` with both values encoded
as canonical decimal strings from `0` through `18446744073709551615`. No leading
zeros, signs, whitespace, fractional values or exponent notation are accepted.
Native values are encoded before JSON reaches JavaScript. Converting an already
rounded number to a string cannot recover its identity.

Preview selection guards, preview `sourceIdentity`, the device/inode fields of
`sourceFacts`, and colour-tag selection guards use this same two-field format.
The backend accepts old integer inputs only through the JavaScript safe range
`0..9007199254740991`; larger numeric inputs fail closed. Existing small-number
callers therefore continue to work without admitting lossy identities. Preview
resource tokens and the separate six-field captured-action facts retain their
existing contracts.

Runtime history normalizes safe legacy values to strings, matches exact paths
and identities, and follows a renamed inode only when exactly one candidate
exists. A replaced path, ambiguous hard links, or invalid/unsafe identity never
degrades to a path-only selection. Desktop placement keys preserve exact decimal
identities and distinguish actual hard links by their existing name suffix.

`test_file_identity.py` exercises real listing serialization through a JavaScript
JSON round-trip, exact original/replacement preview guards, canonical boundaries,
legacy compatibility, tag writes guarded before mutation, and both shared
JavaScript models. Synthetic stat values above `2^60` make the precision defect
deterministic without changing filesystem or host state. The ordinary native
Files history and owned-preview fixtures cover their actual UI consumers.
