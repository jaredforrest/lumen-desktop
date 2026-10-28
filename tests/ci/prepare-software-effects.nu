# The driver can be built in even when modules.builtin does not list it.
def diagnostic [program: string, ...arguments: string] {
    let result = (run-external $program ...$arguments | complete)
    $result.stdout + $result.stderr
}

def present [device: path] {
    (^test -c $device | complete).exit_code == 0
}

def main [artifacts: path, --device: path = /dev/udmabuf] {
    mkdir $artifacts
    let kernel = (^uname -r | str trim)
    let diagnostics = [
        (diagnostic uname -- -a)
        (diagnostic ls -- -l $device /sys/class/misc/udmabuf)
        (diagnostic grep -- -E 'CONFIG_(UDMABUF|MEMFD_CREATE|DMA_SHARED_BUFFER)=' $'/boot/config-($kernel)')
        (diagnostic grep -- -i udmabuf $'/lib/modules/($kernel)/modules.builtin')
    ]
    ($diagnostics | str join '') | save --force ($artifacts | path join kernel.txt)
    if not (present $device) {
        let attempt = (^sudo modprobe udmabuf | complete)
        ($attempt.stdout + $attempt.stderr) | save --force ($artifacts | path join modprobe.log)
        ^sudo udevadm settle
    }
    if not (present $device) {
        error make {msg: '::error::Software effects require /dev/udmabuf; see kernel diagnostics. No effects acceptance was skipped.'}
    }
    let owner = $'(^id -u | str trim):(^id -g | str trim)'
    ^sudo chown $owner $device
    ^sudo chmod '0600' $device
}
