# Verify that excluding the unused scanner warning fixture cannot hide a binding test.
def fail [message: string] {
  error make { msg: $message }
}

def main [source: path] {
  let root = ($source | path expand)
  let project = (open --raw ($root | path join 'subprojects/gobject-introspection-tests/meson.build'))
  for item in [
    { target: libwarnlib, constructor: library }
    { target: warnlib_gir, constructor: 'gnome.generate_gir' }
  ] {
    let constructor = ($item.constructor | str replace --all '.' '\.')
    let pattern = (['(?ms)^' $item.target ' = ' $constructor '\((?<body>.*?)^\)'] | str join)
    let blocks = ($project | parse --regex $pattern)
    if ($blocks | length) != 1 { fail 'Review standalone warning-fixture target changes' }
    let count = (($blocks | get 0.body | split row 'build_by_default: false' | length) - 1)
    if $count != 1 { fail 'Review standalone warning-fixture target changes' }
  }

  let bindings = (open --raw ($root | path join 'tests/meson.build'))
  let dependencies = ($bindings | parse --regex '(?ms)^test_deps = \[(?<body>.*?)\]')
  if ($dependencies | length) != 1 { fail 'Review binding-test dependency declaration changes' }
  let body = ($dependencies | get 0.body)
  for name in [gimarshallingtests_typelib regress_typelib utility_typelib] {
    if not ($body | str contains $name) { fail 'Required binding-test typelib removed' }
  }

  let pattern = ($root | path join 'tests' '**' '*')
  for path in (glob $pattern) {
    let info = ($path | path parse)
    if ($info.extension in [py c h build]) or ($info.stem == 'meson' and $info.extension == 'build') {
      if ((open --raw $path | str lowercase) | str contains 'warnlib') {
        fail 'WarnLib is now used by binding tests; review default target exclusion'
      }
    }
  }
  let options = (open --raw ($root | path join 'meson_options.txt'))
  if not ($options | str contains "option('tests', type : 'boolean', value : true") {
    fail 'Upstream tests must remain enabled by default'
  }
  print 'Verified standalone scanner fixture remains explicitly buildable; binding test dependencies unchanged'
}
