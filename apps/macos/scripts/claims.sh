# Every tree on this Mac claims its ports in one folder, so sessions working at once never pick
# the same ones. A claim is a file naming the claimant, a folder of the caller's (`CLAIMANT`), and
# lapses with that folder.
CLAIMS="$HOME/Library/Caches/motile-dev/ports"
mkdir -p "$CLAIMS"

# `$2` if the claimant has it, or else the first port from it on that no one claims and nothing
# has, of TCP's or UDP's. Another port the claimant has is another service's.
claim_port() {
    local port="$2" owner
    while true; do
        owner="$(cat "$CLAIMS/$port" 2>/dev/null || true)"
        [ "$owner" != "$CLAIMANT" ] || [ "$port" != "$2" ] || break
        [ -z "$owner" ] || [ -d "$owner" ] || rm -f "$CLAIMS/$port"
        if ! lsof -nP -i"$1:$port" >/dev/null 2>&1 && (set -C; echo "$CLAIMANT" > "$CLAIMS/$port") 2>/dev/null; then
            break
        fi
        port=$((port + 1))
    done
    echo "$port"
}

# Drops the claimant's claims but on the ports given.
release_ports() {
    local claim
    for claim in "$CLAIMS"/*; do
        case " $* " in *" ${claim##*/} "*) continue ;; esac
        [ "$(cat "$claim" 2>/dev/null)" != "$CLAIMANT" ] || rm -f "$claim"
    done
}
