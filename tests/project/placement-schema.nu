def evaluate [schema: path, nixpkgs: path, values: string] {
  let expression = ('let pkgs = import ' + ($nixpkgs | path expand) + ' { }; in import ' + ($schema | path expand) + ' { inherit (pkgs) lib; values = ' + $values + '; }')
  let result = (do { ^nix-instantiate --eval --strict --json --expr $expression } | complete)
  $result
}

def require [condition: bool, message: string] {
  if not $condition { error make { msg: $message } }
}

def main [schema: path, nixpkgs: path] {
  let defaults = (evaluate $schema $nixpkgs '{ }')
  require ($defaults.exit_code == 0) $'Default settings failed evaluation: ($defaults.stderr)'
  let base = ($defaults.stdout | from json)
  require ($base.windows.rememberOwnedGeometry == false) 'Owned geometry must be disabled by default'

  let enabled = (evaluate $schema $nixpkgs '{ windows.rememberOwnedGeometry = true; }')
  require ($enabled.exit_code == 0) $'Owned geometry opt-in failed evaluation: ($enabled.stderr)'
  let opted = ($enabled.stdout | from json)
  require ($opted.windows.rememberOwnedGeometry == true) 'Owned geometry opt-in was lost'
  require ($opted.windows.tileGap == 8) 'An unrelated window default changed'

  let edited = (evaluate $schema $nixpkgs '{ windows = { rememberOwnedGeometry = true; tileGap = 12; }; }')
  require ($edited.exit_code == 0) $'Combined placement settings failed evaluation: ($edited.stderr)'
  let merged = ($edited.stdout | from json)
  require ($merged.windows.rememberOwnedGeometry == true and $merged.windows.tileGap == 12) 'An unrelated window edit lost the opt-in'

  for invalid in ['1' '"true"' 'null'] {
    let rejected = (evaluate $schema $nixpkgs $'{ windows.rememberOwnedGeometry = ($invalid); }')
    require ($rejected.exit_code != 0) $'Invalid owned geometry value was accepted: ($invalid)'
  }
}
